use crate::{config::AppSettings, errors::AppError};

pub trait SecretStore: Send + Sync {
    fn save_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_api_key(&self) -> Result<(), AppError>;
    fn save_cloud_ocr_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_cloud_ocr_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_cloud_ocr_api_key(&self) -> Result<(), AppError>;
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

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    const SERVICE: &'static str = "QuickTranslate";
    const USERNAME: &'static str = "openai-compatible-api-key";
    const CLOUD_OCR_USERNAME: &'static str = "cloud-ocr-api-key";

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
