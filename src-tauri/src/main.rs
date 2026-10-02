// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--speech-worker") {
        quicktranslate_lib::run_speech_worker();
        return;
    }
    quicktranslate_lib::run()
}
