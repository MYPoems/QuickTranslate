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
    let vocabulary = std::env::var_os("QUICKTRANSLATE_QA_VOCABULARY").is_some();
    if vocabulary {
        if let Err(error) = setup_vocabulary(app) {
            eprintln!("Isolated vocabulary QA: {error}");
        }
    }
    let handle = app.clone();
    app.once("popup-ready", move |_| {
        crate::window::show_popup(&handle);
        if let Some(window) = handle.get_webview_window("popup") { let _ = window.set_title("QuickTranslate QA — isolated data"); }
        let _ = handle.emit_to("popup", "translation-state", serde_json::json!({
            "requestId": 1, "status": "success", "sourceKind": "ocr",
            "result": { "sourceText": "这是中文原文的第一段，用于验证离线朗读与高亮。\n这是第二段，验证自动切换、暂停和停止。", "translation": "This is the first paragraph for offline speech and highlighting.\nThis is the second paragraph for pause, stop, and sequential reading.", "detectedLanguage": "chinese", "targetLanguage": "english", "provider": "QA fixture", "model": "offline", "cached": true }
        }));
        if vocabulary {
            crate::window::show_vocabulary(&handle);
            if let Some(window) = handle.get_webview_window("vocabulary") { let _ = window.set_title("QuickTranslate QA 生词本 — isolated data"); }
        } else {
            crate::window::show_settings(&handle);
            if let Some(window) = handle.get_webview_window("settings") { let _ = window.set_title("QuickTranslate QA 设置 — isolated data"); }
        }
    });
}

fn setup_vocabulary(app: &tauri::AppHandle) -> Result<(), AppError> {
    use crate::vocabulary::{fixture_card, VocabularyStore};
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let mut bytes = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 || bytes.len() + n > 65536 {
                    break;
                }
                bytes.extend_from_slice(&buf[..n]);
                if let Some(start) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..start]);
                    let size = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= start + 4 + size {
                        break;
                    }
                }
            }
            let body = bytes
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .and_then(|start| {
                    serde_json::from_slice::<serde_json::Value>(&bytes[start + 4..]).ok()
                });
            let input = body
                .as_ref()
                .and_then(|v| v["messages"][1]["content"].as_str())
                .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
            let word = input
                .as_ref()
                .and_then(|v| v["word"].as_str())
                .unwrap_or("");
            let valid = matches!(word, "architecture" | "resilient" | "curious");
            let response = if valid {
                serde_json::json!({"choices":[{"message":{"content":serde_json::to_string(&fixture_card(word)).unwrap()}}]})
            } else {
                serde_json::json!({"error":"Only three isolated fixture words supported"})
            };
            let json = response.to_string();
            let _ = write!(stream, "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", if valid {"200 OK"} else {"400 Bad Request"}, json.len(), json);
        }
    });
    let state = app.state::<crate::app::AppState>();
    let mut settings = state.settings.get()?;
    settings.base_url = format!("http://127.0.0.1:{port}/v1");
    settings.provider = "QA loopback".into();
    settings.model = "QA fixture".into();
    state.settings.replace(settings)?;
    let now = VocabularyStore::now()?;
    if state.vocabulary.list("", "learning", 0, now)?.total == 0 {
        let past = now - 6 * 4 * 3600;
        let mut entries = Vec::new();
        for word in ["architecture", "resilient", "curious"] {
            let e = state.vocabulary.collect(
                word,
                &fixture_card(word).examples[0].english,
                "",
                past,
            )?;
            entries.push(if word == "architecture" {
                e
            } else {
                state
                    .vocabulary
                    .save_card(e.id, e.revision, fixture_card(word), past)?
            });
        }
        for e in entries.iter_mut().skip(1) {
            *e = state.vocabulary.review(
                e.id,
                e.revision,
                &format!("qa-study-{}", e.id),
                "study",
                past,
            )?;
        }
        for step in 1..=3 {
            for e in entries
                .iter_mut()
                .skip(1)
                .filter(|e| step == 1 || e.word == "curious")
            {
                *e = state.vocabulary.review(
                    e.id,
                    e.revision,
                    &format!("qa-review-{}-{step}", e.id),
                    "remember",
                    past + step * 4 * 3600,
                )?;
            }
        }
    }
    Ok(())
}
