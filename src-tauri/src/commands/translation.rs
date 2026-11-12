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

/// Translates again, bypassing the cache. When the popup passes the request it is showing
/// (`request_id`) and which of its own attempts this is (`generation`), the text is also
/// streamed to it as it arrives. `target` asks for a language other than the usual one,
/// for this text only.
#[tauri::command]
pub async fn retranslate_text(
    text: String,
    request_id: Option<u64>,
    generation: Option<u64>,
    target: Option<crate::translation::types::Language>,
    app: AppHandle,
) -> Result<TranslationResult, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let api_key = provider_api_key(&settings, state.secrets.as_ref())?;
    match request_id {
        Some(request_id) => {
            state
                .translation
                .translate_streaming(
                    text,
                    settings,
                    api_key,
                    false,
                    target,
                    crate::app::delta_emitter(&app, request_id, generation),
                )
                .await
        }
        None => {
            state
                .translation
                .translate_fresh(text, settings, api_key)
                .await
        }
    }
}

/// Translates text typed or pasted into the popup; the result arrives as popup events.
#[tauri::command]
pub fn translate_input(text: String, app: AppHandle) -> u64 {
    crate::app::trigger_text_translation(app, text)
}

/// Stops the request on screen, whether it is recognising a screenshot or translating.
/// Returns the id the popup should treat as current from now on.
#[tauri::command]
pub fn cancel_request(app: AppHandle) -> u64 {
    app.state::<AppState>().cancel_active()
}

/// Opens the settings window, for the popup's "configure" prompts.
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    crate::window::show_settings(&app);
}

#[tauri::command]
pub async fn copy_translation(text: String, shortcut: Option<bool>) -> Result<bool, AppError> {
    platform::copy_text(text, shortcut.unwrap_or(false)).await
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
pub async fn open_history_translation(id: i64, app: AppHandle) -> Result<(), AppError> {
    let result = app
        .state::<AppState>()
        .translation
        .history_result(id)
        .await?;
    crate::app::show_history_result(&app, result)
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
