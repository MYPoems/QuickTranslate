use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::{
    config::SettingsStore,
    errors::AppError,
    platform,
    security::{provider_api_key, KeyringSecretStore, SecretStore},
    storage::TranslationCache,
    translation::{service::TranslationService, types::TranslationResult},
    window,
};

pub struct AppState {
    pub settings: Arc<SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub translation: Arc<TranslationService>,
    pub cache_path: PathBuf,
    latest_request: AtomicU64,
    popup_pinned: AtomicBool,
    active_request: Mutex<CancellationToken>,
    last_error: RwLock<Option<DiagnosticError>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticError {
    pub code: String,
    pub message: String,
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> Result<Self, AppError> {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|error| AppError::Settings(error.to_string()))?;
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::Database(error.to_string()))?;
        let settings = Arc::new(SettingsStore::load(config_dir.join("settings.json"))?);
        let cache_path = data_dir.join("translations.sqlite3");
        let cache = Arc::new(TranslationCache::open(&cache_path)?);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("QuickTranslate/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        Ok(Self {
            settings,
            secrets: Arc::new(KeyringSecretStore),
            translation: Arc::new(TranslationService::new(client, cache)),
            cache_path,
            latest_request: AtomicU64::new(0),
            popup_pinned: AtomicBool::new(false),
            active_request: Mutex::new(CancellationToken::new()),
            last_error: RwLock::new(None),
        })
    }

    fn begin_request(&self) -> (u64, CancellationToken) {
        let request_id = self.latest_request.fetch_add(1, Ordering::Relaxed) + 1;
        let next = CancellationToken::new();
        if let Ok(mut active) = self.active_request.lock() {
            active.cancel();
            *active = next.clone();
        }
        (request_id, next)
    }

    fn is_latest(&self, request_id: u64) -> bool {
        self.latest_request.load(Ordering::Relaxed) == request_id
    }

    pub fn record_error(&self, error: &AppError) {
        if let Ok(mut last_error) = self.last_error.write() {
            *last_error = Some(DiagnosticError {
                code: error.code().to_string(),
                message: error.user_message().to_string(),
            });
        }
    }

    pub fn last_error(&self) -> Option<DiagnosticError> {
        self.last_error.read().ok().and_then(|error| error.clone())
    }

    pub fn popup_pinned(&self) -> bool {
        self.popup_pinned.load(Ordering::Relaxed)
    }

    pub fn set_popup_pinned(&self, pinned: bool) {
        self.popup_pinned.store(pinned, Ordering::Relaxed);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PopupPayload {
    request_id: u64,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<TranslationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<crate::errors::AppError>,
}

pub fn trigger_selected_translation(app: AppHandle) -> u64 {
    let (request_id, cancellation) = app.state::<AppState>().begin_request();
    tauri::async_runtime::spawn(async move {
        let Some(selected) = cancellation
            .run_until_cancelled(platform::get_selected_text())
            .await
        else {
            return;
        };
        let selected = match selected {
            Ok(text) => text,
            Err(error) => {
                show_error(&app, request_id, error, true);
                return;
            }
        };

        translate_request(app, request_id, cancellation, selected, "selection").await;
    });
    request_id
}

pub fn trigger_ocr_translation(app: AppHandle, recognized_text: String) -> u64 {
    let (request_id, cancellation) = app.state::<AppState>().begin_request();
    tauri::async_runtime::spawn(translate_request(
        app,
        request_id,
        cancellation,
        recognized_text,
        "ocr",
    ));
    request_id
}

pub fn show_ocr_error(app: &AppHandle, error: AppError) -> u64 {
    let (request_id, _) = app.state::<AppState>().begin_request();
    show_error(app, request_id, error, true);
    request_id
}

async fn translate_request(
    app: AppHandle,
    request_id: u64,
    cancellation: CancellationToken,
    source_text: String,
    source_kind: &'static str,
) {
    if !app.state::<AppState>().is_latest(request_id) {
        return;
    }
    if source_kind == "ocr" {
        window::prepare_ocr_popup(&app);
    }
    window::show_popup(&app);
    let source_text = source_text.trim().to_string();
    let _ = app.emit_to(
        "popup",
        "translation-state",
        PopupPayload {
            request_id,
            status: "loading",
            source_text: Some(source_text.clone()),
            source_kind: Some(source_kind),
            result: None,
            error: None,
        },
    );

    let state = app.state::<AppState>();
    let settings = match state.settings.get() {
        Ok(settings) => settings,
        Err(error) => {
            show_error(&app, request_id, error, false);
            return;
        }
    };
    let api_key = match provider_api_key(&settings, state.secrets.as_ref()) {
        Ok(api_key) => api_key,
        Err(error) => {
            show_error(&app, request_id, error, false);
            return;
        }
    };
    let Some(outcome) = cancellation
        .run_until_cancelled(state.translation.translate(source_text, settings, api_key))
        .await
    else {
        return;
    };
    if !state.is_latest(request_id) {
        return;
    }
    match outcome {
        Ok(result) => {
            let _ = app.emit_to(
                "popup",
                "translation-state",
                PopupPayload {
                    request_id,
                    status: "success",
                    source_text: None,
                    source_kind: Some(source_kind),
                    result: Some(result),
                    error: None,
                },
            );
        }
        Err(error) => show_error(&app, request_id, error, false),
    }
}

fn show_error(app: &AppHandle, request_id: u64, error: AppError, ensure_visible: bool) {
    let state = app.state::<AppState>();
    if !state.is_latest(request_id) {
        return;
    }
    state.record_error(&error);
    if ensure_visible {
        window::show_popup(app);
    }
    let _ = app.emit_to(
        "popup",
        "translation-state",
        PopupPayload {
            request_id,
            status: "error",
            source_text: None,
            source_kind: None,
            result: None,
            error: Some(error),
        },
    );
}
