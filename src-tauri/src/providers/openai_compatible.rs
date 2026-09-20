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
        let json = content
            .strip_prefix("```json")
            .or_else(|| content.strip_prefix("```"))
            .unwrap_or(content)
            .strip_suffix("```")
            .unwrap_or(content)
            .trim();
        if let Ok(dictionary) = serde_json::from_str::<DictionaryResponse>(json) {
            if !dictionary.translation.trim().is_empty() {
                result.translation = dictionary.translation;
                result.phonetic = dictionary.phonetic;
                result.part_of_speech = dictionary.part_of_speech;
                result.definitions = dictionary.definitions;
                result.example = dictionary.example;
            }
        }
    }
    Ok(result)
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
