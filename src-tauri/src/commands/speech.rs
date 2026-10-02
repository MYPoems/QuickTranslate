use crate::{app::AppState, speech_plugin::PluginStatus};
use crate::{
    errors::AppError,
    speech::{SpeechAudio, SpeechVoice},
};
use tauri::{ipc::Channel, AppHandle, Emitter, Manager};

#[tauri::command]
pub fn get_speech_preferences(
    app: AppHandle,
) -> Result<crate::speech::SpeechPreferences, AppError> {
    Ok(app.state::<AppState>().settings.get()?.speech)
}

#[tauri::command]
pub async fn stream_cloud_speech(
    text: String,
    language: String,
    on_audio: Channel<crate::speech_cloud::SpeechPacket>,
    app: AppHandle,
) -> Result<crate::speech_cloud::SpeechTiming, AppError> {
    let state = app.state::<AppState>();
    let settings = state.settings.get()?.speech;
    if settings.provider != crate::speech::SpeechProvider::Cloud {
        return Err(AppError::Speech("请先在设置中选择云端朗读".into()));
    }
    let key = state.secrets.get_cloud_speech_api_key()?.ok_or_else(|| {
        AppError::Speech("尚未配置云端朗读 API Key；请打开设置配置，或手动选择离线朗读".into())
    })?;
    let service = state.cloud_speech.clone();
    let result = service
        .speak(&text, &language, &settings, &key, |packet| {
            on_audio
                .send(packet)
                .map_err(|_| AppError::Speech("播放窗口已关闭".into()))
        })
        .await;
    let idle = service.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(301)).await;
        idle.release_idle().await;
    });
    result
}

#[tauri::command]
pub async fn test_cloud_speech(
    update: crate::config::UpdateSettings,
    app: AppHandle,
) -> Result<crate::speech_cloud::SpeechTiming, AppError> {
    let settings = crate::config::SettingsStore::validate(&update)?.speech;
    let state = app.state::<AppState>();
    let key = if update.clear_cloud_speech_api_key {
        None
    } else {
        update
            .cloud_speech_api_key
            .filter(|key| !key.trim().is_empty())
            .or(state.secrets.get_cloud_speech_api_key()?)
    }
    .ok_or_else(|| AppError::Speech("请填写云端朗读 API Key 后测试".into()))?;
    // Isolated test session: never changes saved settings or interrupts current reading.
    crate::speech_cloud::CloudSpeech::default()
        .speak(
            "你好，语音连接测试。",
            "zh",
            &settings,
            key.trim(),
            |_| Ok(()),
        )
        .await
}

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
        let settings = app.state::<AppState>().settings.get()?.speech;
        if settings.provider != crate::speech::SpeechProvider::Offline {
            return Err(AppError::Speech(
                "当前选择云端朗读，请使用流式播放接口".into(),
            ));
        }
        tokio::task::spawn_blocking(move || {
            if voice_id.starts_with("plugin:") {
                return plugin.synthesize_with_threads(
                    &text,
                    &voice_id,
                    &language,
                    settings.threads,
                );
            }
            let system = crate::speech::native::synthesize(&text, &voice_id, &language);
            // Only automatic selection may use the plugin when a system voice is unavailable.
            if system.is_err() && voice_id.is_empty() && plugin.status()?.installed {
                plugin.synthesize_with_threads(&text, "", &language, settings.threads)
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
    app.state::<AppState>().cloud_speech.stop();
}
#[tauri::command]
pub async fn uninstall_speech_plugin(app: AppHandle) -> Result<PluginStatus, AppError> {
    let plugin = app.state::<AppState>().speech_plugin.clone();
    tokio::task::spawn_blocking(move || plugin.uninstall())
        .await
        .map_err(|err| AppError::Speech(err.to_string()))?
}
