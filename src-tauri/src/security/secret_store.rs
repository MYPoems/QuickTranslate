use sha2::{Digest, Sha256};

use crate::{config::AppSettings, errors::AppError};

/// Raw access to the credential store by account name. Callers go through [`SecretKind`]
/// and the functions below, which decide which account a key lives under.
pub trait SecretStore: Send + Sync {
    fn read(&self, account: &str) -> Result<Option<String>, AppError>;
    fn write(&self, account: &str, value: &str) -> Result<(), AppError>;
    fn remove(&self, account: &str) -> Result<(), AppError>;
}

/// The three services that take their own API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Translation,
    CloudOcr,
    CloudSpeech,
}

impl SecretKind {
    pub const ALL: [Self; 3] = [Self::Translation, Self::CloudOcr, Self::CloudSpeech];

    /// The account name used before keys were bound to an address. Still read once, to move
    /// an existing key to the address it was being used with.
    fn legacy_account(self) -> &'static str {
        match self {
            Self::Translation => "openai-compatible-api-key",
            Self::CloudOcr => "cloud-ocr-api-key",
            Self::CloudSpeech => "cloud-speech-api-key",
        }
    }

    /// The address this kind of key is currently used with.
    pub fn endpoint(self, settings: &AppSettings) -> &str {
        match self {
            Self::Translation => &settings.base_url,
            Self::CloudOcr => &settings.cloud_ocr_base_url,
            Self::CloudSpeech => &settings.speech.cloud_endpoint,
        }
    }

    /// A key belongs to one service address: scheme, host and port. Requests to any other
    /// address never see it. `None` when the address is empty or not a URL.
    fn account(self, endpoint: &str) -> Option<String> {
        let url = reqwest::Url::parse(endpoint.trim()).ok()?;
        let host = url.host_str()?.to_ascii_lowercase();
        let port = url
            .port_or_known_default()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        Some(format!(
            "{}@{}://{host}{port}",
            self.legacy_account(),
            url.scheme()
        ))
    }

    fn migration_marker(self) -> String {
        format!("{}#moved", self.legacy_account())
    }
}

/// The key saved for `endpoint`, if any.
pub fn get_key(
    secrets: &dyn SecretStore,
    kind: SecretKind,
    endpoint: &str,
) -> Result<Option<String>, AppError> {
    match kind.account(endpoint) {
        Some(account) => Ok(secrets
            .read(&account)?
            .filter(|value| !value.trim().is_empty())),
        None => Ok(None),
    }
}

pub fn save_key(
    secrets: &dyn SecretStore,
    kind: SecretKind,
    endpoint: &str,
    value: &str,
) -> Result<(), AppError> {
    let account = kind
        .account(endpoint)
        .ok_or_else(|| AppError::Settings("no service address to save the key for".into()))?;
    secrets.write(&account, value)
}

pub fn delete_key(
    secrets: &dyn SecretStore,
    kind: SecretKind,
    endpoint: &str,
) -> Result<(), AppError> {
    match kind.account(endpoint) {
        Some(account) => secrets.remove(&account),
        None => Ok(()),
    }
}

/// Puts a key back to what it was: `Some` saves it, `None` removes it.
pub fn restore_key(
    secrets: &dyn SecretStore,
    kind: SecretKind,
    endpoint: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    match value {
        Some(value) => save_key(secrets, kind, endpoint, value),
        None => delete_key(secrets, kind, endpoint),
    }
}

/// Earlier versions kept one key per kind, whatever address it was for. Moves each such key
/// to the address the settings point at now, which is the one it was being sent to.
///
/// The old entry is left in place so that an older version still finds it. A marker with the
/// key's fingerprint keeps this from running again later, when the settings may point
/// somewhere else; it does run again if an older version has since saved a different key.
pub fn adopt_legacy_keys(
    secrets: &dyn SecretStore,
    settings: &AppSettings,
) -> Result<(), AppError> {
    for kind in SecretKind::ALL {
        let Some(legacy) = secrets
            .read(kind.legacy_account())?
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let fingerprint = hex::encode(Sha256::digest(legacy.as_bytes()));
        let marker = kind.migration_marker();
        if secrets.read(&marker)?.as_deref() == Some(fingerprint.as_str()) {
            continue;
        }
        // Without a usable address there is nowhere to put it yet; try again next start.
        let Some(account) = kind.account(kind.endpoint(settings)) else {
            continue;
        };
        secrets.write(&account, &legacy)?;
        secrets.write(&marker, &fingerprint)?;
    }
    Ok(())
}

/// The translation key for the configured address, or an empty string for a local model
/// that needs none.
pub fn provider_api_key(
    settings: &AppSettings,
    secrets: &dyn SecretStore,
) -> Result<String, AppError> {
    match get_key(secrets, SecretKind::Translation, &settings.base_url)? {
        Some(value) => Ok(value),
        None if !settings.requires_api_key() => Ok(String::new()),
        None => Err(AppError::ProviderNotConfigured),
    }
}

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    const SERVICE: &'static str = "QuickTranslate";

    fn entry(account: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(Self::SERVICE, account)
            .map_err(|error| AppError::Settings(error.to_string()))
    }
}

