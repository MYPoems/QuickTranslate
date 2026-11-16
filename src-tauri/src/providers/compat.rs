// Services that speak the same chat-completions protocol still differ in what a request may
// or must contain. The differences that matter here are kept in one place.

use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

/// What a request to a given service and model should carry beyond the common fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dialect {
    /// The sampling temperature to send, or `None` where the service rejects the field.
    pub temperature: Option<f32>,
    /// `Some(false)` where the model would otherwise reason at length before answering.
    pub enable_thinking: Option<bool>,
}

/// Works out the dialect from the service address and the model name.
///
/// * Current Claude models answer a request that sets `temperature` below 1 with HTTP 400,
///   on Anthropic's own endpoint and behind gateways alike, so the field is left out.
/// * The newer models on Alibaba Cloud Model Studio think before they answer unless told
///   not to. For a lookup or a screenshot that only adds seconds and spends tokens, so the
///   switch is sent with `false`; a model that refuses the switch is remembered and asked
///   without it from then on (see [`remember_thinking_switch_refused`]).
pub fn dialect(base_url: &str, model: &str, temperature: f32) -> Dialect {
    let host = host(base_url);
    let model = model.trim().to_ascii_lowercase();
    let claude = host == "api.anthropic.com" || model.contains("claude");
    Dialect {
        temperature: (!claude).then_some(temperature),
        enable_thinking: (is_model_studio(&host) && !thinking_switch_refused(&host, &model))
            .then_some(false),
    }
}

/// The pixel bounds that Model Studio's dedicated text-extraction models take with an image:
/// a smaller image is enlarged to the lower bound and a larger one reduced to the upper.
/// The values are the ones in their documentation.
///
/// Nobody else is sent them. General vision models on the same service have other limits
/// (qwen4-max answers HTTP 400 to a lower bound under 4096) and other services do not
/// know the fields; they get the image alone and apply their own defaults.
pub fn ocr_pixel_bounds(base_url: &str, model: &str) -> Option<(u32, u32)> {
    let model = model.trim().to_ascii_lowercase();
    // A model may be named with a vendor prefix, as in "vendor/model".
    let name = model.rsplit('/').next().unwrap_or(&model);
    (is_model_studio(&host(base_url)) && name.starts_with("qwen") && name.contains("ocr"))
        .then_some((3072, 8_388_608))
}

/// What to tell the user when a service answers HTTP 403.
///
/// A 403 is not always a bad key. Model Studio answers it when a model's free quota is used
/// up and the account stops at the free tier; reported as "invalid API key" that sent the
/// user looking in the wrong place. That case is named outright. Anything else is shown in
/// the service's own words, shortened, and with the key removed should a service echo it.
pub fn forbidden_message(body: &str, api_key: &str) -> String {
    if body.contains("FreeTierOnly") {
        return "这个模型的免费额度已用完，而阿里云百炼开启了“免费额度用完即停”，所以请求被拒绝。请换一个仍有额度的模型，或在百炼控制台关闭该模型的“用完即停”后按量付费。".into();
    }
    let said = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["message"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.to_string());
    let mut said = said.split_whitespace().collect::<Vec<_>>().join(" ");
    if !api_key.trim().is_empty() {
        said = said.replace(api_key.trim(), "***");
    }
    if said.is_empty() {
        return "服务拒绝了这次请求（HTTP 403）。请检查 API Key 是否有权使用这个模型。".into();
    }
    let excerpt: String = said.chars().take(160).collect();
    let ellipsis = if said.chars().count() > 160 {
        "…"
    } else {
        ""
    };
    format!("服务拒绝了这次请求（HTTP 403）：{excerpt}{ellipsis}")
}

fn host(base_url: &str) -> String {
    reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default()
}

fn is_model_studio(host: &str) -> bool {
    host.ends_with(".aliyuncs.com") && (host.starts_with("dashscope") || host.contains(".maas."))
}

fn refused() -> &'static Mutex<HashSet<String>> {
    static REFUSED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    REFUSED.get_or_init(Mutex::default)
}

fn refusal_key(host: &str, model: &str) -> String {
    format!("{host}\n{model}")
}

fn thinking_switch_refused(host: &str, model: &str) -> bool {
    refused()
        .lock()
        .is_ok_and(|set| set.contains(&refusal_key(host, model)))
}

