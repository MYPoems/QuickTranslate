use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::platform;

pub fn show_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("popup") {
        let (width, height) = window
            .outer_size()
            .map(|size| {
                (
                    i32::try_from(size.width).unwrap_or(i32::MAX),
                    i32::try_from(size.height).unwrap_or(i32::MAX),
                )
            })
            .unwrap_or((420, 260));

        if let Some(position) = platform::popup_placement(width, height) {
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

pub fn show_history(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("history") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(app, "history", WebviewUrl::App("index.html".into()))
        .title("QuickTranslate 翻译历史")
        .inner_size(760.0, 680.0)
        .min_inner_size(560.0, 460.0)
        .resizable(true)
        .center()
        .build();
}
