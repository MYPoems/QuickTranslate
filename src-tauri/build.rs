/// Every command the frontend can call. Listing them here puts them under Tauri's access
/// control: a window may only call the commands its capability file in `capabilities/`
/// grants, instead of every window being able to call everything.
///
/// Keep this list, `generate_handler!` in `src/lib.rs` and the capability files in step;
/// `tests/ipc.test.ts` fails when they drift apart.
const COMMANDS: &[&str] = &[
    "get_appearance_preferences",
    "get_shortcuts",
    "get_key_status",
    "check_shortcut",
    "open_api_key_page",
    "get_onboarding",
    "complete_onboarding",
    "test_cloud_ocr",
    "set_vocabulary_collection_open",
    "open_vocabulary",
    "list_vocabulary",
    "get_vocabulary_entry",
    "collect_vocabulary",
    "generate_vocabulary",
    "review_vocabulary",
    "save_vocabulary_card",
    "adopt_vocabulary_lemma",
    "begin_vocabulary_quiz",
    "submit_vocabulary_quiz",
    "abandon_vocabulary_quiz",
    "delete_vocabulary",
    "set_vocabulary_rules",
    "export_vocabulary",
    "import_vocabulary",
    "list_speech_voices",
    "get_speech_preferences",
    "stream_cloud_speech",
    "test_cloud_speech",
    "synthesize_speech",
    "stop_speech",
    "get_speech_plugin_status",
    "install_speech_plugin",
    "cancel_speech_plugin_install",
    "uninstall_speech_plugin",
    "translate_selected_text",
    "translate_text",
    "retranslate_text",
    "translate_input",
    "open_settings",
    "copy_translation",
    "clear_translation_cache",
    "list_translation_history",
    "open_history_translation",
    "set_history_favorite",
    "delete_history_entry",
    "get_popup_pinned",
    "set_popup_pinned",
    "recognize_ocr_region",
    "complete_paddle_ocr",
    "fail_paddle_ocr",
    "get_paddle_ocr_plugin_status",
    "install_paddle_ocr_plugin",
    "uninstall_paddle_ocr_plugin",
    "hide_ocr_window",
    "hide_translation_window",
    "get_settings",
    "save_settings",
    "test_provider",
    "get_diagnostics",
    "check_for_updates",
    "get_update_state",
    "download_update",
    "cancel_update_download",
    "install_update",
    "export_settings_backup",
    "import_settings_backup",
];

fn main() {
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows-app-manifest.xml"));
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(windows)
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run Tauri build script");
}
