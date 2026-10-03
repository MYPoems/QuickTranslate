use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{errors::AppError, security::SecretStore, speech::SpeechPreferences};

pub const CURRENT_SETTINGS_SCHEMA: u32 = 5;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppearanceTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearancePreferences {
    pub theme: AppearanceTheme,
    pub popup_opacity: u8,
}

impl Default for AppearancePreferences {
    fn default() -> Self {
        Self {
            theme: AppearanceTheme::System,
            popup_opacity: 96,
        }
    }
}

impl AppearancePreferences {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(70..=100).contains(&self.popup_opacity) {
            return Err(AppError::Settings(
                "悬浮窗背景不透明度必须在 70% 到 100% 之间".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OcrEngineKind {
    #[default]
    Windows,
    Paddle,
    Cloud,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OcrLanguage {
    #[default]
    Auto,
    Chinese,
    English,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub appearance: AppearancePreferences,
    pub schema_version: u32,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
    #[serde(default = "legacy_ocr_engine")]
    pub ocr_engine: OcrEngineKind,
    pub ocr_language: OcrLanguage,
    pub cloud_ocr_base_url: String,
    pub cloud_ocr_model: String,
    pub speech: SpeechPreferences,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: AppearancePreferences::default(),
            schema_version: CURRENT_SETTINGS_SCHEMA,
            provider: "OpenAI Compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            global_shortcut: "Alt+Q".into(),
            ocr_shortcut: "Alt+W".into(),
            ocr_engine: OcrEngineKind::Cloud,
            ocr_language: OcrLanguage::Auto,
            cloud_ocr_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            cloud_ocr_model: "qwen3.5-ocr".into(),
            speech: SpeechPreferences::default(),
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

fn legacy_ocr_engine() -> OcrEngineKind {
    OcrEngineKind::Windows
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub appearance: AppearancePreferences,
    pub speech: SpeechPreferences,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
    pub ocr_engine: OcrEngineKind,
    pub ocr_language: OcrLanguage,
    pub cloud_ocr_base_url: String,
    pub cloud_ocr_model: String,
    pub api_key_configured: bool,
    pub cloud_ocr_api_key_configured: bool,
    pub cloud_speech_api_key_configured: bool,
    pub paddle_ocr_installed: bool,
    pub auto_start_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default)]
    pub appearance: AppearancePreferences,
    #[serde(default)]
    pub speech: SpeechPreferences,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
    pub ocr_engine: OcrEngineKind,
    pub ocr_language: OcrLanguage,
    pub cloud_ocr_base_url: String,
    pub cloud_ocr_model: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
    pub cloud_ocr_api_key: Option<String>,
    #[serde(default)]
    pub clear_cloud_ocr_api_key: bool,
    pub cloud_speech_api_key: Option<String>,
    #[serde(default)]
    pub clear_cloud_speech_api_key: bool,
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
                fs::copy(&path, migration_backup_path(&path, source_schema))
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
        paddle_ocr_installed: bool,
    ) -> Result<SettingsView, AppError> {
        let settings = self.get()?;
        Ok(SettingsView {
            appearance: settings.appearance,
            speech: settings.speech,
            provider: settings.provider,
            base_url: settings.base_url,
            model: settings.model,
            global_shortcut: settings.global_shortcut,
            ocr_shortcut: settings.ocr_shortcut,
            ocr_engine: settings.ocr_engine,
            ocr_language: settings.ocr_language,
            cloud_ocr_base_url: settings.cloud_ocr_base_url,
            cloud_ocr_model: settings.cloud_ocr_model,
            api_key_configured: secrets.get_api_key()?.is_some(),
            cloud_ocr_api_key_configured: secrets.get_cloud_ocr_api_key()?.is_some(),
            cloud_speech_api_key_configured: secrets.get_cloud_speech_api_key()?.is_some(),
            paddle_ocr_installed,
            auto_start_enabled,
        })
    }

    pub fn validate(update: &UpdateSettings) -> Result<AppSettings, AppError> {
        update.appearance.validate()?;
        update.speech.validate()?;
        let provider = update.provider.trim().to_string();
        let model = update.model.trim().to_string();
        let global_shortcut = update.global_shortcut.trim().to_string();
        let ocr_shortcut = update.ocr_shortcut.trim().to_string();
        let cloud_ocr_model = update.cloud_ocr_model.trim().to_string();
        let cloud_ocr_base_url = update.cloud_ocr_base_url.trim();
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
        if update.ocr_engine == OcrEngineKind::Cloud
            && (cloud_ocr_base_url.is_empty() || cloud_ocr_model.is_empty())
        {
            return Err(AppError::Settings(
                "云端视觉 OCR 需要 Base URL 和模型名称".into(),
            ));
        }
        let cloud_ocr_base_url = if cloud_ocr_base_url.is_empty() {
            String::new()
        } else {
            validate_base_url(cloud_ocr_base_url)?
        };

        Ok(AppSettings {
            appearance: update.appearance.clone(),
            speech: update.speech.clone(),
            schema_version: CURRENT_SETTINGS_SCHEMA,
            provider,
            base_url: validate_base_url(&update.base_url)?,
            model,
            global_shortcut,
            ocr_shortcut,
            ocr_engine: update.ocr_engine,
            ocr_language: update.ocr_language,
            cloud_ocr_base_url,
            cloud_ocr_model,
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

fn migration_backup_path(path: &Path, source_schema: u32) -> PathBuf {
    path.with_extension(format!("pre-v{source_schema}.json"))
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
    if source_schema > CURRENT_SETTINGS_SCHEMA {
        return Err(AppError::Settings(
            "设置来自更高版本，拒绝降级覆盖；请恢复升级前备份或安装新版本".into(),
        ));
    }
    let settings =
        serde_json::from_value(raw).map_err(|error| AppError::Settings(error.to_string()))?;
    Ok((settings, source_schema))
}

fn load_with_backup(path: &Path) -> Result<(AppSettings, u32), AppError> {
    if path.exists() {
        // A future schema is not corruption. Never restore an older backup over it.
        let raw = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if raw
            .as_ref()
            .and_then(|value| value.get("schemaVersion"))
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version > u64::from(CURRENT_SETTINGS_SCHEMA))
        {
            return Err(AppError::Settings("设置来自更高版本，拒绝降级覆盖".into()));
        }
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

    #[test]
    fn v15_appearance_migration_preserves_configuration_and_exact_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut before = AppSettings {
            schema_version: 4,
            model: "custom-model".into(),
            ocr_engine: OcrEngineKind::Paddle,
            speech: SpeechPreferences {
                english_voice: "my-voice".into(),
                rate: 125,
                ..SpeechPreferences::default()
            },
            ..AppSettings::default()
        };
        let mut raw = serde_json::to_value(&before).unwrap();
        raw.as_object_mut().unwrap().remove("appearance");
        let bytes = serde_json::to_vec_pretty(&raw).unwrap();
        fs::write(&path, &bytes).unwrap();
        let upgraded = SettingsStore::load(path.clone()).unwrap().get().unwrap();
        before.schema_version = CURRENT_SETTINGS_SCHEMA;
        assert_eq!(upgraded, before);
        assert_eq!(fs::read(migration_backup_path(&path, 4)).unwrap(), bytes);
        assert_eq!(SettingsStore::load(path).unwrap().get().unwrap(), upgraded);
    }

    #[test]
    fn appearance_round_trip_and_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let store = SettingsStore::load(path.clone()).unwrap();
        for theme in [
            AppearanceTheme::Light,
            AppearanceTheme::Dark,
            AppearanceTheme::System,
        ] {
            let candidate = SettingsStore::validate(&UpdateSettings {
                appearance: AppearancePreferences {
                    theme,
                    popup_opacity: 70,
                },
                ..update("https://example.com/v1")
            })
            .unwrap();
            store.replace(candidate.clone()).unwrap();
            assert_eq!(
                SettingsStore::load(path.clone()).unwrap().get().unwrap(),
                candidate
            );
        }
        for popup_opacity in [0, 69, 101, 255] {
            assert!(SettingsStore::validate(&UpdateSettings {
                appearance: AppearancePreferences {
                    popup_opacity,
                    ..AppearancePreferences::default()
                },
                ..update("https://example.com/v1")
            })
            .is_err());
        }
        assert!(serde_json::from_str::<AppearancePreferences>(r#"{"theme":"invalid"}"#).is_err());
    }

    #[test]
    fn v12_speech_migration_defaults_cloud_but_preserves_local_voice_and_history_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let old = r#"{"schemaVersion":3,"model":"my-model","globalShortcut":"Alt+Q","ocrShortcut":"Alt+W","speech":{"rate":125,"chineseVoice":"plugin:kokoro:58","englishVoice":"my-en-voice","bilingual":true}}"#;
        fs::write(&path, old).unwrap();
        let upgraded = SettingsStore::load(path.clone()).unwrap().get().unwrap();
        assert_eq!(
            upgraded.speech.provider,
            crate::speech::SpeechProvider::Cloud
        );
        assert_eq!(upgraded.speech.chinese_voice, "plugin:kokoro:58");
        assert_eq!(upgraded.speech.english_voice, "my-en-voice");
        assert_eq!(upgraded.speech.rate, 125);
        assert!(upgraded.speech.bilingual);
        assert_eq!(upgraded.model, "my-model");
        assert_eq!(
            fs::read_to_string(migration_backup_path(&path, 3)).unwrap(),
            old
        );
        assert_eq!(SettingsStore::load(path).unwrap().get().unwrap(), upgraded);
    }

    #[test]
    fn v1_upgrade_preserves_every_existing_setting_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let fixture = include_str!("../../../tests/fixtures/settings-v1.0.json");
        fs::write(&path, fixture).unwrap();
        let before: AppSettings = serde_json::from_str(fixture).unwrap();
        let upgraded = SettingsStore::load(path.clone()).unwrap().get().unwrap();
        assert_eq!(
            upgraded,
            AppSettings {
                schema_version: CURRENT_SETTINGS_SCHEMA,
                ..before
            }
        );
        assert_eq!(
            fs::read_to_string(migration_backup_path(&path, 2)).unwrap(),
            fixture
        );
        assert_eq!(upgraded.speech, SpeechPreferences::default());
        let first = fs::read(&path).unwrap();
        let reopened = SettingsStore::load(path.clone()).unwrap().get().unwrap();
        assert_eq!(upgraded, reopened);
        assert_eq!(fs::read(&path).unwrap(), first);
        assert_eq!(
            fs::read_to_string(migration_backup_path(&path, 2)).unwrap(),
            fixture
        );
    }

    #[test]
    fn legacy_local_ocr_is_not_silently_changed_to_cloud() {
        let directory = tempfile::tempdir().unwrap();
        for engine in ["windows", "paddle"] {
            let path = directory.path().join(format!("{engine}.json"));
            fs::write(
                &path,
                format!(r#"{{"schemaVersion":2,"ocrEngine":"{engine}"}}"#),
            )
            .unwrap();
            let migrated = SettingsStore::load(path).unwrap().get().unwrap();
            assert_eq!(serde_json::to_value(migrated.ocr_engine).unwrap(), engine);
        }
        let path = directory.path().join("legacy.json");
        fs::write(&path, r#"{"model":"custom-model"}"#).unwrap();
        assert_eq!(
            SettingsStore::load(path).unwrap().get().unwrap().ocr_engine,
            OcrEngineKind::Windows
        );
        assert_eq!(AppSettings::default().ocr_engine, OcrEngineKind::Cloud);
    }

    #[test]
    fn voice_preferences_and_keys_privacy_survive_save_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let store = SettingsStore::load(path.clone()).unwrap();
        let candidate = AppSettings {
            speech: SpeechPreferences {
                rate: 125,
                chinese_voice: "Chinese-test".into(),
                english_voice: "English-test".into(),
                bilingual: true,
                ..SpeechPreferences::default()
            },
            ..AppSettings::default()
        };
        store.replace(candidate.clone()).unwrap();
        assert_eq!(
            SettingsStore::load(path.clone()).unwrap().get().unwrap(),
            candidate
        );
        let serialized = fs::read_to_string(path).unwrap();
        assert!(!serialized.contains("apiKey"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn future_schema_is_not_overwritten_even_with_old_recovery_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let future = r#"{"schemaVersion":999,"model":"future-model","futureField":"preserve"}"#;
        fs::write(&path, future).unwrap();
        fs::write(backup_path(&path), "{}").unwrap();
        assert!(SettingsStore::load(path.clone()).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), future);
    }

    #[test]
    fn corrupt_or_missing_primary_recovers_valid_atomic_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let fixture = include_str!("../../../tests/fixtures/settings-v1.0.json");
        fs::write(&path, "{incomplete").unwrap();
        fs::write(backup_path(&path), fixture).unwrap();
        assert_eq!(
            SettingsStore::load(path.clone())
                .unwrap()
                .get()
                .unwrap()
                .model,
            "qwen-turbo"
        );
        fs::remove_file(&path).unwrap();
        fs::write(backup_path(&path), fixture).unwrap();
        assert_eq!(
            SettingsStore::load(path).unwrap().get().unwrap().ocr_engine,
            OcrEngineKind::Cloud
        );
    }

    fn update(base_url: &str) -> UpdateSettings {
        UpdateSettings {
            appearance: AppearancePreferences::default(),
            speech: SpeechPreferences::default(),
            provider: "OpenAI Compatible".into(),
            base_url: base_url.into(),
            model: "test-model".into(),
            global_shortcut: "Alt+Q".into(),
            ocr_shortcut: "Alt+W".into(),
            ocr_engine: OcrEngineKind::Windows,
            ocr_language: OcrLanguage::Auto,
            cloud_ocr_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            cloud_ocr_model: "qwen3.5-ocr".into(),
            api_key: None,
            clear_api_key: false,
            cloud_ocr_api_key: None,
            clear_cloud_ocr_api_key: false,
            cloud_speech_api_key: None,
            clear_cloud_speech_api_key: false,
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
        assert_eq!(migrated.ocr_engine, OcrEngineKind::Windows);
        assert_eq!(migrated.ocr_language, OcrLanguage::Auto);
        assert_eq!(migrated.cloud_ocr_model, "qwen3.5-ocr");
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
    fn upgrades_legacy_file_and_keeps_versioned_backup() {
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
        assert!(migration_backup_path(&path, 0).exists());
        let _ = fs::remove_file(migration_backup_path(&path, 0));
        let _ = fs::remove_file(path);
    }
}
