use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    app::{begin_ocr_recognition, finish_ocr_error, finish_ocr_translation, AppState},
    config::OcrEngineKind,
    errors::AppError,
    ocr::{cloud, plugin},
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

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OcrRegionResult {
    Completed {
        request_id: u64,
    },
    Paddle {
        request_id: u64,
        image_data_url: String,
        detection_model_path: String,
        recognition_model_path: String,
    },
}

#[tauri::command]
pub async fn recognize_ocr_region(
    region: OcrRegion,
    app: AppHandle,
) -> Result<OcrRegionResult, AppError> {
    if !region.x.is_finite()
        || !region.y.is_finite()
        || !region.width.is_finite()
        || !region.height.is_finite()
        || region.width < 8.0
        || region.height < 8.0
    {
        let request_id = begin_ocr_recognition(&app);
        return Ok(OcrRegionResult::Completed {
            request_id: finish_ocr_error(&app, request_id, AppError::Ocr("所选区域太小".into())),
        });
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
    let settings = app.state::<AppState>().settings.get()?;
    let request_id = begin_ocr_recognition(&app);

    match settings.ocr_engine {
        OcrEngineKind::Windows => {
            let language = settings.ocr_language;
            let recognized = tokio::task::spawn_blocking(move || {
                platform::recognize_screen_region(x, y, width, height, language)
            })
            .await
            .map_err(|error| AppError::Ocr(error.to_string()))?;
            Ok(OcrRegionResult::Completed {
                request_id: finish_result(&app, request_id, recognized),
            })
        }
        OcrEngineKind::Paddle => {
            let paths = match plugin::verified_paths(&app.state::<AppState>().ocr_plugin_dir) {
                Ok(paths) => paths,
                Err(error) => {
                    return Ok(OcrRegionResult::Completed {
                        request_id: finish_ocr_error(&app, request_id, error),
                    });
                }
            };
            let screenshot = tokio::task::spawn_blocking(move || {
                platform::capture_screen_region_png(x, y, width, height)
            })
            .await
            .map_err(|error| AppError::Ocr(error.to_string()))?;
            let screenshot = match screenshot {
                Ok(value) => value,
                Err(error) => {
                    return Ok(OcrRegionResult::Completed {
                        request_id: finish_ocr_error(&app, request_id, error),
                    });
                }
            };
            Ok(OcrRegionResult::Paddle {
                request_id,
                image_data_url: format!("data:image/png;base64,{}", STANDARD.encode(screenshot)),
                detection_model_path: paths.detection.display().to_string(),
                recognition_model_path: paths.recognition.display().to_string(),
            })
        }
        OcrEngineKind::Cloud => {
            let api_key = match app
                .state::<AppState>()
                .secrets
                .get_cloud_ocr_api_key()?
                .filter(|value| !value.trim().is_empty())
            {
                Some(value) => value,
                None => {
                    return Ok(OcrRegionResult::Completed {
                        request_id: finish_ocr_error(
                            &app,
                            request_id,
                            AppError::Ocr("请先配置云端视觉 OCR API Key".into()),
                        ),
                    });
                }
            };
            let screenshot = tokio::task::spawn_blocking(move || {
                platform::capture_screen_region_png(x, y, width, height)
            })
            .await
            .map_err(|error| AppError::Ocr(error.to_string()))?;
            let recognized = match screenshot {
                Ok(png) => {
                    cloud::recognize(
                        &app.state::<AppState>().http_client,
                        &settings,
                        &api_key,
                        &png,
                    )
                    .await
                }
                Err(error) => Err(error),
            };
            Ok(OcrRegionResult::Completed {
                request_id: finish_result(&app, request_id, recognized),
            })
        }
    }
}

#[tauri::command]
pub fn complete_paddle_ocr(request_id: u64, text: String, app: AppHandle) -> Result<u64, AppError> {
    if text.trim().is_empty() {
        return Ok(finish_ocr_error(&app, request_id, AppError::OcrNoText));
    }
    Ok(finish_ocr_translation(
        app,
        request_id,
        text.trim().to_string(),
    ))
}

#[tauri::command]
pub fn fail_paddle_ocr(request_id: u64, message: String, app: AppHandle) -> Result<u64, AppError> {
    let mut message = message.replace(['\r', '\n'], " ");
    message.truncate(240);
    Ok(finish_ocr_error(
        &app,
        request_id,
        AppError::Ocr(format!("PP-OCRv6 Small 运行失败：{message}")),
    ))
}

#[tauri::command]
pub fn get_paddle_ocr_plugin_status(
    app: AppHandle,
) -> Result<plugin::PaddleOcrPluginStatus, AppError> {
    Ok(plugin::status(&app.state::<AppState>().ocr_plugin_dir))
}

#[tauri::command]
pub async fn install_paddle_ocr_plugin(
    app: AppHandle,
) -> Result<plugin::PaddleOcrPluginStatus, AppError> {
    plugin::install(&app.state::<AppState>().ocr_plugin_dir).await
}

#[tauri::command]
pub fn uninstall_paddle_ocr_plugin(
    app: AppHandle,
) -> Result<plugin::PaddleOcrPluginStatus, AppError> {
    plugin::uninstall(&app.state::<AppState>().ocr_plugin_dir)
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

fn finish_result(app: &AppHandle, request_id: u64, result: Result<String, AppError>) -> u64 {
    match result {
        Ok(text) => finish_ocr_translation(app.clone(), request_id, text),
        Err(error) => finish_ocr_error(app, request_id, error),
    }
}
