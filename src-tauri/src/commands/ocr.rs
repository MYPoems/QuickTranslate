use serde::Deserialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::{
    app::{show_ocr_error, trigger_ocr_translation},
    errors::AppError,
    platform,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRegion {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
pub async fn recognize_ocr_region(region: OcrRegion, app: AppHandle) -> Result<u64, AppError> {
    if !region.x.is_finite()
        || !region.y.is_finite()
        || !region.width.is_finite()
        || !region.height.is_finite()
        || region.width < 8.0
        || region.height < 8.0
    {
        return Ok(show_ocr_error(&app, AppError::Ocr("所选区域太小".into())));
    }

    let window = app
        .get_webview_window("ocr")
        .ok_or_else(|| AppError::Ocr("OCR 选区窗口不存在".into()))?;
    let position = window
        .outer_position()
        .map_err(|error| AppError::Ocr(error.to_string()))?;
    let scale = window
        .scale_factor()
        .map_err(|error| AppError::Ocr(error.to_string()))?;
    let _ = window.hide();
    tokio::time::sleep(Duration::from_millis(90)).await;

    let x = position.x.saturating_add((region.x * scale).round() as i32);
    let y = position.y.saturating_add((region.y * scale).round() as i32);
    let width = (region.width * scale).round() as i32;
    let height = (region.height * scale).round() as i32;
    let recognized =
        tokio::task::spawn_blocking(move || platform::recognize_screen_region(x, y, width, height))
            .await
            .map_err(|error| AppError::Ocr(error.to_string()))?;

    match recognized {
        Ok(text) => Ok(trigger_ocr_translation(app, text)),
        Err(error) => Ok(show_ocr_error(&app, error)),
    }
}

#[tauri::command]
pub fn hide_ocr_window(app: AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("ocr")
        .ok_or_else(|| AppError::Ocr("OCR 选区窗口不存在".into()))?;
    window
        .hide()
        .map_err(|error| AppError::Ocr(error.to_string()))
}
