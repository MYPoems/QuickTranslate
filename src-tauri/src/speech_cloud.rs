//! Qwen realtime PCM streaming. Dedicated key, TLS-only allowlisted endpoints,
//! reusable sessions, explicit cancellation, bounded memory; no silent fallback.
use crate::{errors::AppError, speech::SpeechPreferences, speech_cache::AudioCache};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, sync::Mutex as AsyncMutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
use tokio_util::sync::CancellationToken;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const MAX_AUDIO: usize = 16 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(300);
static EVENT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechPacket {
    pub kind: &'static str,
    pub audio: String,
    pub sample_rate: u32,
    pub voice_name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechTiming {
    pub first_audio_ms: u128,
    pub total_ms: u128,
    pub cached: bool,
}
struct Session {
    socket: Socket,
    key: String,
    used: Instant,
}
pub struct CloudSpeech {
    session: AsyncMutex<Option<Session>>,
    cancel: Mutex<CancellationToken>,
    cache: Mutex<AudioCache<Vec<u8>>>,
    #[cfg(test)]
    test_endpoint: Option<String>,
}
impl Default for CloudSpeech {
    fn default() -> Self {
        Self {
            session: AsyncMutex::new(None),
            cancel: Mutex::new(CancellationToken::new()),
            cache: Mutex::new(AudioCache::new(24 * 1024 * 1024)),
            #[cfg(test)]
            test_endpoint: None,
        }
    }
}
pub fn validate_endpoint(value: &str) -> Result<(), AppError> {
    let url = reqwest::Url::parse(value).map_err(|_| fail("云端朗读 WebSocket 地址无效"))?;
    if url.scheme() != "wss"
        || !matches!(
            url.host_str(),
            Some("dashscope.aliyuncs.com" | "dashscope-intl.aliyuncs.com")
        )
        || url.path() != "/api-ws/v1/realtime"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(fail(
            "云端朗读仅支持阿里云北京/新加坡官方 WSS 地址，请选择对应地域",
        ));
    }
    Ok(())
}
fn fail(message: &str) -> AppError {
    AppError::Speech(message.into())
}
fn service_error(code: &str) -> AppError {
    let code = code.to_ascii_lowercase();
    if code.contains("auth")
        || code.contains("api_key")
        || code.contains("apikey")
        || code.contains("401")
        || code.contains("403")
    {
        fail("云端朗读认证失败，请检查 API Key、调用地域和模型权限")
    } else if code.contains("quota") || code.contains("balance") {
        fail("云端朗读额度不足，请检查阿里云账户余额与额度")
    } else if code.contains("rate") || code.contains("429") {
        fail("云端朗读请求过于频繁，请稍后重试")
    } else {
        fail("云端朗读服务返回错误，请检查模型、音色、权限和额度，或手动切换离线朗读")
    }
}
async fn send(socket: &mut Socket, value: Value) -> Result<(), AppError> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|_| fail("云端朗读连接已断开，请重试"))
}
async fn receive(socket: &mut Socket) -> Result<Value, AppError> {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .map_err(|_| fail("云端朗读响应超时，请检查网络或手动切换离线朗读"))?
            .ok_or_else(|| fail("云端朗读连接提前关闭"))?
            .map_err(|_| fail("云端朗读网络异常，请重试"))?;
        match message {
            Message::Text(text) => {
                let value: Value =
                    serde_json::from_str(&text).map_err(|_| fail("云端语音响应格式无效"))?;
                if value["type"] == "error" {
                    return Err(service_error(
                        value["error"]["code"].as_str().unwrap_or("unknown"),
                    ));
                }
                return Ok(value);
            }
            Message::Ping(bytes) => {
                socket
                    .send(Message::Pong(bytes))
                    .await
                    .map_err(|_| fail("云端连接中断"))?;
            }
            Message::Close(_) => return Err(fail("云端朗读连接提前关闭")),
            _ => (),
        }
    }
}
impl CloudSpeech {
    pub fn stop(&self) {
        if let Ok(mut token) = self.cancel.lock() {
            token.cancel();
            *token = CancellationToken::new();
        }
    }
    pub async fn release_idle(&self) {
        let mut session = self.session.lock().await;
        if session.as_ref().is_some_and(|s| s.used.elapsed() >= IDLE) {
            *session = None;
        }
    }
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
    pub async fn speak(
        &self,
        text: &str,
        language: &str,
        settings: &SpeechPreferences,
        api_key: &str,
        publish: impl Fn(SpeechPacket) -> Result<(), AppError>,
    ) -> Result<SpeechTiming, AppError> {
        settings.validate()?;
        if text.trim().is_empty() || text.chars().count() > 600 || !matches!(language, "zh" | "en")
        {
            return Err(fail("朗读文本或语言无效"));
        }
        if api_key.trim().is_empty() {
            return Err(fail(
                "请在设置中配置独立的云端朗读 API Key，或手动选择离线朗读",
            ));
        }
        let token = self
            .cancel
            .lock()
            .map_err(|_| fail("语音任务锁异常"))?
            .clone();
        let voice = if language == "zh" {
            &settings.cloud_chinese_voice
        } else {
            &settings.cloud_english_voice
        };
        let identity = format!(
            "{}\0{}\0{}\0{}\0{}",
            settings.cloud_endpoint, settings.cloud_model, voice, language, api_key
        );
        let key = hex::encode(Sha256::digest(identity.as_bytes()));
        let cache_key = hex::encode(Sha256::digest(format!("{key}\0{text}").as_bytes()));
        let cached = self
            .cache
            .lock()
            .map_err(|_| fail("音频缓存锁异常"))?
            .get(&cache_key);
        let name = format!("阿里云 {voice}");
        let packet = |kind, audio| SpeechPacket {
            kind,
            audio,
            sample_rate: 24000,
            voice_name: name.clone(),
        };
        if let Some(bytes) = cached {
            if token.is_cancelled() {
                return Err(fail("朗读已取消"));
            }
            for chunk in bytes.chunks(24000) {
                publish(packet("audio", STANDARD.encode(chunk)))?;
            }
            publish(packet("done", String::new()))?;
            return Ok(SpeechTiming {
                first_audio_ms: 0,
                total_ms: 0,
                cached: true,
            });
        }
        let start = Instant::now();
        let mut slot = token
            .run_until_cancelled(self.session.lock())
            .await
            .ok_or_else(|| fail("朗读已取消"))?;
        let previous = slot.take();
        let result = token.run_until_cancelled(tokio::time::timeout(Duration::from_secs(120), async {
            let mut session = match previous {
                Some(s) if s.key == key && s.used.elapsed() < IDLE => s,
                _ => {
                    let mut url = reqwest::Url::parse(&settings.cloud_endpoint).map_err(|_| fail("语音地址无效"))?;
                    url.query_pairs_mut().append_pair("model", &settings.cloud_model);
                    let mut request = url.as_str().into_client_request().map_err(|_| fail("语音连接参数无效"))?;
                    #[cfg(test)]
                    if let Some(endpoint) = &self.test_endpoint { request = endpoint.as_str().into_client_request().unwrap(); }
                    request.headers_mut().insert("Authorization", format!("Bearer {api_key}").parse().map_err(|_| fail("API Key 格式无效"))?);
                    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(request)).await
                        .map_err(|_| fail("云端朗读连接超时"))?.map_err(|err| match err {
                            tokio_tungstenite::tungstenite::Error::Http(response) => service_error(&response.status().as_u16().to_string()),
                            _ => fail("无法连接云端朗读，请检查网络和地域"),
                        })?;
                    send(&mut socket, json!({"event_id":"setup", "type":"session.update", "session":{"mode":"commit", "voice":voice, "language_type":if language == "zh" {"Chinese"} else {"English"}, "response_format":"pcm", "sample_rate":24000}})).await?;
                    loop { if receive(&mut socket).await?["type"] == "session.updated" { break; } }
                    Session { socket, key: key.clone(), used: Instant::now() }
                }
            };
            let event = EVENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            send(&mut session.socket, json!({"event_id":format!("text-{event}"),"type":"input_text_buffer.append","text":text})).await?;
            send(&mut session.socket, json!({"event_id":format!("commit-{event}"),"type":"input_text_buffer.commit"})).await?;
            let mut bytes = Vec::new(); let mut first = None;
            loop {
                if start.elapsed() > Duration::from_secs(120) { return Err(fail("云端朗读合成超时，请缩短文本或切换离线方案")); }
                let value = receive(&mut session.socket).await?;
                match value["type"].as_str().unwrap_or("") {
                    "response.audio.delta" => {
                        let audio = value["delta"].as_str().ok_or_else(|| fail("语音音频块缺失"))?;
                        if audio.len() > MAX_AUDIO * 2 { return Err(fail("语音音频块过大")); }
                        let chunk = STANDARD.decode(audio).map_err(|_| fail("语音音频编码无效"))?;
                        if chunk.is_empty() || !chunk.len().is_multiple_of(2) || bytes.len() + chunk.len() > MAX_AUDIO { return Err(fail("语音音频大小无效")); }
                        first.get_or_insert_with(|| start.elapsed().as_millis());
                        bytes.extend_from_slice(&chunk); publish(packet("audio", audio.to_string()))?;
                    }
                    "response.done" => {
                        if value["response"]["status"].as_str() != Some("completed") || bytes.is_empty() { return Err(fail("云端语音生成未完成，请重试")); }
                        publish(packet("done", String::new()))?;
                        self.cache.lock().map_err(|_| fail("音频缓存锁异常"))?.insert(cache_key, bytes.clone(), bytes.len());
                        session.used = Instant::now(); *slot = Some(session);
                        return Ok(SpeechTiming { first_audio_ms: first.unwrap_or(0), total_ms: start.elapsed().as_millis(), cached: false });
                    }
                    _ => (),
                }
            }
        })).await;
        result
            .ok_or_else(|| fail("朗读已取消"))?
            .map_err(|_| fail("云端朗读合成超时，请缩短文本或切换离线方案"))?
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_reject_key_exfiltration_and_wrong_protocol() {
        for endpoint in [
            "ws://dashscope.aliyuncs.com/api-ws/v1/realtime",
            "wss://evil.example/api-ws/v1/realtime",
            "wss://dashscope.aliyuncs.com/api-ws/v1/realtime?key=x",
            "wss://user:pass@dashscope.aliyuncs.com/api-ws/v1/realtime",
        ] {
            assert!(validate_endpoint(endpoint).is_err());
        }
        validate_endpoint(&SpeechPreferences::default().cloud_endpoint).unwrap();
        validate_endpoint("wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime").unwrap();
    }
    #[tokio::test]
    async fn empty_key_never_connects() {
        assert!(CloudSpeech::default()
            .speak("test", "en", &SpeechPreferences::default(), "", |_| panic!(
                "must not send"
            ))
            .await
            .is_err());
    }

