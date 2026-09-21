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

pub const CURRENT_SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub schema_version: u32,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SETTINGS_SCHEMA,
            provider: "OpenAI Compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            global_shortcut: "Alt+Q".into(),
            ocr_shortcut: "Alt+W".into(),
        }
    }
}

impl AppSettings {
    pub fn requires_api_key(&self) -> bool {
        reqwest::Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .is_none_or(|host| !is_loopback_host(Some(&host)))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
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
    pub ocr_shortcut: String,
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
        let (mut current, source_schema) = load_with_backup(&path)?;
        if source_schema < CURRENT_SETTINGS_SCHEMA {
            if path.exists() {
                fs::copy(&path, migration_backup_path(&path))
                    .map_err(|error| AppError::Settings(error.to_string()))?;
            }
            current.schema_version = CURRENT_SETTINGS_SCHEMA;
            let serialized = serde_json::to_vec_pretty(&current)
                .map_err(|error| AppError::Settings(error.to_string()))?;
            persist_atomically(&path, &serialized)?;
        }
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
            ocr_shortcut: settings.ocr_shortcut,
            api_key_configured: secrets.get_api_key()?.is_some(),
            auto_start_enabled,
        })
    }

    pub fn validate(update: &UpdateSettings) -> Result<AppSettings, AppError> {
        let provider = update.provider.trim().to_string();
        let model = update.model.trim().to_string();
        let global_shortcut = update.global_shortcut.trim().to_string();
        let ocr_shortcut = update.ocr_shortcut.trim().to_string();
        if provider.is_empty()
            || model.is_empty()
            || global_shortcut.is_empty()
            || ocr_shortcut.is_empty()
        {
            return Err(AppError::Settings("required settings are empty".into()));
        }
        if global_shortcut.eq_ignore_ascii_case(&ocr_shortcut) {
            return Err(AppError::Settings("划词翻译与 OCR 快捷键不能相同".into()));
        }

        Ok(AppSettings {
            schema_version: CURRENT_SETTINGS_SCHEMA,
            provider,
            base_url: validate_base_url(&update.base_url)?,
            model,
            global_shortcut,
            ocr_shortcut,
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

fn migration_backup_path(path: &Path) -> PathBuf {
    path.with_extension("pre-v1.json")
}

fn read_settings(path: &Path) -> Result<(AppSettings, u32), AppError> {
    let bytes = fs::read(path).map_err(|error| AppError::Settings(error.to_string()))?;
    let raw: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| AppError::Settings(error.to_string()))?;
    let source_schema = raw
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let settings =
        serde_json::from_value(raw).map_err(|error| AppError::Settings(error.to_string()))?;
    Ok((settings, source_schema))
}

fn load_with_backup(path: &Path) -> Result<(AppSettings, u32), AppError> {
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
    Ok((AppSettings::default(), CURRENT_SETTINGS_SCHEMA))
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
            ocr_shortcut: "Alt+W".into(),
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
    fn only_loopback_providers_can_run_without_api_key() {
        let remote = SettingsStore::validate(&update("https://example.com/v1")).unwrap();
        let local = SettingsStore::validate(&update("http://localhost:11434/v1")).unwrap();
        assert!(remote.requires_api_key());
        assert!(!local.requires_api_key());
    }

    #[test]
    fn migrates_missing_ocr_shortcut_and_rejects_duplicates() {
        let legacy = r#"{
          "provider": "OpenAI Compatible",
          "baseUrl": "https://api.openai.com/v1",
          "model": "gpt-4.1-mini",
          "globalShortcut": "Alt+Q"
        }"#;
        let migrated: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(migrated.ocr_shortcut, "Alt+W");
        assert_eq!(migrated.schema_version, CURRENT_SETTINGS_SCHEMA);

        let mut duplicate = update("https://example.com/v1");
        duplicate.ocr_shortcut = duplicate.global_shortcut.clone();
        assert!(SettingsStore::validate(&duplicate).is_err());
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

    #[test]
    fn upgrades_legacy_file_and_keeps_pre_v1_backup() {
        let path = std::env::temp_dir().join(format!(
            "quicktranslate-legacy-settings-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let legacy = r#"{
          "provider": "OpenAI Compatible",
          "baseUrl": "https://api.openai.com/v1",
          "model": "gpt-4.1-mini",
          "globalShortcut": "Alt+Q"
        }"#;
        fs::write(&path, legacy).unwrap();
        let store = SettingsStore::load(path.clone()).unwrap();
        assert_eq!(store.get().unwrap().schema_version, CURRENT_SETTINGS_SCHEMA);
        assert!(migration_backup_path(&path).exists());
        let _ = fs::remove_file(migration_backup_path(&path));
        let _ = fs::remove_file(path);
    }
}
