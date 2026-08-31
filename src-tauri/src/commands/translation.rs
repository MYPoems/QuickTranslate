use tauri::{AppHandle, Manager};

use crate::{
    app::{trigger_selected_translation, AppState},
    errors::AppError,
    platform,
    translation::types::TranslationResult,
};

#[tauri::command]
pub fn translate_selected_text(app: AppHandle) -> u64 {
    trigger_selected_translation(app)
}

#[tauri::command]
pub async fn translate_text(text: String, app: AppHandle) -> Result<TranslationResult, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let api_key = state
        .secrets
        .get_api_key()?
        .filter(|value| !value.trim().is_empty())
        .ok_or(AppError::ProviderNotConfigured)?;
    state.translation.translate(text, settings, api_key).await
}

#[tauri::command]
pub async fn copy_translation(text: String) -> Result<(), AppError> {
    platform::copy_text(text).await
}

#[tauri::command]
pub fn hide_translation_window(app: AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("popup")
        .ok_or_else(|| AppError::Internal("popup window is missing".into()))?;
    window
        .hide()
        .map_err(|error| AppError::Internal(error.to_string()))
}
