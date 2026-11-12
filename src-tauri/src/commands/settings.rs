use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    app::{AppState, DiagnosticError},
    config::{
        AppSettings, OcrEngineKind, OcrLanguage, SettingsBackup, SettingsStore, SettingsView,
        UpdateSettings, CURRENT_SETTINGS_SCHEMA,
    },
    errors::AppError,
    security::{delete_key, get_key, restore_key, save_key, SecretKind, SecretStore},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsView {
    app_version: &'static str,
    provider: String,
    base_url: String,
    model: String,
    ocr_engine: OcrEngineKind,
    ocr_language: OcrLanguage,
    api_key_configured: bool,
    cloud_ocr_model: String,
    cloud_ocr_api_key_configured: bool,
    paddle_ocr_installed: bool,
    cache_entries: i64,
    settings_path: String,
    cache_path: String,
    last_error: Option<DiagnosticError>,
    last_capture: Option<crate::platform::CaptureDiagnostics>,
}

/// The two global shortcuts, for windows that mention them in their own text.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutsView {
    global_shortcut: String,
    ocr_shortcut: String,
}

impl From<&AppSettings> for ShortcutsView {
    fn from(settings: &AppSettings) -> Self {
        Self {
            global_shortcut: settings.global_shortcut.clone(),
            ocr_shortcut: settings.ocr_shortcut.clone(),
        }
    }
}

#[tauri::command]
pub fn get_shortcuts(app: AppHandle) -> Result<ShortcutsView, AppError> {
    Ok(ShortcutsView::from(
        &app.state::<AppState>().settings.get()?,
    ))
}

#[tauri::command]
pub fn get_appearance_preferences(
    app: AppHandle,
) -> Result<crate::config::AppearancePreferences, AppError> {
    Ok(app.state::<AppState>().settings.get()?.appearance)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    state.settings.view(
        state.secrets.as_ref(),
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
        crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
    )
}

/// What the settings form asked for one kind of key.
pub(crate) struct KeyChange<'a> {
    kind: SecretKind,
    entered: Option<&'a str>,
    clear: bool,
}

impl KeyChange<'_> {
    /// Applies the request to the key stored for `endpoint`: a typed key is saved, the
    /// delete checkbox removes it, and an empty field leaves it alone. Keys saved for other
    /// addresses are never touched, so switching back to one of them finds its key again.
    fn apply(&self, secrets: &dyn SecretStore, endpoint: &str) -> Result<(), AppError> {
        if self.clear {
            return delete_key(secrets, self.kind, endpoint);
        }
        match self.typed() {
            Some(key) => save_key(secrets, self.kind, endpoint, key),
            None => Ok(()),
        }
    }

    fn typed(&self) -> Option<&str> {
        self.entered.map(str::trim).filter(|key| !key.is_empty())
    }

    /// The key a request to `endpoint` would use if this form were saved.
    pub(crate) fn effective(
        &self,
        secrets: &dyn SecretStore,
        endpoint: &str,
    ) -> Result<Option<String>, AppError> {
        if self.clear {
            return Ok(None);
        }
        match self.typed() {
            Some(key) => Ok(Some(key.to_string())),
            None => get_key(secrets, self.kind, endpoint),
        }
    }
}

pub(crate) fn key_changes(update: &UpdateSettings) -> [KeyChange<'_>; 3] {
    [
        KeyChange {
            kind: SecretKind::Translation,
            entered: update.api_key.as_deref(),
            clear: update.clear_api_key,
        },
        KeyChange {
            kind: SecretKind::CloudOcr,
            entered: update.cloud_ocr_api_key.as_deref(),
            clear: update.clear_cloud_ocr_api_key,
        },
        KeyChange {
            kind: SecretKind::CloudSpeech,
            entered: update.cloud_speech_api_key.as_deref(),
            clear: update.clear_cloud_speech_api_key,
        },
    ]
}

