use std::{fs, path::PathBuf, sync::RwLock};

use serde::{Deserialize, Serialize};

use crate::{errors::AppError, security::SecretStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let current = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| AppError::Settings(error.to_string()))?;
            serde_json::from_slice(&bytes).map_err(|error| AppError::Settings(error.to_string()))?
        } else {
            AppSettings::default()
        };
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

    pub fn update(&self, update: &UpdateSettings) -> Result<AppSettings, AppError> {
        let base_url = update.base_url.trim().trim_end_matches('/').to_string();
        let model = update.model.trim().to_string();
        let global_shortcut = update.global_shortcut.trim().to_string();
        if base_url.is_empty() || model.is_empty() || global_shortcut.is_empty() {
            return Err(AppError::Settings("required settings are empty".into()));
        }

        let settings = AppSettings {
            provider: update.provider.trim().to_string(),
            base_url,
            model,
            global_shortcut,
        };
        let serialized = serde_json::to_vec_pretty(&settings)
            .map_err(|error| AppError::Settings(error.to_string()))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::Settings(error.to_string()))?;
        }
        fs::write(&self.path, serialized).map_err(|error| AppError::Settings(error.to_string()))?;
        *self
            .current
            .write()
            .map_err(|_| AppError::Settings("settings lock poisoned".into()))? = settings.clone();
        Ok(settings)
    }
}
