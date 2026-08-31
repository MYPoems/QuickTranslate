use crate::errors::AppError;

pub trait SecretStore: Send + Sync {
    fn save_api_key(&self, value: &str) -> Result<(), AppError>;
    fn get_api_key(&self) -> Result<Option<String>, AppError>;
    fn delete_api_key(&self) -> Result<(), AppError>;
}

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    const SERVICE: &'static str = "QuickTranslate";
    const USERNAME: &'static str = "openai-compatible-api-key";

    fn entry() -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(Self::SERVICE, Self::USERNAME)
            .map_err(|error| AppError::Settings(error.to_string()))
    }
}

impl SecretStore for KeyringSecretStore {
    fn save_api_key(&self, value: &str) -> Result<(), AppError> {
        Self::entry()?
            .set_password(value)
            .map_err(|error| AppError::Settings(error.to_string()))
    }

    fn get_api_key(&self) -> Result<Option<String>, AppError> {
        match Self::entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }

    fn delete_api_key(&self) -> Result<(), AppError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }
}
