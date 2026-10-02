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
#[cfg(all(debug_assertions, not(test)))]
mod qa;
mod security;
mod speech;
mod speech_cache;
mod speech_cloud;
mod speech_plugin;
#[cfg(windows)]
mod speech_worker;
mod vocabulary;
mod vocabulary_model;

#[cfg(windows)]
pub fn run_speech_worker() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        return;
    }
    let threads = args[3].to_string_lossy().parse().unwrap_or(4);
    let _ = speech_worker::run(std::path::Path::new(&args[2]), threads);
}
mod storage;
mod translation;
#[cfg(not(test))]
mod tray;
mod update;
mod upgrade;
#[cfg(not(test))]
mod window;
mod window_state;

#[cfg(not(test))]
use app::{trigger_selected_translation, AppState};
#[cfg(not(test))]
use tauri::{Emitter, Manager};
#[cfg(not(test))]
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, Shortcut, ShortcutState};

#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::update::UpdateManager::default())
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
            window::restore_popup_size(app.handle());
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            #[cfg(debug_assertions)]
            if qa::directory().is_some() {
                qa::setup(app.handle());
                return Ok(());
            }
            app.global_shortcut()
                .register(settings.global_shortcut.as_str())?;
            app.global_shortcut()
                .register(settings.ocr_shortcut.as_str())?;
            tray::setup(app)?;
            if std::env::args().any(|arg| arg == "--vocabulary") {
                window::show_vocabulary(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. }
                if matches!(
                    window.label(),
                    "popup" | "settings" | "history" | "ocr" | "vocabulary"
                ) =>
            {
                api.prevent_close();
                if window.label() == "popup" {
                    window
                        .app_handle()
                        .state::<AppState>()
                        .vocabulary_collection_open
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    let _ = window.emit("popup-hidden", ());
                }
                if window.label() == "vocabulary" {
                    let _ = window.emit("vocabulary-hidden", ());
                }
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(false)
                if window.label() == "popup"
                    && !window.app_handle().state::<AppState>().popup_pinned()
                    && !window
                        .app_handle()
                        .state::<AppState>()
                        .vocabulary_collection_open
                        .load(std::sync::atomic::Ordering::Relaxed) =>
            {
                let _ = window.emit("popup-hidden", ());
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(focused) if window.label() == "ocr" => {
                window::handle_ocr_focus_change(window, *focused);
            }
            tauri::WindowEvent::Resized(size) if window.label() == "popup" => {
                let scale_factor = window.scale_factor().unwrap_or(1.0);
                let logical = size.to_logical::<f64>(scale_factor);
                let popup_size =
                    std::sync::Arc::clone(&window.app_handle().state::<AppState>().popup_size);
                if let Some(revision) = popup_size.update(logical.width, logical.height) {
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(window_state::SAVE_DEBOUNCE).await;
                        let _ = popup_size.persist_if_current(revision);
                    });
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::vocabulary::set_vocabulary_collection_open,
            commands::vocabulary::open_vocabulary,
            commands::vocabulary::list_vocabulary,
            commands::vocabulary::get_vocabulary_entry,
            commands::vocabulary::collect_vocabulary,
            commands::vocabulary::generate_vocabulary,
            commands::vocabulary::review_vocabulary,
            commands::vocabulary::save_vocabulary_card,
            commands::vocabulary::adopt_vocabulary_lemma,
            commands::vocabulary::begin_vocabulary_quiz,
            commands::vocabulary::submit_vocabulary_quiz,
            commands::vocabulary::abandon_vocabulary_quiz,
            commands::vocabulary::delete_vocabulary,
            commands::vocabulary::set_vocabulary_rules,
            commands::vocabulary::export_vocabulary,
            commands::vocabulary::import_vocabulary,
            commands::speech::list_speech_voices,
            commands::speech::get_speech_preferences,
            commands::speech::stream_cloud_speech,
            commands::speech::test_cloud_speech,
            commands::speech::synthesize_speech,
            commands::speech::stop_speech,
            commands::speech::get_speech_plugin_status,
            commands::speech::install_speech_plugin,
            commands::speech::cancel_speech_plugin_install,
            commands::speech::uninstall_speech_plugin,
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
            commands::update::check_for_updates,
            commands::update::get_update_state,
            commands::update::download_update,
            commands::update::cancel_update_download,
            commands::update::install_update,
            commands::settings::export_settings_backup,
            commands::settings::import_settings_backup,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run QuickTranslate");
}
