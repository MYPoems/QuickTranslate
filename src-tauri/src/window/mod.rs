use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::platform;

pub fn show_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("popup") {
        if let Some(position) = platform::popup_placement(420, 260) {
            let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("QuickTranslate 设置")
        .inner_size(520.0, 650.0)
        .min_inner_size(460.0, 560.0)
        .resizable(true)
        .center()
        .build();
}
