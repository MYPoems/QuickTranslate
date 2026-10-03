use super::{
    AppSettings, AppearancePreferences, OcrEngineKind, OcrLanguage, SettingsStore, UpdateSettings,
    CURRENT_SETTINGS_SCHEMA,
};
use crate::{errors::AppError, speech::SpeechPreferences};
use serde::{Deserialize, Serialize};

// Pure configuration parsing lives outside Tauri commands so migrations are testable.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsBackup {
    pub appearance: AppearancePreferences,
    pub speech: SpeechPreferences,
    pub schema_version: u32,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub global_shortcut: String,
    pub ocr_shortcut: String,
    pub ocr_engine: OcrEngineKind,
    pub ocr_language: OcrLanguage,
    pub cloud_ocr_base_url: String,
    pub cloud_ocr_model: String,
    pub auto_start_enabled: bool,
}

impl Default for SettingsBackup {
    fn default() -> Self {
        let settings = AppSettings::default();
        Self {
            appearance: settings.appearance,
            speech: settings.speech,
            schema_version: settings.schema_version,
            provider: settings.provider,
            base_url: settings.base_url,
            model: settings.model,
            global_shortcut: settings.global_shortcut,
            ocr_shortcut: settings.ocr_shortcut,
            ocr_engine: OcrEngineKind::Windows,
            ocr_language: settings.ocr_language,
            cloud_ocr_base_url: settings.cloud_ocr_base_url,
            cloud_ocr_model: settings.cloud_ocr_model,
            auto_start_enabled: false,
        }
    }
}

pub fn parse_settings_backup(contents: &str) -> Result<SettingsBackup, AppError> {
    let backup: SettingsBackup = serde_json::from_str(contents)
        .map_err(|error| AppError::Settings(format!("备份 JSON 无效：{error}")))?;
    if backup.schema_version > CURRENT_SETTINGS_SCHEMA {
        return Err(AppError::Settings(
            "该备份来自更高版本，当前版本无法导入".into(),
        ));
    }
    let normalized = SettingsStore::validate(&UpdateSettings {
        appearance: backup.appearance,
        speech: backup.speech,
        provider: backup.provider,
        base_url: backup.base_url,
        model: backup.model,
        global_shortcut: backup.global_shortcut,
        ocr_shortcut: backup.ocr_shortcut,
        ocr_engine: backup.ocr_engine,
        ocr_language: backup.ocr_language,
        cloud_ocr_base_url: backup.cloud_ocr_base_url,
        cloud_ocr_model: backup.cloud_ocr_model,
        api_key: None,
        clear_api_key: false,
        cloud_ocr_api_key: None,
        clear_cloud_ocr_api_key: false,
        cloud_speech_api_key: None,
        clear_cloud_speech_api_key: false,
        auto_start_enabled: backup.auto_start_enabled,
    })?;
    Ok(SettingsBackup {
        appearance: normalized.appearance,
        speech: normalized.speech,
        schema_version: CURRENT_SETTINGS_SCHEMA,
        provider: normalized.provider,
        base_url: normalized.base_url,
        model: normalized.model,
        global_shortcut: normalized.global_shortcut,
        ocr_shortcut: normalized.ocr_shortcut,
        ocr_engine: normalized.ocr_engine,
        ocr_language: normalized.ocr_language,
        cloud_ocr_base_url: normalized.cloud_ocr_base_url,
        cloud_ocr_model: normalized.cloud_ocr_model,
        auto_start_enabled: backup.auto_start_enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_backup_defaults_appearance_and_new_backup_preserves_it() {
        let fixture = include_str!("../../../tests/fixtures/settings-v1.0.json");
        let old = parse_settings_backup(fixture).unwrap();
        assert_eq!(old.appearance, AppearancePreferences::default());
        let mut raw = serde_json::to_value(old).unwrap();
        raw["appearance"] = serde_json::json!({"theme":"dark","popupOpacity":80});
        let imported = parse_settings_backup(&raw.to_string()).unwrap();
        assert_eq!(
            serde_json::to_value(imported.appearance).unwrap(),
            raw["appearance"]
        );
        raw["appearance"]["popupOpacity"] = serde_json::json!(20);
        assert!(parse_settings_backup(&raw.to_string()).is_err());
        assert!(!raw.to_string().contains("apiKey"));
        raw["schemaVersion"] = serde_json::json!(999);
        assert!(parse_settings_backup(&raw.to_string()).is_err());
    }
}
