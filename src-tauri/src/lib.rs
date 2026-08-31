#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
mod app;
#[cfg(not(test))]
mod commands;
mod config;
mod errors;
mod platform;
mod providers;
mod security;
mod storage;
mod translation;
#[cfg(not(test))]
mod tray;
#[cfg(not(test))]
mod window;

#[cfg(not(test))]
use app::{trigger_selected_translation, AppState};
#[cfg(not(test))]
use tauri::Manager;
#[cfg(not(test))]
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            ShortcutBuilder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        trigger_selected_translation(app.clone());
                    }
                })
                .build(),
        )
        .setup(|app| {
            let state = AppState::initialize(app.handle())
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let shortcut = state
                .settings
                .get()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .global_shortcut;
            app.manage(state);
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            app.global_shortcut().register(shortcut.as_str())?;
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. }
                if window.label() == "popup" || window.label() == "settings" =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(false) if window.label() == "popup" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::translation::translate_selected_text,
            commands::translation::translate_text,
            commands::translation::copy_translation,
            commands::translation::hide_translation_window,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::test_provider,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run QuickTranslate");
}
