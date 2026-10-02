use crate::{app::AppState, speech_plugin::PluginStatus};
use crate::{
    errors::AppError,
    speech::{SpeechAudio, SpeechVoice},
};
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub async fn list_speech_voices(app: AppHandle) -> Result<Vec<SpeechVoice>, AppError> {
    #[cfg(windows)]
    {
        let plugin = app.state::<AppState>().speech_plugin.clone();
        tokio::task::spawn_blocking(move || {
            let mut voices = plugin.voices();
            match crate::speech::native::voices() {
                Ok(system) => voices.extend(system),
                Err(err) if voices.is_empty() => return Err(err),
                Err(_) => (),
            }
            Ok(voices)
        })
        .await
        .map_err(|error| AppError::Speech(error.to_string()))?
    }
    #[cfg(not(windows))]
    {
        Err(AppError::UnsupportedPlatform)
    }
}

#[tauri::command]
pub async fn synthesize_speech(
    text: String,
    language: String,
    voice_id: String,
    app: AppHandle,
) -> Result<SpeechAudio, AppError> {
    #[cfg(windows)]
    {
        let plugin = app.state::<AppState>().speech_plugin.clone();
        tokio::task::spawn_blocking(move || {
            if voice_id.starts_with("plugin:") {
                return plugin.synthesize(&text, &voice_id, &language);
            }
            let system = crate::speech::native::synthesize(&text, &voice_id, &language);
            // Only automatic selection may use the plugin when a system voice is unavailable.
            if system.is_err() && voice_id.is_empty() && plugin.status()?.installed {
                plugin.synthesize(&text, "", &language)
            } else {
                system
            }
        })
        .await
        .map_err(|error| AppError::Speech(error.to_string()))?
    }
    #[cfg(not(windows))]
    {
        let _ = (text, language, voice_id);
        Err(AppError::UnsupportedPlatform)
    }
}

#[tauri::command]
pub async fn get_speech_plugin_status(app: AppHandle) -> Result<PluginStatus, AppError> {
    let plugin = app.state::<AppState>().speech_plugin.clone();
    tokio::task::spawn_blocking(move || plugin.status())
        .await
        .map_err(|err| AppError::Speech(err.to_string()))?
}
#[tauri::command]
pub async fn install_speech_plugin(app: AppHandle) -> Result<PluginStatus, AppError> {
    let state = app.state::<AppState>();
    state
        .speech_plugin
        .clone()
        .install(state.http_client.clone(), |status| {
            let _ = app.emit("speech-plugin-progress", status);
        })
        .await
}
#[tauri::command]
pub fn cancel_speech_plugin_install(app: AppHandle) -> Result<(), AppError> {
    app.state::<AppState>().speech_plugin.cancel()
}
#[tauri::command]
pub fn stop_speech(app: AppHandle) {
    app.state::<AppState>().speech_plugin.stop_synthesis();
}
#[tauri::command]
pub async fn uninstall_speech_plugin(app: AppHandle) -> Result<PluginStatus, AppError> {
    let plugin = app.state::<AppState>().speech_plugin.clone();
    tokio::task::spawn_blocking(move || plugin.uninstall())
        .await
        .map_err(|err| AppError::Speech(err.to_string()))?
}
