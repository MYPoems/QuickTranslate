//! Isolated native UI smoke mode. Compiled out of release builds.
use crate::{errors::AppError, security::SecretStore};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Listener, Manager};

pub fn directory() -> Option<PathBuf> {
    std::env::var_os("QUICKTRANSLATE_QA_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[derive(Default)]
pub struct Secrets(Mutex<(Option<String>, Option<String>, Option<String>)>);
impl SecretStore for Secrets {
    fn save_cloud_speech_api_key(&self, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().2 = Some(value.into());
        Ok(())
    }
    fn get_cloud_speech_api_key(&self) -> Result<Option<String>, AppError> {
        Ok(self.0.lock().unwrap().2.clone())
    }
    fn delete_cloud_speech_api_key(&self) -> Result<(), AppError> {
        self.0.lock().unwrap().2 = None;
        Ok(())
    }
    fn save_api_key(&self, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().0 = Some(value.into());
        Ok(())
    }
    fn get_api_key(&self) -> Result<Option<String>, AppError> {
        Ok(self.0.lock().unwrap().0.clone())
    }
    fn delete_api_key(&self) -> Result<(), AppError> {
        self.0.lock().unwrap().0 = None;
        Ok(())
    }
    fn save_cloud_ocr_api_key(&self, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().1 = Some(value.into());
        Ok(())
    }
    fn get_cloud_ocr_api_key(&self) -> Result<Option<String>, AppError> {
        Ok(self.0.lock().unwrap().1.clone())
    }
    fn delete_cloud_ocr_api_key(&self) -> Result<(), AppError> {
        self.0.lock().unwrap().1 = None;
        Ok(())
    }
}

pub fn setup(app: &tauri::AppHandle) {
    if directory().is_none() {
        return;
    }
    app.state::<crate::app::AppState>().set_popup_pinned(true);
    let handle = app.clone();
    app.once("popup-ready", move |_| {
        crate::window::show_popup(&handle);
        if let Some(window) = handle.get_webview_window("popup") { let _ = window.set_title("QuickTranslate QA — isolated data"); }
        let _ = handle.emit_to("popup", "translation-state", serde_json::json!({
            "requestId": 1, "status": "success", "sourceKind": "ocr",
            "result": { "sourceText": "这是中文原文的第一段，用于验证离线朗读与高亮。\n这是第二段，验证自动切换、暂停和停止。", "translation": "This is the first paragraph for offline speech and highlighting.\nThis is the second paragraph for pause, stop, and sequential reading.", "detectedLanguage": "chinese", "targetLanguage": "english", "provider": "QA fixture", "model": "offline", "cached": true }
        }));
        crate::window::show_settings(&handle);
        if let Some(window) = handle.get_webview_window("settings") { let _ = window.set_title("QuickTranslate QA 设置 — isolated data"); }
    });
}
