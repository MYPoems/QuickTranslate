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
    let host = reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    let model = model.trim().to_ascii_lowercase();
    let claude = host == "api.anthropic.com" || model.contains("claude");
    let model_studio = host.ends_with(".aliyuncs.com")
        && (host.starts_with("dashscope") || host.contains(".maas."));
    Dialect {
        temperature: (!claude).then_some(temperature),
        enable_thinking: (model_studio && !thinking_switch_refused(&host, &model)).then_some(false),
    }
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
    let host = reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
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
