use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    app::{AppState, DiagnosticError},
    config::{
        AppSettings, OcrEngineKind, OcrLanguage, SettingsStore, SettingsView, UpdateSettings,
        CURRENT_SETTINGS_SCHEMA,
    },
    errors::AppError,
    security::provider_api_key,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsView {
    app_version: &'static str,
    provider: String,
    base_url: String,
    model: String,
    ocr_engine: OcrEngineKind,
    ocr_language: OcrLanguage,
    api_key_configured: bool,
    cloud_ocr_model: String,
    cloud_ocr_api_key_configured: bool,
    paddle_ocr_installed: bool,
    cache_entries: i64,
    settings_path: String,
    cache_path: String,
    last_error: Option<DiagnosticError>,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsBackup {
    schema_version: u32,
    provider: String,
    base_url: String,
    model: String,
    global_shortcut: String,
    ocr_shortcut: String,
    ocr_engine: OcrEngineKind,
    ocr_language: OcrLanguage,
    cloud_ocr_base_url: String,
    cloud_ocr_model: String,
    auto_start_enabled: bool,
}

impl Default for SettingsBackup {
    fn default() -> Self {
        let settings = AppSettings::default();
        Self {
            schema_version: settings.schema_version,
            provider: settings.provider,
            base_url: settings.base_url,
            model: settings.model,
            global_shortcut: settings.global_shortcut,
            ocr_shortcut: settings.ocr_shortcut,
            ocr_engine: settings.ocr_engine,
            ocr_language: settings.ocr_language,
            cloud_ocr_base_url: settings.cloud_ocr_base_url,
            cloud_ocr_model: settings.cloud_ocr_model,
            auto_start_enabled: false,
        }
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
        crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
    )
}

#[tauri::command]
pub fn save_settings(update: UpdateSettings, app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    let candidate = SettingsStore::validate(&update)?;
    let previous = state.settings.get()?;
    let previous_api_key = state.secrets.get_api_key()?;
    let previous_cloud_ocr_api_key = state.secrets.get_cloud_ocr_api_key()?;
    let previous_auto_start = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| AppError::AutoStart(error.to_string()))?;

    let desired_api_key = if update.clear_api_key {
        None
    } else {
        update
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .or_else(|| previous_api_key.clone())
    };
    let desired_cloud_ocr_api_key = if update.clear_cloud_ocr_api_key {
        None
    } else {
        update
            .cloud_ocr_api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .or_else(|| previous_cloud_ocr_api_key.clone())
    };

    let result = (|| -> Result<(), AppError> {
        replace_shortcuts(&app, &candidate.global_shortcut, &candidate.ocr_shortcut)?;
        set_auto_start(&app, update.auto_start_enabled)?;
        replace_api_key(state.secrets.as_ref(), desired_api_key.as_deref())?;
        replace_cloud_ocr_api_key(state.secrets.as_ref(), desired_cloud_ocr_api_key.as_deref())?;
        state.settings.replace(candidate)?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = state.settings.replace(previous.clone());
        let _ = replace_api_key(state.secrets.as_ref(), previous_api_key.as_deref());
        let _ = replace_cloud_ocr_api_key(
            state.secrets.as_ref(),
            previous_cloud_ocr_api_key.as_deref(),
        );
        let _ = set_auto_start(&app, previous_auto_start);
        let _ = replace_shortcuts(&app, &previous.global_shortcut, &previous.ocr_shortcut);
        state.record_error(&error);
        return Err(error);
    }

    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
        crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
    )
}

fn replace_shortcuts(app: &AppHandle, translation: &str, ocr: &str) -> Result<(), AppError> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::InvalidShortcut(error.to_string()))?;
    app.global_shortcut()
        .register(translation)
        .map_err(|error| AppError::InvalidShortcut(error.to_string()))?;
    app.global_shortcut()
        .register(ocr)
        .map_err(|error| AppError::InvalidShortcut(error.to_string()))
}

fn replace_api_key(
    secrets: &dyn crate::security::SecretStore,
    api_key: Option<&str>,
) -> Result<(), AppError> {
    match api_key {
        Some(api_key) => secrets.save_api_key(api_key),
        None => secrets.delete_api_key(),
    }
}

fn replace_cloud_ocr_api_key(
    secrets: &dyn crate::security::SecretStore,
    api_key: Option<&str>,
) -> Result<(), AppError> {
    match api_key {
        Some(api_key) => secrets.save_cloud_ocr_api_key(api_key),
        None => secrets.delete_cloud_ocr_api_key(),
    }
}

fn set_auto_start(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| AppError::AutoStart(error.to_string()))
}

#[tauri::command]
pub async fn test_provider(update: UpdateSettings, app: AppHandle) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let settings = SettingsStore::validate(&update)?;
    let api_key = update
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
        .map(Ok)
        .unwrap_or_else(|| provider_api_key(&settings, state.secrets.as_ref()))?;
    state.translation.test_connection(settings, api_key).await?;
    Ok("Connection successful".into())
}

#[tauri::command]
pub async fn get_diagnostics(app: AppHandle) -> Result<DiagnosticsView, AppError> {
    let state = app.state::<AppState>();
    let AppSettings {
        provider,
        base_url,
        model,
        ocr_engine,
        ocr_language,
        cloud_ocr_model,
        ..
    } = state.settings.get()?;
    Ok(DiagnosticsView {
        app_version: env!("CARGO_PKG_VERSION"),
        provider,
        base_url,
        model,
        ocr_engine,
        ocr_language,
        api_key_configured: state.secrets.get_api_key()?.is_some(),
        cloud_ocr_model,
        cloud_ocr_api_key_configured: state.secrets.get_cloud_ocr_api_key()?.is_some(),
        paddle_ocr_installed: crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
        cache_entries: state.translation.cache_size().await?,
        settings_path: state.settings.path().display().to_string(),
        cache_path: state.cache_path.display().to_string(),
        last_error: state.last_error(),
    })
}

#[tauri::command]
pub fn export_settings_backup(app: AppHandle) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let backup = SettingsBackup {
        schema_version: CURRENT_SETTINGS_SCHEMA,
        provider: settings.provider,
        base_url: settings.base_url,
        model: settings.model,
        global_shortcut: settings.global_shortcut,
        ocr_shortcut: settings.ocr_shortcut,
        ocr_engine: settings.ocr_engine,
        ocr_language: settings.ocr_language,
        cloud_ocr_base_url: settings.cloud_ocr_base_url,
        cloud_ocr_model: settings.cloud_ocr_model,
        auto_start_enabled: app
            .autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
    };
    serde_json::to_string_pretty(&backup).map_err(|error| AppError::Settings(error.to_string()))
}

#[tauri::command]
pub fn import_settings_backup(contents: String) -> Result<SettingsBackup, AppError> {
    let backup: SettingsBackup = serde_json::from_str(&contents)
        .map_err(|error| AppError::Settings(format!("备份 JSON 无效：{error}")))?;
    if backup.schema_version > CURRENT_SETTINGS_SCHEMA {
        return Err(AppError::Settings(
            "该备份来自更高版本，当前版本无法导入".into(),
        ));
    }
    let normalized = SettingsStore::validate(&UpdateSettings {
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
        auto_start_enabled: backup.auto_start_enabled,
    })?;
    Ok(SettingsBackup {
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

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<crate::update::UpdateInfo, AppError> {
    crate::update::check_for_updates(&app.state::<AppState>().http_client).await
}
