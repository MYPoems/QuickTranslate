use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    app::AppState,
    config::{SettingsView, UpdateSettings},
    errors::AppError,
};

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
    let previous = state.settings.get()?;
    let previous_auto_start = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| AppError::AutoStart(error.to_string()))?;

    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::InvalidShortcut(error.to_string()))?;
    if let Err(error) = app
        .global_shortcut()
        .register(update.global_shortcut.as_str())
    {
        let _ = app
            .global_shortcut()
            .register(previous.global_shortcut.as_str());
        return Err(AppError::InvalidShortcut(error.to_string()));
    }

    let auto_start_changed = update.auto_start_enabled != previous_auto_start;
    if auto_start_changed {
        if let Err(error) = set_auto_start(&app, update.auto_start_enabled) {
            let _ = app.global_shortcut().unregister_all();
            let _ = app
                .global_shortcut()
                .register(previous.global_shortcut.as_str());
            return Err(error);
        }
    }

    if let Err(error) = state.settings.update(&update) {
        if auto_start_changed {
            let _ = set_auto_start(&app, previous_auto_start);
        }
        let _ = app.global_shortcut().unregister_all();
        let _ = app
            .global_shortcut()
            .register(previous.global_shortcut.as_str());
        return Err(error);
    }
    if update.clear_api_key {
        state.secrets.delete_api_key()?;
    } else if let Some(key) = update
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        state.secrets.save_api_key(key.trim())?;
    }
    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
    )
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
    let settings = crate::config::AppSettings {
        provider: update.provider,
        base_url: update.base_url.trim().trim_end_matches('/').into(),
        model: update.model.trim().into(),
        global_shortcut: update.global_shortcut,
    };
    state.translation.test_connection(settings, api_key).await?;
    Ok("Connection successful".into())
}
