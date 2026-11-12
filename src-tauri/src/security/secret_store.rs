use crate::{config::AppSettings, errors::AppError};

pub trait SecretStore: Send + Sync {
    fn save_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_api_key(&self) -> Result<(), AppError>;
    fn save_cloud_ocr_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_cloud_ocr_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_cloud_ocr_api_key(&self) -> Result<(), AppError>;
    fn save_cloud_speech_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_cloud_speech_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_cloud_speech_api_key(&self) -> Result<(), AppError>;
}

pub fn provider_api_key(
    settings: &AppSettings,
    secrets: &dyn SecretStore,
) -> Result<String, AppError> {
    match secrets
        .get_api_key()?
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => Ok(value),
        None if !settings.requires_api_key() => Ok(String::new()),
        None => Err(AppError::ProviderNotConfigured),
    }
}

/// Chooses the key that may accompany requests to `target_base_url`.
///
/// One key is stored per feature, so a saved key belongs to the service it was saved for.
/// It is reused only while the settings still point at that service (same scheme, host and
/// port); otherwise the user has to enter the new service's key or delete the saved one,
/// and nothing is sent anywhere in the meantime.
pub fn key_for_endpoint(
    entered: Option<&str>,
    clear: bool,
    saved: Option<&str>,
    saved_base_url: &str,
    target_base_url: &str,
) -> Result<Option<String>, AppError> {
    if clear {
        return Ok(None);
    }
    if let Some(entered) = entered.map(str::trim).filter(|key| !key.is_empty()) {
        return Ok(Some(entered.to_string()));
    }
    let Some(saved) = saved.filter(|key| !key.trim().is_empty()) else {
        return Ok(None);
    };
    // An empty target means the feature is switched off: no request can carry the key.
    if target_base_url.trim().is_empty() || same_origin(saved_base_url, target_base_url) {
        return Ok(Some(saved.to_string()));
    }
    Err(AppError::Provider(
        "服务地址已更换：请填写新服务的 API Key，或勾选删除已保存的 Key。已保存的 Key 不会发送到新地址。"
            .into(),
    ))
}

fn same_origin(left: &str, right: &str) -> bool {
    let origin = |value: &str| {
        let url = reqwest::Url::parse(value.trim()).ok()?;
        Some((
            url.scheme().to_string(),
            url.host_str()?.to_ascii_lowercase(),
            url.port_or_known_default(),
        ))
    };
    origin(left).is_some_and(|left| origin(right) == Some(left))
}

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    const SERVICE: &'static str = "QuickTranslate";
    const USERNAME: &'static str = "openai-compatible-api-key";
    const CLOUD_OCR_USERNAME: &'static str = "cloud-ocr-api-key";
    const CLOUD_SPEECH_USERNAME: &'static str = "cloud-speech-api-key";

    fn entry(username: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(Self::SERVICE, username)
            .map_err(|error| AppError::Settings(error.to_string()))
    }

    fn read(username: &str) -> Result<Option<String>, AppError> {
        match Self::entry(username)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }

    fn delete(username: &str) -> Result<(), AppError> {
        match Self::entry(username)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }
}

impl SecretStore for KeyringSecretStore {
    fn save_cloud_speech_api_key(&self, value: &str) -> Result<(), AppError> {
        Self::entry(Self::CLOUD_SPEECH_USERNAME)?
            .set_password(value)
            .map_err(|error| AppError::Settings(error.to_string()))
    }
    fn get_cloud_speech_api_key(&self) -> Result<Option<String>, AppError> {
        Self::read(Self::CLOUD_SPEECH_USERNAME)
    }
    fn delete_cloud_speech_api_key(&self) -> Result<(), AppError> {
        Self::delete(Self::CLOUD_SPEECH_USERNAME)
    }
    fn save_api_key(&self, value: &str) -> Result<(), AppError> {
        Self::entry(Self::USERNAME)?
            .set_password(value)
            .map_err(|error| AppError::Settings(error.to_string()))
    }

    fn get_api_key(&self) -> Result<Option<String>, AppError> {
        Self::read(Self::USERNAME)
    }

    fn delete_api_key(&self) -> Result<(), AppError> {
        Self::delete(Self::USERNAME)
    }

    fn save_cloud_ocr_api_key(&self, value: &str) -> Result<(), AppError> {
        Self::entry(Self::CLOUD_OCR_USERNAME)?
            .set_password(value)
            .map_err(|error| AppError::Settings(error.to_string()))
    }

    fn get_cloud_ocr_api_key(&self) -> Result<Option<String>, AppError> {
        Self::read(Self::CLOUD_OCR_USERNAME)
    }

    fn delete_cloud_ocr_api_key(&self) -> Result<(), AppError> {
        Self::delete(Self::CLOUD_OCR_USERNAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAI: &str = "https://api.openai.com/v1";

    #[test]
    fn saved_key_is_reused_only_for_the_service_it_was_saved_for() {
        let reuse = |target| key_for_endpoint(None, false, Some("saved"), OPENAI, target);
        assert_eq!(reuse(OPENAI).unwrap().as_deref(), Some("saved"));
        // Same service, different path or spelling.
        assert_eq!(
            reuse("https://API.openai.com:443/v2/").unwrap().as_deref(),
            Some("saved")
        );
        for elsewhere in [
            "https://api.deepseek.com/v1",
            "https://api.openai.com.example.net/v1",
            "https://api.openai.com:8443/v1",
            "http://localhost:11434/v1",
            "not a url",
        ] {
            assert!(reuse(elsewhere).is_err(), "{elsewhere}");
        }
    }

    #[test]
    fn an_entered_or_cleared_key_never_falls_back_to_the_saved_one() {
        let other = "https://api.deepseek.com/v1";
        assert_eq!(
            key_for_endpoint(Some(" new "), false, Some("saved"), OPENAI, other)
                .unwrap()
                .as_deref(),
            Some("new")
        );
        assert_eq!(
            key_for_endpoint(Some("new"), true, Some("saved"), OPENAI, other).unwrap(),
            None
        );
        assert_eq!(
            key_for_endpoint(Some("  "), false, None, OPENAI, other).unwrap(),
            None
        );
        // A feature that is switched off keeps its key for later.
        assert_eq!(
            key_for_endpoint(None, false, Some("saved"), OPENAI, "")
                .unwrap()
                .as_deref(),
            Some("saved")
        );
    }
}
