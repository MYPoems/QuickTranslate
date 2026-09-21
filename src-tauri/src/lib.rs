#![cfg_attr(test, allow(dead_code, unused_imports))]

#[cfg(not(test))]
mod app;
#[cfg(not(test))]
mod commands;
mod config;
mod errors;
mod ocr;
mod platform;
mod providers;
mod security;
mod storage;
mod translation;
#[cfg(not(test))]
mod tray;
mod update;
#[cfg(not(test))]
mod window;

#[cfg(not(test))]
use app::{trigger_selected_translation, AppState};
#[cfg(not(test))]
use tauri::Manager;
#[cfg(not(test))]
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, Shortcut, ShortcutState};

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
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let settings = app.state::<AppState>().settings.get();
                        let is_ocr = settings
                            .as_ref()
                            .ok()
                            .and_then(|settings| settings.ocr_shortcut.parse::<Shortcut>().ok())
                            .is_some_and(|configured| configured == *shortcut);
                        if is_ocr {
                            window::toggle_ocr_overlay(app);
                        } else {
                            trigger_selected_translation(app.clone());
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let state = AppState::initialize(app.handle())
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let settings = state
                .settings
                .get()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            app.manage(state);
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            app.global_shortcut()
                .register(settings.global_shortcut.as_str())?;
            app.global_shortcut()
                .register(settings.ocr_shortcut.as_str())?;
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. }
                if matches!(window.label(), "popup" | "settings" | "history" | "ocr") =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(false)
                if window.label() == "popup"
                    && !window.app_handle().state::<AppState>().popup_pinned() =>
            {
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(focused) if window.label() == "ocr" => {
                window::handle_ocr_focus_change(window, *focused);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::translation::translate_selected_text,
            commands::translation::translate_text,
            commands::translation::retranslate_text,
            commands::translation::copy_translation,
            commands::translation::clear_translation_cache,
            commands::translation::list_translation_history,
            commands::translation::set_history_favorite,
            commands::translation::delete_history_entry,
            commands::translation::get_popup_pinned,
            commands::translation::set_popup_pinned,
            commands::ocr::recognize_ocr_region,
            commands::ocr::complete_paddle_ocr,
            commands::ocr::fail_paddle_ocr,
            commands::ocr::get_paddle_ocr_plugin_status,
            commands::ocr::install_paddle_ocr_plugin,
            commands::ocr::uninstall_paddle_ocr_plugin,
            commands::ocr::hide_ocr_window,
            commands::translation::hide_translation_window,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::test_provider,
            commands::settings::get_diagnostics,
            commands::settings::check_for_updates,
            commands::settings::export_settings_backup,
            commands::settings::import_settings_backup,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run QuickTranslate");
}