#[tauri::command]
pub fn save_settings(update: UpdateSettings, app: AppHandle) -> Result<SettingsView, AppError> {
    let state = app.state::<AppState>();
    let secrets = state.secrets.as_ref();
    let candidate = SettingsStore::validate(&update)?;
    let changes = key_changes(&update);
    let endpoints: Vec<String> = changes
        .iter()
        .map(|change| change.kind.endpoint(&candidate).to_string())
        .collect();
    #[cfg(debug_assertions)]
    if crate::qa::directory().is_some() {
        for (change, endpoint) in changes.iter().zip(&endpoints) {
            change.apply(secrets, endpoint)?;
        }
        let appearance = candidate.appearance.clone();
        state.settings.replace(candidate)?;
        let _ = app.emit("appearance-changed", &appearance);
        return state.settings.view(
            secrets,
            false,
            crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
        );
    }
    let previous = state.settings.get()?;
    // What the affected addresses hold now, to put back if a later step fails.
    let previous_keys = changes
        .iter()
        .zip(&endpoints)
        .map(|(change, endpoint)| get_key(secrets, change.kind, endpoint))
        .collect::<Result<Vec<_>, _>>()?;
    let previous_auto_start = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| AppError::AutoStart(error.to_string()))?;

    let result = (|| -> Result<(), AppError> {
        replace_shortcuts(&app, &candidate.global_shortcut, &candidate.ocr_shortcut)?;
        set_auto_start(&app, update.auto_start_enabled)?;
        for (change, endpoint) in changes.iter().zip(&endpoints) {
            change.apply(secrets, endpoint)?;
        }
        state.settings.replace(candidate)?;
        state.cloud_speech.stop();
        let _ = app.emit("speech-stop", "settings");
        state.cloud_speech.clear_cache();
        state.speech_plugin.stop_synthesis();
        Ok(())
    })();

    if let Err(error) = result {
        let _ = state.settings.replace(previous.clone());
        for ((change, endpoint), key) in changes.iter().zip(&endpoints).zip(&previous_keys) {
            let _ = restore_key(secrets, change.kind, endpoint, key.as_deref());
        }
        let _ = set_auto_start(&app, previous_auto_start);
        let _ = replace_shortcuts(&app, &previous.global_shortcut, &previous.ocr_shortcut);
        state.record_error(&error);
        return Err(error);
    }

    let saved = state.settings.get()?;
    let _ = app.emit("shortcuts-changed", ShortcutsView::from(&saved));
    let _ = app.emit("appearance-changed", saved.appearance);
    state.settings.view(
        secrets,
        app.autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
        crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
    )
}

/// Whether a key is saved for each address currently in the form, which may differ from
/// the saved settings while the user is editing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    api_key_configured: bool,
    cloud_ocr_api_key_configured: bool,
    cloud_speech_api_key_configured: bool,
}

#[tauri::command]
pub fn get_key_status(
    base_url: String,
    cloud_ocr_base_url: String,
    cloud_speech_endpoint: String,
    app: AppHandle,
) -> Result<KeyStatus, AppError> {
    let state = app.state::<AppState>();
    let configured = |kind: SecretKind, endpoint: &str| {
        get_key(state.secrets.as_ref(), kind, endpoint).map(|key| key.is_some())
    };
    Ok(KeyStatus {
        api_key_configured: configured(SecretKind::Translation, &base_url)?,
        cloud_ocr_api_key_configured: configured(SecretKind::CloudOcr, &cloud_ocr_base_url)?,
        cloud_speech_api_key_configured: configured(
            SecretKind::CloudSpeech,
            &cloud_speech_endpoint,
        )?,
    })
}

fn replace_shortcuts(app: &AppHandle, translation: &str, ocr: &str) -> Result<(), AppError> {
    const UNAVAILABLE: &str = "快捷键格式无效，或已被其他程序占用";
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::Internal(error.to_string()))?;
    app.global_shortcut()
        .register(translation)
        .map_err(|_| AppError::invalid("globalShortcut", UNAVAILABLE))?;
    app.global_shortcut()
        .register(ocr)
        .map_err(|_| AppError::invalid("ocrShortcut", UNAVAILABLE))
}

/// Whether a shortcut typed into the form could be registered: `ok`, `invalid` (not a
/// shortcut) or `taken` (another program holds it). The two shortcuts this application has
/// registered itself count as available.
#[tauri::command]
pub fn check_shortcut(shortcut: String, app: AppHandle) -> &'static str {
    let Ok(parsed) = shortcut
        .trim()
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
    else {
        return "invalid";
    };
    #[cfg(debug_assertions)]
    if crate::qa::directory().is_some() {
        return "ok";
    }
    let shortcuts = app.global_shortcut();
    if shortcuts.is_registered(parsed) {
        return "ok";
    }
    match shortcuts.register(parsed) {
        Ok(()) => {
            let _ = shortcuts.unregister(parsed);
            "ok"
        }
        Err(_) => "taken",
    }
}

