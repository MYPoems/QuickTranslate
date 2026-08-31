use async_trait::async_trait;
use reqwest::StatusCode;
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

        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

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
        parse_response(response, &request, &self.config.model)
    }
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
        provider: "OpenAI Compatible".into(),
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
        let result = parse_response(response, &request("Hello world."), "test").unwrap();
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
        let result = parse_response(response, &request("architecture"), "test").unwrap();
        assert_eq!(result.translation, "架构");
        assert_eq!(result.definitions, vec!["体系结构"]);
    }

    #[test]
    fn rejects_empty_choices() {
        let response = ChatResponse { choices: vec![] };
        assert!(parse_response(response, &request("hello"), "test").is_err());
    }
}
