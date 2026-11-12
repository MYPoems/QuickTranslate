//! Streaming against a real HTTP connection: a loopback server that speaks just enough
//! HTTP to send what a provider would.

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

use super::{OpenAiCompatibleProvider, ProviderConfig};
use crate::{
    errors::AppError,
    translation::types::{Language, TranslationRequest},
};

/// Serves one prepared reply per incoming connection, in order, and returns the requests it
/// received. Each reply is a list of byte chunks written with a short pause in between.
fn serve(replies: Vec<Vec<Vec<u8>>>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/v1", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap_or(0);
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8_lossy(&bytes).into_owned());
            for chunk in reply {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(15));
            }
        }
        requests
    });
    (address, handle)
}

fn provider(base_url: String) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(
        reqwest::Client::new(),
        ProviderConfig {
            provider: "Test".into(),
            base_url,
            api_key: "test-key".into(),
            model: "test-model".into(),
        },
    )
}

fn sentence() -> TranslationRequest {
    TranslationRequest::new(
        "Stay curious. Keep learning.",
        Language::English,
        Language::Chinese,
    )
}

fn event(content: &str) -> Vec<u8> {
    format!(
        "data: {}\n\n",
        serde_json::json!({ "choices": [{ "delta": { "content": content } }] })
    )
    .into_bytes()
}

const STREAM_HEAD: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nConnection: close\r\n\r\n";

fn json_reply(status: &str, body: &str) -> Vec<Vec<u8>> {
    vec![format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()]
}

const WHOLE_REPLY: &str = r#"{"choices":[{"message":{"content":"保持好奇，持续学习。"}}]}"#;

#[tokio::test]
async fn text_is_handed_over_as_it_arrives() {
    let mut bytes = "保持好奇，".as_bytes().to_vec();
    // Split a character across two network chunks.
    let tail = bytes.split_off(4);
    let (address, server) = serve(vec![vec![
        STREAM_HEAD.to_vec(),
        event(" "),
        [
            &b"data: {\"choices\":[{\"delta\":{\"content\":\""[..],
            &bytes,
        ]
        .concat(),
        [&tail[..], &b"\"}}]}\n\n"[..]].concat(),
        event("持续学习。"),
        b"data: [DONE]\n\n".to_vec(),
    ]]);

    let mut seen: Vec<String> = Vec::new();
    let result = provider(address)
        .translate_stream(sentence(), |text| seen.push(text.to_string()))
        .await
        .unwrap();

    assert_eq!(result.translation, "保持好奇，持续学习。");
    assert_eq!(result.source_text, "Stay curious. Keep learning.");
    assert_eq!(
        seen.last().map(String::as_str),
        Some("保持好奇，持续学习。")
    );
    assert!(
        seen.len() >= 2 && seen[0] == "保持好奇，",
        "the beginning was visible before the end arrived: {seen:?}"
    );
    for pair in seen.windows(2) {
        assert!(pair[1].starts_with(&pair[0]), "text only ever grows");
    }
    let requests = server.join().unwrap();
    assert!(requests[0].contains("\"stream\":true"));
    assert!(requests[0]
        .to_ascii_lowercase()
        .contains("authorization: bearer test-key"));
}

#[tokio::test]
async fn a_service_that_refuses_to_stream_is_asked_again_the_ordinary_way() {
    let (address, server) = serve(vec![
        json_reply("400 Bad Request", r#"{"error":"stream is not supported"}"#),
        json_reply("200 OK", WHOLE_REPLY),
    ]);
    let mut calls = 0;
    let result = provider(address)
        .translate_stream(sentence(), |_| calls += 1)
        .await
        .unwrap();
    assert_eq!(result.translation, "保持好奇，持续学习。");
    assert_eq!(calls, 0);
    let requests = server.join().unwrap();
    assert!(requests[0].contains("\"stream\":true"));
    assert!(!requests[1].contains("\"stream\""));
}

#[tokio::test]
async fn a_whole_reply_to_a_streaming_request_is_accepted() {
    let (address, server) = serve(vec![json_reply("200 OK", WHOLE_REPLY)]);
    let result = provider(address)
        .translate_stream(sentence(), |_| panic!("nothing arrives in pieces"))
        .await
        .unwrap();
    assert_eq!(result.translation, "保持好奇，持续学习。");
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn a_stream_that_ends_empty_or_fails_half_way_is_an_error() {
    let (address, server) = serve(vec![vec![
        STREAM_HEAD.to_vec(),
        b"data: [DONE]\n\n".to_vec(),
    ]]);
    assert!(matches!(
        provider(address).translate_stream(sentence(), |_| {}).await,
        Err(AppError::Provider(_))
    ));
    server.join().unwrap();

    let (address, server) = serve(vec![vec![
        STREAM_HEAD.to_vec(),
        event("保持"),
        b"data: {\"error\":{\"message\":\"quota exceeded\"}}\n\n".to_vec(),
    ]]);
    let mut seen = Vec::new();
    let failed = provider(address)
        .translate_stream(sentence(), |text| seen.push(text.to_string()))
        .await;
    assert!(matches!(failed, Err(AppError::Provider(_))));
    assert_eq!(seen, ["保持"]);
    server.join().unwrap();
}

#[tokio::test]
async fn a_single_word_is_looked_up_whole() {
    let reply = r#"{"choices":[{"message":{"content":"{\"translation\":\"架构\",\"definitions\":[\"体系结构\"]}"}}]}"#;
    let (address, server) = serve(vec![json_reply("200 OK", reply)]);
    let result = provider(address)
        .translate_stream(
            TranslationRequest::new("architecture", Language::English, Language::Chinese),
            |_| panic!("dictionary lookups are not streamed"),
        )
        .await
        .unwrap();
    assert_eq!(result.translation, "架构");
    assert!(!server.join().unwrap()[0].contains("\"stream\""));
}