/// The first-run guide is offered while nothing is set up: no key for the configured
/// service, which needs one, and the guide neither finished nor skipped before.
pub fn needs_onboarding(app: &AppHandle) -> bool {
    #[cfg(debug_assertions)]
    if crate::qa::directory().is_some() {
        return false;
    }
    let state = app.state::<AppState>();
    let Some(marker) = onboarding_marker(&state) else {
        return false;
    };
    if marker.exists() {
        return false;
    }
    let Ok(settings) = state.settings.get() else {
        return false;
    };
    settings.requires_api_key()
        && matches!(
            get_key(
                state.secrets.as_ref(),
                SecretKind::Translation,
                &settings.base_url
            ),
            Ok(None)
        )
}

fn onboarding_marker(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .settings
        .path()
        .parent()
        .map(|directory| directory.join("onboarding-complete"))
}

#[tauri::command]
pub fn get_onboarding(app: AppHandle) -> bool {
    needs_onboarding(&app)
}

/// Records that the guide was finished or skipped, so it is not offered again.
#[tauri::command]
pub fn complete_onboarding(app: AppHandle) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let marker = onboarding_marker(&state)
        .ok_or_else(|| AppError::Settings("settings path has no parent".into()))?;
    std::fs::write(marker, b"The first-run guide was finished or skipped.\n")
        .map_err(|error| AppError::Settings(error.to_string()))
}

/// Opens the page where the default provider issues API keys. The address is fixed here;
/// nothing a window sends can change where the browser goes.
#[tauri::command]
pub fn open_api_key_page() -> Result<(), AppError> {
    const PAGE: &str = "https://bailian.console.aliyun.com/?tab=model#/api-key";
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(PAGE)
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Internal(error.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = PAGE;
        Err(AppError::UnsupportedPlatform)
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
    let settings = SettingsStore::validate(&update)?;
    // Tested with the key typed in the form, or else the key saved for the address being
    // tested. A key saved for another address is never used.
    let [translation, ..] = key_changes(&update);
    let api_key = match translation.effective(state.secrets.as_ref(), &settings.base_url)? {
        Some(api_key) => api_key,
        None if !settings.requires_api_key() => String::new(),
        None => return Err(AppError::ProviderNotConfigured),
    };
    state.translation.test_connection(settings, api_key).await?;
    Ok("Connection successful".into())
}

#[tauri::command]
pub async fn get_diagnostics(app: AppHandle) -> Result<DiagnosticsView, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let configured = |kind: SecretKind| {
        get_key(state.secrets.as_ref(), kind, kind.endpoint(&settings)).map(|key| key.is_some())
    };
    let api_key_configured = configured(SecretKind::Translation)?;
    let cloud_ocr_api_key_configured = configured(SecretKind::CloudOcr)?;
    let AppSettings {
        provider,
        base_url,
        model,
        ocr_engine,
        ocr_language,
        cloud_ocr_model,
        ..
    } = settings;
    Ok(DiagnosticsView {
        app_version: env!("CARGO_PKG_VERSION"),
        provider,
        base_url,
        model,
        ocr_engine,
        ocr_language,
        api_key_configured,
        cloud_ocr_model,
        cloud_ocr_api_key_configured,
        paddle_ocr_installed: crate::ocr::plugin::status(&state.ocr_plugin_dir).installed,
        cache_entries: state.translation.cache_size().await?,
        settings_path: state.settings.path().display().to_string(),
        cache_path: state.cache_path.display().to_string(),
        last_error: state.last_error(),
        last_capture: crate::platform::last_capture_diagnostics(),
    })
}

#[tauri::command]
pub fn export_settings_backup(app: AppHandle) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?;
    let backup = SettingsBackup {
        appearance: settings.appearance,
        speech: settings.speech,
        schema_version: CURRENT_SETTINGS_SCHEMA,
        provider: settings.provider,
        base_url: settings.base_url,
        model: settings.model,
        global_shortcut: settings.global_shortcut,
        ocr_shortcut: settings.ocr_shortcut,
        ocr_engine: settings.ocr_engine,
        ocr_language: settings.ocr_language,
        cloud_ocr_base_url: settings.cloud_ocr_base_url,
        cloud_ocr_model: settings.cloud_ocr_model,
        auto_start_enabled: app
            .autolaunch()
            .is_enabled()
            .map_err(|error| AppError::AutoStart(error.to_string()))?,
    };
    serde_json::to_string_pretty(&backup).map_err(|error| AppError::Settings(error.to_string()))
}

#[tauri::command]
pub fn import_settings_backup(contents: String) -> Result<SettingsBackup, AppError> {
    crate::config::parse_settings_backup(&contents)
}