/// Records that this model answered HTTP 400 to a request carrying the thinking switch and
/// accepted the same request without it, so later requests do not waste a round trip.
/// Kept for the life of the process only.
pub fn remember_thinking_switch_refused(base_url: &str, model: &str) {
    let host = host(base_url);
    if let Ok(mut set) = refused().lock() {
        set.insert(refusal_key(&host, &model.trim().to_ascii_lowercase()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL_STUDIO: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

    #[test]
    fn ordinary_services_get_the_temperature_and_nothing_else() {
        for (base, model) in [
            ("https://api.openai.com/v1", "gpt-4.1-mini"),
            ("https://api.deepseek.com/v1", "deepseek-chat"),
            ("http://localhost:11434/v1", "qwen2.5:3b"),
            ("not a url", "anything"),
        ] {
            assert_eq!(
                dialect(base, model, 0.1),
                Dialect {
                    temperature: Some(0.1),
                    enable_thinking: None
                },
                "{base} {model}"
            );
        }
    }

    #[test]
    fn claude_is_never_sent_a_temperature() {
        assert_eq!(
            dialect("https://api.anthropic.com/v1", "claude-haiku-5-5", 0.1).temperature,
            None
        );
        // Behind a gateway the model name is what gives it away.
        assert_eq!(
            dialect(
                "https://gateway.example/v1",
                "anthropic/Claude-Sonnet-5-5",
                0.2
            )
            .temperature,
            None
        );
        assert_eq!(
            dialect("https://api.anthropic.com/v1/", "claude-haiku-5-5", 0.1).enable_thinking,
            None
        );
    }

    #[test]
    fn model_studio_is_told_not_to_think() {
        for base in [
            MODEL_STUDIO,
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
            "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
        ] {
            let dialect = dialect(base, "qwen3.8-flash", 0.1);
            assert_eq!(dialect.enable_thinking, Some(false), "{base}");
            assert_eq!(dialect.temperature, Some(0.1), "{base}");
        }
        // Other Alibaba Cloud hosts are not Model Studio.
        assert_eq!(
            dialect(
                "https://oss-cn-beijing.aliyuncs.com/v1",
                "qwen3.8-flash",
                0.1
            )
            .enable_thinking,
            None
        );
    }

    #[test]
    fn a_used_up_free_quota_is_not_reported_as_a_bad_key() {
        let body = r#"{"error":{"message":"The free tier of the model has been exhausted.","type":"AllocationQuota.FreeTierOnly","code":"AllocationQuota.FreeTierOnly"}}"#;
        let message = forbidden_message(body, "sk-test");
        assert!(message.contains("免费额度已用完"), "{message}");
        assert!(message.contains("用完即停"), "{message}");
    }

    #[test]
    fn other_refusals_are_shown_in_the_services_words_without_the_key() {
        let message = forbidden_message(
            r#"{"error":{"message":"Access to model denied for key sk-secret-123.\n Contact support."}}"#,
            "sk-secret-123",
        );
        assert!(message.contains("HTTP 403"), "{message}");
        assert!(
            message.contains("Access to model denied for key ***. Contact support."),
            "{message}"
        );
        assert!(!message.contains("sk-secret-123"));

        // Not JSON: the text itself, cut between characters.
        let long = "拒绝访问。".repeat(60);
        let message = forbidden_message(&long, "");
        assert!(message.ends_with('…'), "{message}");
        assert!(message.chars().count() < 200);

        assert!(forbidden_message("", "sk-test").contains("是否有权使用这个模型"));
    }

    #[test]
    fn only_the_dedicated_text_extraction_models_get_pixel_bounds() {
        for model in [
            "qwen3.5-ocr",
            "qwen-vl-ocr",
            "Qwen-VL-OCR-latest",
            " qwen3.5-ocr ",
        ] {
            assert_eq!(
                ocr_pixel_bounds(MODEL_STUDIO, model),
                Some((3072, 8_388_608)),
                "{model}"
            );
        }
        // General vision models have other limits: qwen4-max refuses 3072.
        for model in [
            "qwen4-max",
            "qwen3.8-flash",
            "vanchin/deepseek-ocr",
            "deepseek-v4.1-flash",
        ] {
            assert_eq!(ocr_pixel_bounds(MODEL_STUDIO, model), None, "{model}");
        }
        // Other services do not know the fields at all.
        for (base, model) in [
            ("https://api.anthropic.com/v1", "claude-haiku-5-5"),
            ("https://api.openai.com/v1", "gpt-4.1-mini"),
            ("https://gateway.example/v1", "qwen3.5-ocr"),
        ] {
            assert_eq!(ocr_pixel_bounds(base, model), None, "{base} {model}");
        }
    }

    #[test]
    fn a_model_that_refused_the_switch_is_asked_without_it_afterwards() {
        let model = "thinking-only-test-model";
        assert_eq!(
            dialect(MODEL_STUDIO, model, 0.1).enable_thinking,
            Some(false)
        );
        remember_thinking_switch_refused(MODEL_STUDIO, " Thinking-Only-Test-Model ");
        assert_eq!(dialect(MODEL_STUDIO, model, 0.1).enable_thinking, None);
        // Other models on the same service are unaffected.
        assert_eq!(
            dialect(MODEL_STUDIO, "qwen3.8-flash", 0.1).enable_thinking,
            Some(false)
        );
    }
}