impl SecretStore for KeyringSecretStore {
    fn read(&self, account: &str) -> Result<Option<String>, AppError> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }

    fn write(&self, account: &str, value: &str) -> Result<(), AppError> {
        Self::entry(account)?
            .set_password(value)
            .map_err(|error| AppError::Settings(error.to_string()))
    }

    fn remove(&self, account: &str) -> Result<(), AppError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Settings(error.to_string())),
        }
    }
}

/// An in-memory store for tests and the isolated QA mode.
#[derive(Default)]
pub struct MemorySecretStore(std::sync::Mutex<std::collections::BTreeMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn read(&self, account: &str) -> Result<Option<String>, AppError> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }

    fn write(&self, account: &str, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().insert(account.into(), value.into());
        Ok(())
    }

    fn remove(&self, account: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAI: &str = "https://api.openai.com/v1";
    const DEEPSEEK: &str = "https://api.deepseek.com/v1";

    fn settings(base_url: &str) -> AppSettings {
        AppSettings {
            base_url: base_url.into(),
            ..AppSettings::default()
        }
    }

    #[test]
    fn a_key_is_only_visible_from_the_address_it_was_saved_for() {
        let store = MemorySecretStore::default();
        save_key(&store, SecretKind::Translation, OPENAI, "openai-key").unwrap();
        let read = |endpoint| get_key(&store, SecretKind::Translation, endpoint).unwrap();

        assert_eq!(read(OPENAI).as_deref(), Some("openai-key"));
        // Same service: another path, another spelling of host and port.
        assert_eq!(
            read("https://API.openai.com:443/v2/").as_deref(),
            Some("openai-key")
        );
        for elsewhere in [
            DEEPSEEK,
            "https://api.openai.com.example.net/v1",
            "https://api.openai.com:8443/v1",
            "http://api.openai.com/v1",
            "http://localhost:11434/v1",
            "not a url",
            "",
        ] {
            assert_eq!(read(elsewhere), None, "{elsewhere}");
        }
        // The same address under another kind is a different key.
        assert_eq!(get_key(&store, SecretKind::CloudOcr, OPENAI).unwrap(), None);
    }

    #[test]
    fn each_address_keeps_its_own_key_across_switches() {
        let store = MemorySecretStore::default();
        save_key(&store, SecretKind::Translation, OPENAI, "openai-key").unwrap();
        save_key(&store, SecretKind::Translation, DEEPSEEK, "deepseek-key").unwrap();

        assert_eq!(
            provider_api_key(&settings(OPENAI), &store).unwrap(),
            "openai-key"
        );
        assert_eq!(
            provider_api_key(&settings(DEEPSEEK), &store).unwrap(),
            "deepseek-key"
        );
        delete_key(&store, SecretKind::Translation, DEEPSEEK).unwrap();
        assert!(matches!(
            provider_api_key(&settings(DEEPSEEK), &store),
            Err(AppError::ProviderNotConfigured)
        ));
        assert_eq!(
            provider_api_key(&settings(OPENAI), &store).unwrap(),
            "openai-key"
        );
        // A local model needs no key and must not borrow one.
        assert_eq!(
            provider_api_key(&settings("http://localhost:11434/v1"), &store).unwrap(),
            ""
        );
    }

    #[test]
    fn a_legacy_key_moves_once_to_the_address_in_use_and_stays_readable_by_old_versions() {
        let store = MemorySecretStore::default();
        store.write("openai-compatible-api-key", "old-key").unwrap();

        adopt_legacy_keys(&store, &settings(OPENAI)).unwrap();
        assert_eq!(
            provider_api_key(&settings(OPENAI), &store).unwrap(),
            "old-key"
        );
        assert_eq!(
            store.read("openai-compatible-api-key").unwrap().as_deref(),
            Some("old-key"),
            "an older version can still read it"
        );

        // Later the settings point elsewhere: the old key must not follow.
        adopt_legacy_keys(&store, &settings(DEEPSEEK)).unwrap();
        assert!(provider_api_key(&settings(DEEPSEEK), &store).is_err());

        // An older version saved a different key in the meantime, for the address now in use.
        store
            .write("openai-compatible-api-key", "newer-key")
            .unwrap();
        adopt_legacy_keys(&store, &settings(DEEPSEEK)).unwrap();
        assert_eq!(
            provider_api_key(&settings(DEEPSEEK), &store).unwrap(),
            "newer-key"
        );
        assert_eq!(
            provider_api_key(&settings(OPENAI), &store).unwrap(),
            "old-key"
        );
    }

    #[test]
    fn a_legacy_key_waits_when_there_is_no_address_to_bind_it_to() {
        let store = MemorySecretStore::default();
        store.write("cloud-ocr-api-key", "ocr-key").unwrap();
        let mut current = settings(OPENAI);
        current.cloud_ocr_base_url = String::new();
        adopt_legacy_keys(&store, &current).unwrap();

        current.cloud_ocr_base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1".into();
        adopt_legacy_keys(&store, &current).unwrap();
        assert_eq!(
            get_key(&store, SecretKind::CloudOcr, &current.cloud_ocr_base_url)
                .unwrap()
                .as_deref(),
            Some("ocr-key")
        );
    }
}
