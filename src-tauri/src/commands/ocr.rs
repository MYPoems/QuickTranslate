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
    let request_id = begin_ocr_recognition(&app);
    // The popup now shows "recognising…". Every failure from here on has to reach it as an
    // error state; returning the error to the (already hidden) overlay would leave it spinning.
    match recognize_region(&app, request_id, x, y, width, height).await {
        Ok(result) => Ok(result),
        Err(error) => Ok(OcrRegionResult::Completed {
            request_id: finish_ocr_error(&app, request_id, error),
        }),
    }
}

async fn recognize_region(
    app: &AppHandle,
    request_id: u64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<OcrRegionResult, AppError> {
    let settings = app.state::<AppState>().settings.get()?;
    match settings.ocr_engine {
        OcrEngineKind::Windows => {
            let language = settings.ocr_language;
            let recognized = tokio::task::spawn_blocking(move || {
                platform::recognize_screen_region(x, y, width, height, language)
            })
            .await
            .map_err(|error| AppError::Ocr(error.to_string()))?;
            Ok(OcrRegionResult::Completed {
                request_id: finish_result(app, request_id, recognized),
            })
        }
        OcrEngineKind::Paddle => {
            let paths = match plugin::verified_paths(&app.state::<AppState>().ocr_plugin_dir) {
                Ok(paths) => paths,
                Err(error) => {
                    return Ok(OcrRegionResult::Completed {
                        request_id: finish_ocr_error(app, request_id, error),
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
                        request_id: finish_ocr_error(app, request_id, error),
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
            let api_key = match crate::security::get_key(
                app.state::<AppState>().secrets.as_ref(),
                crate::security::SecretKind::CloudOcr,
                &settings.cloud_ocr_base_url,
            )? {
                Some(value) => value,
                None => {
                    return Ok(OcrRegionResult::Completed {
                        request_id: finish_ocr_error(
                            app,
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
                request_id: finish_result(app, request_id, recognized),
            })
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudOcrTest {
    elapsed_ms: u64,
    /// Whether the reply contained the text of the built-in test picture.
    recognized: bool,
}

/// Checks the cloud OCR settings in the form, saved or not, against the built-in test
/// picture. Nothing from the screen is uploaded.
#[tauri::command]
pub async fn test_cloud_ocr(
    update: crate::config::UpdateSettings,
    app: AppHandle,
) -> Result<CloudOcrTest, AppError> {
    let settings = crate::config::SettingsStore::validate(&update)?;
    if settings.cloud_ocr_base_url.is_empty() {
        return Err(AppError::invalid(
            "cloudOcrBaseUrl",
            "请先填写云端 OCR 服务地址",
        ));
    }
    if settings.cloud_ocr_model.is_empty() {
        return Err(AppError::invalid(
            "cloudOcrModel",
            "请先填写云端 OCR 模型名称",
        ));
    }
    let state = app.state::<AppState>();
    let [_, cloud_ocr, _] = crate::commands::settings::key_changes(&update);
    let api_key = cloud_ocr
        .effective(state.secrets.as_ref(), &settings.cloud_ocr_base_url)?
        .ok_or_else(|| {
            AppError::invalid(
                "cloudOcrApiKey",
                "请填写这个服务地址的云端 OCR API Key 后再测试",
            )
        })?;
    let started = std::time::Instant::now();
    let recognized = match cloud::recognize(
        &state.http_client,
        &settings,
        &api_key,
        cloud::CONNECTION_TEST_IMAGE,
    )
    .await
    {
        Ok(text) => text.to_lowercase().contains(cloud::CONNECTION_TEST_WORD),
        Err(AppError::OcrNoText) => false,
        Err(error) => return Err(error),
    };
    Ok(CloudOcrTest {
        elapsed_ms: started.elapsed().as_millis() as u64,
        recognized,
    })
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
