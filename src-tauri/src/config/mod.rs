mod backup;
mod settings;
pub use backup::{parse_settings_backup, SettingsBackup};

pub use settings::{
    AppSettings, AppearancePreferences, OcrEngineKind, OcrLanguage, SettingsStore, SettingsView,
    UpdateSettings, CURRENT_SETTINGS_SCHEMA,
};
