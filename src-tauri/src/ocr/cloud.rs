use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{config::AppSettings, errors::AppError};

const MAX_CLOUD_IMAGE_BYTES: usize = 7_000_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
/// A small built-in picture of two lines of text, for testing a connection without sending
/// anything from the user's screen.
pub const CONNECTION_TEST_IMAGE: &[u8] = include_bytes!("../../assets/ocr-connection-test.png");
/// A word in that picture; finding it in the reply means the model really read the image.
pub const CONNECTION_TEST_WORD: &str = "quicktranslate";
const OCR_PROMPT: &str =
    "请准确提取图片中的全部原始文字，保持阅读顺序、换行、大小写和标点。只输出识别文字，不要解释，不要翻译，不要使用代码块。";

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Value,
}

pub async fn recognize(
    client: &reqwest::Client,
    settings: &AppSettings,
    api_key: &str,
    png: &[u8],
) -> Result<String, AppError> {
    if api_key.trim().is_empty() {
        return Err(AppError::Ocr("请先在设置中填写云端视觉 OCR API Key".into()));
    }
    if png.len() > MAX_CLOUD_IMAGE_BYTES {
        return Err(AppError::Ocr(
            "截图过大，请缩小框选区域后重试（云端图片上限 7 MB）".into(),
        ));
    }
    let endpoint = format!(
        "{}/chat/completions",
        settings.cloud_ocr_base_url.trim_end_matches('/')
    );
    let data_url = format!("data:image/png;base64,{}", STANDARD.encode(png));
    let body = json!({
        "model": settings.cloud_ocr_model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": { "url": data_url },
                    "min_pixels": 3072,
                    "max_pixels": 8388608
                },
                { "type": "text", "text": OCR_PROMPT }
            ]
        }]
    });

    let mut attempt = 0;
    loop {
        attempt += 1;
        // The shared client's 30 s budget suits text requests; an image upload of several
        // megabytes plus recognition needs its own.
        let response = match client
            .post(&endpoint)
            .timeout(REQUEST_TIMEOUT)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            // Nothing was sent yet, so trying once more costs nothing.
            Err(error) if error.is_connect() && attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(650)).await;
                continue;
            }
            Err(error) if error.is_timeout() => {
                return Err(AppError::Ocr(format!(
                    "云端识别超时（{} 秒）。区域较大或网络较慢时，请缩小框选范围后重试",
                    REQUEST_TIMEOUT.as_secs()
                )));
            }
            Err(error) => return Err(AppError::Network(error.to_string())),
        };
        let status = response.status();
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < 2 {
            tokio::time::sleep(Duration::from_millis(650)).await;
            continue;
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::InvalidApiKey);
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(AppError::Ocr(format!(
                "云端视觉模型请求失败（HTTP {}）：{}",
                status.as_u16(),
                safe_error_excerpt(&message)
            )));
        }
        let payload: ChatResponse = response
            .json()
            .await
            .map_err(|error| AppError::Ocr(format!("无法解析云端 OCR 响应：{error}")))?;
        let text = payload
            .choices
            .first()
            .and_then(|choice| content_text(&choice.message.content))
            .unwrap_or_default();
        let text = normalize_output(&text);
        if text.is_empty() {
            return Err(AppError::OcrNoText);
        }
        return Ok(text);
    }
}

fn content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    content.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn normalize_output(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let without_opening = trimmed
            .strip_prefix("```text")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        return without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
            .to_string();
    }
    trimmed.to_string()
}

fn safe_error_excerpt(message: &str) -> String {
    let mut excerpt = message.replace(['\r', '\n'], " ");
    excerpt.truncate(320);
    if excerpt.is_empty() {
        "服务未返回错误详情".into()
    } else {
        excerpt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_string_and_array_message_content() {
        assert_eq!(content_text(&json!("hello")).as_deref(), Some("hello"));
        assert_eq!(
            content_text(&json!([{"type": "text", "text": "a"}, {"text": "b"}])).as_deref(),
            Some("a\nb")
        );
    }

    #[test]
    fn removes_only_outer_code_fence() {
        assert_eq!(normalize_output("```text\nhello\n```"), "hello");
        assert_eq!(normalize_output("plain text"), "plain text");
    }
}