    #[test]
    fn provider_error_codes_are_case_insensitive_and_redacted() {
        assert!(service_error("InvalidApiKey")
            .user_message()
            .contains("认证失败"));
        assert!(service_error("QuotaExceeded")
            .user_message()
            .contains("额度不足"));
        assert!(service_error("RateLimitExceeded")
            .user_message()
            .contains("过于频繁"));
        let message = service_error("unknown-sensitive-content").user_message();
        assert!(!message.contains("sensitive"));
    }

    // tungstenite fixes this callback's error type to an unboxed HTTP response.
    #[allow(clippy::result_large_err)]
    async fn mocked() -> (CloudSpeech, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(
                        request.headers().get("Authorization").unwrap(),
                        "Bearer test-key"
                    );
                    Ok(response)
                },
            )
            .await
            .unwrap();
            let setup: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(setup["type"], "session.update");
            assert_eq!(setup["session"]["voice"], "Cherry");
            assert_eq!(setup["session"]["mode"], "commit");
            assert_eq!(setup["session"]["language_type"], "English");
            assert_eq!(setup["session"]["response_format"], "pcm");
            assert_eq!(setup["session"]["sample_rate"], 24000);
            socket
                .send(Message::Text(
                    json!({"type":"session.updated"}).to_string().into(),
                ))
                .await
                .unwrap();
            for _ in 0..2 {
                let text: Value =
                    serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                let commit: Value =
                    serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                assert_eq!(text["type"], "input_text_buffer.append");
                assert_eq!(commit["type"], "input_text_buffer.commit");
                socket
                    .send(Message::Text(
                        json!({"type":"response.audio.delta","delta": STANDARD.encode([0u8; 480])})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                // Consumer must receive first audio before response.done, not buffered whole-file output.
                tokio::time::sleep(Duration::from_millis(25)).await;
                socket
                    .send(Message::Text(
                        json!({"type":"response.done","response":{"status":"completed"}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        let service = CloudSpeech {
            test_endpoint: Some(format!("ws://{address}")),
            ..Default::default()
        };
        (service, task)
    }
    #[tokio::test]
    async fn pcm_streaming_connection_reuse_and_cache() {
        let (service, task) = mocked().await;
        let settings = SpeechPreferences::default();
        let packets = Mutex::new(Vec::new());
        let start = Instant::now();
        let timing = service
            .speak("one", "en", &settings, "test-key", |packet| {
                packets.lock().unwrap().push((packet.kind, start.elapsed()));
                Ok(())
            })
            .await
            .unwrap();
        let packets = packets.into_inner().unwrap();
        assert_eq!(
            packets.iter().map(|p| p.0).collect::<Vec<_>>(),
            ["audio", "done"]
        );
        assert!(packets[0].1 < packets[1].1);
        assert!(!timing.cached);
        assert!(service.session.lock().await.is_some());
        service
            .speak("two", "en", &settings, "test-key", |_| Ok(()))
            .await
            .unwrap();
        assert!(
            service
                .speak("one", "en", &settings, "test-key", |_| Ok(()))
                .await
                .unwrap()
                .cached
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn cancellation_closes_inflight_socket_and_never_publishes_done() {
        let (service, task) = mocked().await;
        let service = std::sync::Arc::new(service);
        let worker = service.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = Mutex::new(Some(sender));
        let reading = tokio::spawn(async move {
            worker
                .speak(
                    "one",
                    "en",
                    &SpeechPreferences::default(),
                    "test-key",
                    |packet| {
                        assert_eq!(packet.kind, "audio");
                        if let Some(sender) = sender.lock().unwrap().take() {
                            let _ = sender.send(());
                        }
                        Ok(())
                    },
                )
                .await
        });
        receiver.await.unwrap();
        service.stop();
        assert!(reading.await.unwrap().is_err());
        assert!(service.session.lock().await.is_none());
        task.abort();
    }
}
