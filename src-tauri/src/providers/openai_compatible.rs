use async_trait::async_trait;
use std::time::Duration;

use reqwest::{header::RETRY_AFTER, StatusCode};
use serde::{Deserialize, Serialize};

use crate::{
    errors::AppError,
    translation::{
        prompt::{build_system_prompt, is_single_english_word},
        types::{TranslationRequest, TranslationResult},
    },
};

use super::Translator;

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    config: ProviderConfig,
}

impl OpenAiCompatibleProvider {
    pub fn new(client: reqwest::Client, config: ProviderConfig) -> Self {
        Self { client, config }
    }

    fn endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.into()
        } else {
            format!("{base}/chat/completions")
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
    temperature: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Default, Deserialize)]
struct DictionaryResponse {
    translation: String,
    phonetic: Option<String>,
    part_of_speech: Option<String>,
    #[serde(default)]
    definitions: Vec<String>,
    example: Option<String>,
}

#[async_trait]
impl Translator for OpenAiCompatibleProvider {
    async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult, AppError> {
        let system_prompt = build_system_prompt(&request);
        let body = ChatRequest {
            model: &self.config.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: &system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &request.text,
                },
            ],
            temperature: 0.1,
            stream: false,
        };

        let mut attempt = 0;
        let response = loop {
            let mut request = self.client.post(self.endpoint()).json(&body);
            if !self.config.api_key.is_empty() {
                request = request.bearer_auth(&self.config.api_key);
            }
            let outcome = request.send().await;

            match outcome {
                Ok(response) if is_retryable_status(response.status()) && attempt < MAX_RETRIES => {
                    let delay = retry_delay(
                        attempt,
                        response
                            .headers()
                            .get(RETRY_AFTER)
                            .and_then(|value| value.to_str().ok()),
                    );
                    attempt += 1;
                    tokio::time::sleep(delay).await;
                }
                Err(error)
                    if (error.is_timeout() || error.is_connect()) && attempt < MAX_RETRIES =>
                {
                    let delay = retry_delay(attempt, None);
                    attempt += 1;
                    tokio::time::sleep(delay).await;
                }
                Ok(response) => break response,
                Err(error) => return Err(map_reqwest_error(error)),
            }
        };

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(AppError::InvalidApiKey);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::Provider("请求过于频繁，请稍后重试".into()));
        }
        if status.is_server_error() {
            return Err(AppError::Provider("翻译服务暂时不可用".into()));
        }
        if !status.is_success() {
            return Err(AppError::Provider(format!("API 请求失败（HTTP {status}）")));
        }

        let response: ChatResponse = response
            .json()
            .await
            .map_err(|error| AppError::Provider(format!("API 响应格式无效: {error}")))?;
        parse_response(
            response,
            &request,
            &self.config.provider,
            &self.config.model,
        )
    }
}

/// How long a streamed reply may take to begin, and how long it may pause between pieces.
/// A long translation is no longer cut off by a limit on its total duration.
const STREAM_START_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_TOTAL_LIMIT: Duration = Duration::from_secs(600);

impl OpenAiCompatibleProvider {
    /// Translates like [`Translator::translate`], and calls `on_text` with everything
    /// translated so far each time more arrives, so the reader sees the beginning while the
    /// rest is still being written.
    ///
    /// Single-word lookups are not streamed: their reply is JSON that is only usable whole.
    /// A service that refuses or ignores the streaming request is asked again the ordinary
    /// way; `on_text` is then simply never called.
    pub async fn translate_stream(
        &self,
        request: TranslationRequest,
        mut on_text: impl FnMut(&str) + Send,
    ) -> Result<TranslationResult, AppError> {
        if is_single_english_word(&request) {
            return self.translate(request).await;
        }
        let system_prompt = build_system_prompt(&request);
        let body = ChatRequest {
            model: &self.config.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: &system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &request.text,
                },
            ],
            temperature: 0.1,
            stream: true,
        };
        let mut builder = self
            .client
            .post(self.endpoint())
            .timeout(STREAM_TOTAL_LIMIT)
            .json(&body);
        if !self.config.api_key.is_empty() {
            builder = builder.bearer_auth(&self.config.api_key);
        }
        let mut response = match tokio::time::timeout(STREAM_START_TIMEOUT, builder.send()).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => return Err(map_reqwest_error(error)),
            Err(_) => return Err(AppError::Network("the service did not respond".into())),
        };
        // Refusals and transient failures go through the ordinary path, which knows how to
        // retry them and how to explain them.
        if !response.status().is_success() {
            return self.translate(request).await;
        }
        let event_stream = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/event-stream"));
        if !event_stream {
            // The service answered in one piece despite being asked to stream.
            let response: ChatResponse = response
                .json()
                .await
                .map_err(|error| AppError::Provider(format!("API 响应格式无效: {error}")))?;
            return parse_response(
                response,
                &request,
                &self.config.provider,
                &self.config.model,
            );
        }

        let mut events = EventStream::default();
        let mut translation = String::new();
        while !events.finished {
            let chunk = match tokio::time::timeout(STREAM_IDLE_TIMEOUT, response.chunk()).await {
                Ok(Ok(Some(chunk))) => chunk,
                Ok(Ok(None)) => break,
                Ok(Err(error)) => return Err(map_reqwest_error(error)),
                Err(_) => {
                    return Err(AppError::Network(
                        "the service stopped sending the translation".into(),
                    ))
                }
            };
            let before = translation.len();
            for piece in events.push(&chunk)? {
                translation.push_str(&piece);
            }
            if translation.len() > before && !translation.trim().is_empty() {
                on_text(translation.trim_start());
            }
        }
        let translation = translation.trim();
        if translation.is_empty() {
            return Err(AppError::Provider("API 返回了空译文".into()));
        }
        Ok(TranslationResult {
            source_text: request.text.clone(),
            translation: translation.to_string(),
            detected_language: request.source_language,
            target_language: request.target_language,
            provider: self.config.provider.clone(),
            model: self.config.model.clone(),
            cached: false,
            phonetic: None,
            part_of_speech: None,
            definitions: vec![],
            example: None,
        })
    }
}

