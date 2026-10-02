use crate::{
    errors::AppError,
    speech::{SpeechAudio, SpeechVoice},
};
use tauri::AppHandle;

#[tauri::command]
pub async fn list_speech_voices() -> Result<Vec<SpeechVoice>, AppError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(crate::speech::native::voices)
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
    _app: AppHandle,
) -> Result<SpeechAudio, AppError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || {
            crate::speech::native::synthesize(&text, &voice_id, &language)
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
