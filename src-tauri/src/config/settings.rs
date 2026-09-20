use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{errors::AppError, security::SecretStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: "OpenAI Compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            global_shortcut: "Alt+Q".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub api_key_configured: bool,
    pub auto_start_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    #[serde(default)]
    pub auto_start_enabled: bool,
}

pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<AppSettings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Result<Self, AppError> {
        let current = load_with_backup(&path)?;
        Ok(Self {
            path,
            current: RwLock::new(current),
        })
    }

    pub fn get(&self) -> Result<AppSettings, AppError> {
        self.current
            .read()
            .map(|settings| settings.clone())
            .map_err(|_| AppError::Settings("settings lock poisoned".into()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn view(
        &self,
        secrets: &dyn SecretStore,
        auto_start_enabled: bool,
    ) -> Result<SettingsView, AppError> {
        let settings = self.get()?;
        Ok(SettingsView {
            provider: settings.provider,
            base_url: settings.base_url,
            model: settings.model,
            global_shortcut: settings.global_shortcut,
            api_key_configured: secrets.get_api_key()?.is_some(),
            auto_start_enabled,
        })
    }

    pub fn validate(update: &UpdateSettings) -> Result<AppSettings, AppError> {
        let provider = update.provider.trim().to_string();
        let model = update.model.trim().to_string();
        let global_shortcut = update.global_shortcut.trim().to_string();
        if provider.is_empty() || model.is_empty() || global_shortcut.is_empty() {
            return Err(AppError::Settings("required settings are empty".into()));
        }

        Ok(AppSettings {
            provider,
            base_url: validate_base_url(&update.base_url)?,
            model,
            global_shortcut,
        })
    }

    pub fn replace(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        let serialized = serde_json::to_vec_pretty(&settings)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        persist_atomically(&self.path, &serialized)?;
        *self
            .current
            .write()
            .map_err(|_| AppError::Settings("settings lock poisoned".into()))? = settings.clone();
        Ok(settings)
    }
}

fn validate_base_url(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim().trim_end_matches('/');
    let url =
        reqwest::Url::parse(trimmed).map_err(|_| AppError::Settings("Base URL 格式无效".into()))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Settings("Base URL 不允许包含用户名或密码".into()));
    }

    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(url.host_str()) => {}
        "http" => {
            return Err(AppError::Settings(
                "远程 Provider 必须使用 HTTPS；HTTP 仅允许 localhost".into(),
            ));
        }
        _ => {
            return Err(AppError::Settings(
                "Base URL 仅支持 HTTPS，或用于本地模型的 HTTP localhost".into(),
            ));
        }
    }

    if url.host_str().is_none() {
        return Err(AppError::Settings("Base URL 缺少主机名".into()));
    }
    Ok(trimmed.to_string())
}

fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn read_settings(path: &Path) -> Result<AppSettings, AppError> {
    let bytes = fs::read(path).map_err(|error| AppError::Settings(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AppError::Settings(error.to_string()))
}

fn load_with_backup(path: &Path) -> Result<AppSettings, AppError> {
    if path.exists() {
        match read_settings(path) {
            Ok(settings) => return Ok(settings),
            Err(primary_error) => {
                let backup = backup_path(path);
                if backup.exists() {
                    let settings = read_settings(&backup)?;
                    fs::copy(&backup, path)
                        .map_err(|error| AppError::Settings(error.to_string()))?;
                    return Ok(settings);
                }
                return Err(primary_error);
            }
        }
    }

    let backup = backup_path(path);
    if backup.exists() {
        let settings = read_settings(&backup)?;
        fs::copy(&backup, path).map_err(|error| AppError::Settings(error.to_string()))?;
        return Ok(settings);
    }
    Ok(AppSettings::default())
}

fn persist_atomically(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Settings("settings path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| AppError::Settings(error.to_string()))?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(".settings-{}-{nonce}.tmp", std::process::id()));
    let backup = backup_path(path);
    let write_result = (|| -> Result<(), AppError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| AppError::Settings(error.to_string()))?;

        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| AppError::Settings(error.to_string()))?;
        }
        let had_current = path.exists();
        if had_current {
            fs::rename(path, &backup).map_err(|error| AppError::Settings(error.to_string()))?;
        }
        if let Err(error) = fs::rename(&temp, path) {
            if had_current {
                let _ = fs::rename(&backup, path);
            }
            return Err(AppError::Settings(error.to_string()));
        }
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| AppError::Settings(error.to_string()))?;
        }
        Ok(())
    })();

    if temp.exists() {
        let _ = fs::remove_file(temp);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(base_url: &str) -> UpdateSettings {
        UpdateSettings {
            provider: "OpenAI Compatible".into(),
            base_url: base_url.into(),
            model: "test-model".into(),
            global_shortcut: "Alt+Q".into(),
            api_key: None,
            clear_api_key: false,
            auto_start_enabled: false,
        }
    }

    #[test]
    fn accepts_https_and_loopback_http_urls() {
        assert!(SettingsStore::validate(&update("https://example.com/v1/")).is_ok());
        assert!(SettingsStore::validate(&update("http://localhost:11434/v1")).is_ok());
        assert!(SettingsStore::validate(&update("http://127.0.0.1:1234/v1")).is_ok());
        assert!(SettingsStore::validate(&update("http://[::1]:1234/v1")).is_ok());
    }

    #[test]
    fn rejects_insecure_remote_and_credential_urls() {
        assert!(SettingsStore::validate(&update("http://example.com/v1")).is_err());
        assert!(SettingsStore::validate(&update("https://user:secret@example.com/v1")).is_err());
        assert!(SettingsStore::validate(&update("file:///tmp/provider")).is_err());
    }

    #[test]
    fn persists_and_loads_settings() {
        let path = std::env::temp_dir().join(format!(
            "quicktranslate-settings-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = SettingsStore::load(path.clone()).unwrap();
        let expected = SettingsStore::validate(&update("https://example.com/v1/")).unwrap();
        store.replace(expected.clone()).unwrap();
        assert_eq!(
            SettingsStore::load(path.clone()).unwrap().get().unwrap(),
            expected
        );
        let _ = fs::remove_file(path);
    }
}