/// Reads a `text/event-stream` body as it arrives, in pieces of any size, and yields the
/// text carried by each complete event.
#[derive(Default)]
struct EventStream {
    pending: Vec<u8>,
    finished: bool,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl EventStream {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, AppError> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.pending.extend_from_slice(bytes);
        let mut pieces = Vec::new();
        // Only whole lines are decoded, so a character split across two chunks is never
        // read half-way.
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                self.finished = true;
                break;
            }
            let Ok(event) = serde_json::from_str::<StreamEvent>(data) else {
                continue;
            };
            if event.error.is_some() {
                return Err(AppError::Provider("翻译服务在生成译文时报告了错误".into()));
            }
            pieces.extend(
                event
                    .choices
                    .into_iter()
                    .filter_map(|choice| choice.delta.and_then(|delta| delta.content))
                    .filter(|content| !content.is_empty()),
            );
        }
        Ok(pieces)
    }
}

const MAX_RETRIES: u32 = 2;

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32, retry_after: Option<&str>) -> Duration {
    if let Some(seconds) = retry_after.and_then(|value| value.parse::<u64>().ok()) {
        return Duration::from_secs(seconds.min(2));
    }
    Duration::from_millis(250 * 2_u64.pow(attempt.min(3)))
}

fn map_reqwest_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() || error.is_connect() {
        AppError::Network(error.to_string())
    } else {
        AppError::Provider("API 请求失败".into())
    }
}

fn parse_response(
    response: ChatResponse,
    request: &TranslationRequest,
    provider: &str,
    model: &str,
) -> Result<TranslationResult, AppError> {
    let content = response
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| AppError::Provider("API 返回了空译文".into()))?;

    let mut result = TranslationResult {
        source_text: request.text.clone(),
        translation: content.to_string(),
        detected_language: request.source_language,
        target_language: request.target_language,
        provider: provider.into(),
        model: model.into(),
        cached: false,
        phonetic: None,
        part_of_speech: None,
        definitions: vec![],
        example: None,
    };

    if is_single_english_word(request) {
        // The word prompt asks for JSON, but a model may still answer in plain text; that is
        // a usable translation. A reply that is JSON-shaped yet unusable is not: showing it
        // would put raw markup in the popup and, once cached, keep it there.
        if let Some(json) = dictionary_json(content) {
            let dictionary = serde_json::from_str::<DictionaryResponse>(json)
                .ok()
                .filter(|dictionary| !dictionary.translation.trim().is_empty())
                .ok_or_else(|| AppError::Provider("模型返回的词义格式无效，请重新翻译".into()))?;
            result.translation = dictionary.translation.trim().to_string();
            result.phonetic = dictionary.phonetic;
            result.part_of_speech = dictionary.part_of_speech;
            result.definitions = dictionary.definitions;
            result.example = dictionary.example;
        }
    }
    Ok(result)
}

/// The JSON object inside a dictionary reply, with any code fence or surrounding remarks
/// removed. `None` when the reply is not JSON-shaped at all.
fn dictionary_json(content: &str) -> Option<&str> {
    let content = content.trim();
    if !(content.starts_with('{') || content.starts_with("```")) {
        return None;
    }
    match (content.find('{'), content.rfind('}')) {
        (Some(start), Some(end)) if start < end => Some(&content[start..=end]),
        // Fenced or opened but never closed: JSON-shaped, and certain to fail parsing.
        _ => Some(content),
    }
}

