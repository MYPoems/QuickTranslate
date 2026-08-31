use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    config::SettingsStore,
    errors::AppError,
    platform,
    security::{KeyringSecretStore, SecretStore},
    storage::TranslationCache,
    translation::{service::TranslationService, types::TranslationResult},
    window,
};

pub struct AppState {
    pub settings: Arc<SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub translation: Arc<TranslationService>,
    latest_request: AtomicU64,
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
        let cache = Arc::new(TranslationCache::open(
            &data_dir.join("translations.sqlite3"),
        )?);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent("QuickTranslate/0.1")
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        Ok(Self {
            settings,
            secrets: Arc::new(KeyringSecretStore),
            translation: Arc::new(TranslationService::new(client, cache)),
            latest_request: AtomicU64::new(0),
        })
    }

    fn next_request(&self) -> u64 {
        self.latest_request.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn is_latest(&self, request_id: u64) -> bool {
        self.latest_request.load(Ordering::Relaxed) == request_id
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
    result: Option<TranslationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<crate::errors::AppError>,
}

pub fn trigger_selected_translation(app: AppHandle) -> u64 {
    let request_id = app.state::<AppState>().next_request();
    tauri::async_runtime::spawn(async move {
        let selected = match platform::get_selected_text().await {
            Ok(text) => text,
            Err(error) => {
                show_error(&app, request_id, error, true);
                return;
            }
        };

        window::show_popup(&app);
        let _ = app.emit_to(
            "popup",
            "translation-state",
            PopupPayload {
                request_id,
                status: "loading",
                source_text: Some(selected.trim().to_string()),
                result: None,
                error: None,
            },
        );

        let state = app.state::<AppState>();
        let outcome = match (state.settings.get(), state.secrets.get_api_key()) {
            (Ok(settings), Ok(Some(api_key))) if !api_key.trim().is_empty() => {
                state
                    .translation
                    .translate(selected, settings, api_key)
                    .await
            }
            (Ok(_), Ok(_)) => Err(AppError::ProviderNotConfigured),
            (Err(error), _) | (_, Err(error)) => Err(error),
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
                        result: Some(result),
                        error: None,
                    },
                );
            }
            Err(error) => show_error(&app, request_id, error, false),
        }
    });
    request_id
}

fn show_error(app: &AppHandle, request_id: u64, error: AppError, ensure_visible: bool) {
    if !app.state::<AppState>().is_latest(request_id) {
        return;
    }
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
            result: None,
            error: Some(error),
        },
    );
}
