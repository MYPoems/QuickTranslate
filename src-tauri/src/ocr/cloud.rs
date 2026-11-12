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
    let png = fit_for_upload(png)?;
    let png = png.as_ref();
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

/// The least a screenshot may be shrunk to. Below this, small text stops being legible
/// and the recognition would be a guess.
const MIN_UPLOAD_SCALE: f64 = 0.5;

/// A screenshot within the upload limit: the original if it already fits, otherwise a
/// scaled-down copy. A region so large that it would have to shrink below half size is
/// refused, because the text in it would no longer be readable.
fn fit_for_upload(png: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, AppError> {
    fit_within(png, MAX_CLOUD_IMAGE_BYTES)
}

fn fit_within(png: &[u8], limit: usize) -> Result<std::borrow::Cow<'_, [u8]>, AppError> {
    use image::{codecs::png::PngEncoder, imageops::FilterType, ImageEncoder};

    if png.len() <= limit {
        return Ok(std::borrow::Cow::Borrowed(png));
    }
    let too_large = || {
        AppError::Ocr(
            "所选区域太大，缩小到可上传的尺寸后文字会看不清。请框选更小的范围后重试".into(),
        )
    };
    let original = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| AppError::Ocr(format!("无法处理截图：{error}")))?
        .into_rgba8();
    let (width, height) = original.dimensions();
    // A PNG's size follows its pixel count only roughly, so aim a little under the limit
    // and try again if the first attempt is still over.
    let mut scale = (limit as f64 / png.len() as f64).sqrt() * 0.92;
    for _ in 0..4 {
        if scale < MIN_UPLOAD_SCALE {
            return Err(too_large());
        }
        let resized = image::imageops::resize(
            &original,
            ((f64::from(width) * scale).round() as u32).max(1),
            ((f64::from(height) * scale).round() as u32).max(1),
            FilterType::Triangle,
        );
        let mut encoded = Vec::new();
        PngEncoder::new(&mut encoded)
            .write_image(
                resized.as_raw(),
                resized.width(),
                resized.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| AppError::Ocr(format!("无法处理截图：{error}")))?;
        if encoded.len() <= limit {
            return Ok(std::borrow::Cow::Owned(encoded));
        }
        scale *= 0.85;
    }
    Err(too_large())
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
    // Cut by characters: cutting a UTF-8 string at a byte offset can land inside one.
    let excerpt: String = message
        .replace(['\r', '\n'], " ")
        .chars()
        .take(320)
        .collect();
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

    /// A PNG of noise, which compression cannot shrink: about four bytes per pixel.
    fn noise_png(width: u32, height: u32) -> Vec<u8> {
        use image::{codecs::png::PngEncoder, ImageEncoder};
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let pixels: Vec<u8> = (0..width as usize * height as usize * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 32) as u8
            })
            .collect();
        let mut encoded = Vec::new();
        PngEncoder::new(&mut encoded)
            .write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        encoded
    }

    #[test]
    fn an_oversized_screenshot_is_scaled_to_fit_and_a_hopeless_one_is_refused() {
        // The same rule at a small scale, so the test stays quick.
        const LIMIT: usize = 150_000;
        let small = noise_png(64, 64);
        assert!(matches!(
            fit_for_upload(&small).unwrap(),
            std::borrow::Cow::Borrowed(_)
        ));

        let large = noise_png(300, 200);
        assert!(large.len() > LIMIT);
        let fitted = fit_within(&large, LIMIT).unwrap();
        assert!(fitted.len() <= LIMIT);
        let (width, height) = image::load_from_memory(&fitted)
            .unwrap()
            .into_rgba8()
            .dimensions();
        assert!((150..300).contains(&width), "{width}");
        assert!(
            (width * 2).abs_diff(height * 3) <= 3,
            "proportions are kept: {width}x{height}"
        );

        // Would have to shrink below half size: the text would not survive.
        let huge = noise_png(700, 500);
        assert!(matches!(fit_within(&huge, LIMIT), Err(AppError::Ocr(_))));
    }

    #[test]
    fn error_excerpts_cut_between_characters() {
        let excerpt = safe_error_excerpt(&"错误\n".repeat(400));
        assert_eq!(excerpt.chars().count(), 320);
        assert!(!excerpt.contains('\n'));
    }

    #[test]
    fn removes_only_outer_code_fence() {
        assert_eq!(normalize_output("```text\nhello\n```"), "hello");
        assert_eq!(normalize_output("plain text"), "plain text");
    }
}