#[cfg(test)]
mod tests {
    use crate::translation::types::Language;

    use super::*;

    fn request(text: &str) -> TranslationRequest {
        TranslationRequest {
            text: text.into(),
            source_language: Language::English,
            target_language: Language::Chinese,
        }
    }

    #[test]
    fn parses_plain_response() {
        let response = ChatResponse {
            choices: vec![Choice {
                message: ResponseMessage {
                    content: "你好，世界。".into(),
                },
            }],
        };
        let result =
            parse_response(response, &request("Hello world."), "Test Provider", "test").unwrap();
        assert_eq!(result.translation, "你好，世界。");
    }

    #[test]
    fn parses_dictionary_response() {
        let response = ChatResponse {
            choices: vec![Choice {
                message: ResponseMessage {
                    content: r#"{"translation":"架构","phonetic":"/test/","part_of_speech":"noun","definitions":["体系结构"],"example":null}"#.into(),
                },
            }],
        };
        let result =
            parse_response(response, &request("architecture"), "Test Provider", "test").unwrap();
        assert_eq!(result.translation, "架构");
        assert_eq!(result.definitions, vec!["体系结构"]);
    }

    fn word_reply(content: &str) -> Result<TranslationResult, AppError> {
        let response = ChatResponse {
            choices: vec![Choice {
                message: ResponseMessage {
                    content: content.into(),
                },
            }],
        };
        parse_response(response, &request("architecture"), "Test Provider", "test")
    }

    #[test]
    fn reads_dictionary_json_through_fences_and_surrounding_remarks() {
        let json = r#"{"translation":"架构","definitions":["体系结构"]}"#;
        for content in [
            format!("```json\n{json}\n```"),
            format!("```\n{json}\n```"),
            // No closing fence, and a remark after the object.
            format!("```json\n{json}"),
            format!("```json\n{json}\n```\n以上是释义。"),
        ] {
            let result = word_reply(&content).unwrap();
            assert_eq!(result.translation, "架构", "{content}");
            assert_eq!(result.definitions, vec!["体系结构"]);
        }
    }

    #[test]
    fn plain_text_word_reply_is_a_translation_but_broken_json_is_an_error() {
        assert_eq!(word_reply("架构").unwrap().translation, "架构");
        for content in [
            r#"{"translation":"架构","definitions":["#,
            r#"```json
{"translation":"#,
            r#"{"translation":"  "}"#,
            "```json",
        ] {
            assert!(word_reply(content).is_err(), "{content}");
        }
    }

    #[test]
    fn event_stream_survives_any_chunk_boundary() {
        let body = concat!(
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"保持\"}}]}\r\n\r\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"好奇，\"}}]}\n\n",
            "data:{\"choices\":[{\"delta\":{\"content\":\"持续学习。\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ignored\"}}]}\n\n",
        )
        .as_bytes();
        // Every split position, including the middle of a multi-byte character.
        for split in 0..=body.len() {
            let mut events = EventStream::default();
            let mut text = String::new();
            for chunk in [&body[..split], &body[split..]] {
                for piece in events.push(chunk).unwrap() {
                    text.push_str(&piece);
                }
            }
            assert_eq!(text, "保持好奇，持续学习。", "split at {split}");
            assert!(events.finished);
        }
    }

    #[test]
    fn event_stream_reports_an_error_event() {
        let mut events = EventStream::default();
        assert!(events
            .push(b"data: {\"error\":{\"message\":\"quota\"}}\n\n")
            .is_err());
    }

    #[test]
    fn streaming_is_requested_only_when_asked() {
        let request = |stream| {
            serde_json::to_value(ChatRequest {
                model: "m",
                messages: [
                    ChatMessage {
                        role: "system",
                        content: "s",
                    },
                    ChatMessage {
                        role: "user",
                        content: "u",
                    },
                ],
                temperature: 0.1,
                stream,
            })
            .unwrap()
        };
        assert_eq!(request(true)["stream"], true);
        assert!(request(false).get("stream").is_none());
    }

    #[test]
    fn rejects_empty_choices() {
        let response = ChatResponse { choices: vec![] };
        assert!(parse_response(response, &request("hello"), "Test Provider", "test").is_err());
    }

    #[test]
    fn retries_only_transient_http_failures() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn caps_server_retry_after_delay() {
        assert_eq!(retry_delay(0, Some("30")), Duration::from_secs(2));
        assert_eq!(retry_delay(1, None), Duration::from_millis(500));
    }
}
