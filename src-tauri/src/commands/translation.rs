use tauri::{AppHandle, Manager};

use crate::{
    app::{trigger_selected_translation, AppState},
    errors::AppError,
    platform,
    security::provider_api_key,
    storage::HistoryEntry,
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
    let api_key = provider_api_key(&settings, state.secrets.as_ref())?;
    state.translation.translate(text, settings, api_key).await
}

#[tauri::command]
pub async fn retranslate_text(text: String, app: AppHandle) -> Result<TranslationResult, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let api_key = provider_api_key(&settings, state.secrets.as_ref())?;
    state
        .translation
        .translate_fresh(text, settings, api_key)
        .await
}

#[tauri::command]
pub async fn copy_translation(text: String) -> Result<(), AppError> {
    platform::copy_text(text).await
}

#[tauri::command]
pub async fn clear_translation_cache(app: AppHandle) -> Result<usize, AppError> {
    app.state::<AppState>().translation.clear_cache().await
}

#[tauri::command]
pub async fn list_translation_history(
    query: String,
    favorite_only: bool,
    app: AppHandle,
) -> Result<Vec<HistoryEntry>, AppError> {
    app.state::<AppState>()
        .translation
        .history(query, favorite_only, 200)
        .await
}

#[tauri::command]
pub async fn set_history_favorite(id: i64, favorite: bool, app: AppHandle) -> Result<(), AppError> {
    app.state::<AppState>()
        .translation
        .set_favorite(id, favorite)
        .await
}

#[tauri::command]
pub async fn delete_history_entry(id: i64, app: AppHandle) -> Result<(), AppError> {
    app.state::<AppState>()
        .translation
        .delete_history_entry(id)
        .await
}

#[tauri::command]
pub fn get_popup_pinned(app: AppHandle) -> bool {
    app.state::<AppState>().popup_pinned()
}

#[tauri::command]
pub fn set_popup_pinned(pinned: bool, app: AppHandle) -> bool {
    app.state::<AppState>().set_popup_pinned(pinned);
    pinned
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
