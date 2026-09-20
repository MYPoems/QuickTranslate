use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    app::{AppState, DiagnosticError},
    config::{AppSettings, SettingsStore, SettingsView, UpdateSettings},
    errors::AppError,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsView {
    app_version: &'static str,
    provider: String,
    base_url: String,
    model: String,
    api_key_configured: bool,
    cache_entries: i64,
    settings_path: String,
    cache_path: String,
    last_error: Option<DiagnosticError>,
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
    )
}

#[tauri::command]
pub fn save_settings(update: UpdateSettings, app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    let candidate = SettingsStore::validate(&update)?;
    let previous = state.settings.get()?;
    let previous_api_key = state.secrets.get_api_key()?;
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

    let result = (|| -> Result<(), AppError> {
        replace_shortcut(&app, &candidate.global_shortcut)?;
        set_auto_start(&app, update.auto_start_enabled)?;
        replace_api_key(state.secrets.as_ref(), desired_api_key.as_deref())?;
        state.settings.replace(candidate)?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = state.settings.replace(previous.clone());
        let _ = replace_api_key(state.secrets.as_ref(), previous_api_key.as_deref());
        let _ = set_auto_start(&app, previous_auto_start);
        let _ = replace_shortcut(&app, &previous.global_shortcut);
        state.record_error(&error);
        return Err(error);
    }

    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
    )
}

fn replace_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), AppError> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::InvalidShortcut(error.to_string()))?;
    app.global_shortcut()
        .register(shortcut)
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
    let api_key = update
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
        .or(state.secrets.get_api_key()?)
        .ok_or(AppError::ProviderNotConfigured)?;
    let settings = SettingsStore::validate(&update)?;
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
        ..
    } = state.settings.get()?;
    Ok(DiagnosticsView {
        app_version: env!("CARGO_PKG_VERSION"),
        provider,
        base_url,
        model,
        api_key_configured: state.secrets.get_api_key()?.is_some(),
        cache_entries: state.translation.cache_size().await?,
        settings_path: state.settings.path().display().to_string(),
        cache_path: state.cache_path.display().to_string(),
        last_error: state.last_error(),
    })
}
