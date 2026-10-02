use serde::{Deserialize, Serialize};

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SpeechProvider {
    #[default]
    Cloud,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeechPreferences {
    pub provider: SpeechProvider,
    pub cloud_endpoint: String,
    pub cloud_model: String,
    pub cloud_chinese_voice: String,
    pub cloud_english_voice: String,
    pub threads: u16,
    pub rate: u16,
    pub chinese_voice: String,
    pub english_voice: String,
    pub bilingual: bool,
}

impl Default for SpeechPreferences {
    fn default() -> Self {
        Self {
            provider: SpeechProvider::Cloud,
            cloud_endpoint: "wss://dashscope.aliyuncs.com/api-ws/v1/realtime".into(),
            cloud_model: "qwen3-tts-flash-realtime".into(),
            cloud_chinese_voice: "Cherry".into(),
            cloud_english_voice: "Cherry".into(),
            threads: 4,
            rate: 100,
            chinese_voice: String::new(),
            english_voice: String::new(),
            bilingual: false,
        }
    }
}

impl SpeechPreferences {
    pub fn validate(&self) -> Result<(), AppError> {
        crate::speech_cloud::validate_endpoint(&self.cloud_endpoint)?;
        if !self.cloud_model.starts_with("qwen3-tts-")
            || self.cloud_model.len() > 128
            || !self.cloud_model.ends_with("realtime") && !self.cloud_model.contains("realtime-")
        {
            return Err(AppError::Settings("请选择 Qwen3-TTS Realtime 模型".into()));
        }
        for voice in [&self.cloud_chinese_voice, &self.cloud_english_voice] {
            if voice.is_empty()
                || voice.len() > 128
                || !voice
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            {
                return Err(AppError::Settings("云端音色名称无效".into()));
            }
        }
        if !(1..=8).contains(&self.threads) {
            return Err(AppError::Settings("离线语音线程数必须在 1–8 之间".into()));
        }
        if !(50..=200).contains(&self.rate) {
            return Err(AppError::Settings("朗读语速必须在 50%–200% 之间".into()));
        }
        if self.chinese_voice.len() > 1024 || self.english_voice.len() > 1024 {
            return Err(AppError::Settings("音色标识过长".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechVoice {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAudio {
    pub audio_data_url: String,
    pub voice_name: String,
}

#[cfg(windows)]
pub mod native {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::sync::{mpsc, OnceLock};
    use windows::{
        core::HSTRING,
        Media::SpeechSynthesis::SpeechSynthesizer,
        Storage::Streams::DataReader,
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };

    struct Runtime;
    impl Runtime {
        fn initialize() -> Result<Self, AppError> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(speech_error)?;
            Ok(Self)
        }
    }
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    fn speech_error(error: windows::core::Error) -> AppError {
        AppError::Speech(format!("Windows 语音服务不可用：{error}"))
    }

    // WinRT factory caches must not outlive their apartment. Keep one initialized
    // apartment for the entire speech service instead of repeatedly tearing it down
    // on Tokio's pooled threads (which can invalidate the speech factory cache).
    type Job = Box<dyn FnOnce(Result<(), AppError>) + Send>;
    fn dispatch<T: Send + 'static>(
        work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
    ) -> Result<T, AppError> {
        static WORKER: OnceLock<Result<mpsc::SyncSender<Job>, String>> = OnceLock::new();
        let worker = WORKER
            .get_or_init(|| {
                let (sender, receiver) = mpsc::sync_channel::<Job>(8);
                std::thread::Builder::new()
                    .name("quicktranslate-speech".into())
                    .spawn(move || {
                        let runtime = Runtime::initialize();
                        for job in receiver {
                            job(runtime.as_ref().map(|_| ()).map_err(Clone::clone));
                        }
                    })
                    .map_err(|error| error.to_string())?;
                Ok(sender)
            })
            .as_ref()
            .map_err(|error| AppError::Speech(error.clone()))?;
        let (sender, receiver) = mpsc::channel();
        worker
            .try_send(Box::new(move |initialized| {
                let _ = sender.send(initialized.and_then(|_| work()));
            }))
            .map_err(|_| AppError::Speech("语音服务繁忙或已停止，请稍后重试".into()))?;
        receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| AppError::Speech("语音生成超时，请缩短文本或重新选择音色".into()))?
    }

    pub fn voices() -> Result<Vec<SpeechVoice>, AppError> {
        dispatch(voices_inner)
    }
    fn voices_inner() -> Result<Vec<SpeechVoice>, AppError> {
        let voices = SpeechSynthesizer::AllVoices().map_err(speech_error)?;
        voices
            .into_iter()
            .map(|voice| {
                Ok(SpeechVoice {
                    id: voice.Id().map_err(speech_error)?.to_string(),
                    name: voice.DisplayName().map_err(speech_error)?.to_string(),
                    language: voice.Language().map_err(speech_error)?.to_string(),
                })
            })
            .collect()
    }

    pub fn synthesize(text: &str, voice_id: &str, language: &str) -> Result<SpeechAudio, AppError> {
        if text.trim().is_empty() || text.chars().count() > 600 {
            return Err(AppError::Speech("朗读段落为空或超过 600 字符".into()));
        }
        if !matches!(language, "zh" | "en") {
            return Err(AppError::Speech("仅支持中文与英文朗读".into()));
        }
        let (text, voice_id, language) =
            (text.to_string(), voice_id.to_string(), language.to_string());
        dispatch(move || synthesize_inner(&text, &voice_id, &language))
    }
    fn synthesize_inner(
        text: &str,
        voice_id: &str,
        language: &str,
    ) -> Result<SpeechAudio, AppError> {
        let voices = SpeechSynthesizer::AllVoices().map_err(speech_error)?;
        let selected = voices.into_iter().find(|voice| {
            let matches_language = voice
                .Language()
                .is_ok_and(|value| value.to_string().to_lowercase().starts_with(language));
            matches_language
                && (voice_id.is_empty() || voice.Id().is_ok_and(|value| value == voice_id))
        });
        let voice = selected.ok_or_else(|| {
            AppError::Speech(if voice_id.is_empty() {
                "未找到对应语言的离线音色。请在 Windows 设置 → 时间和语言 → 语音中安装中文/英文语音包，然后重启应用。".into()
            } else {
                "所选音色已不可用。请在应用设置中重新选择音色，或恢复“自动选择”。".into()
            })
        })?;
        let voice_name = voice.DisplayName().map_err(speech_error)?.to_string();
        let synthesizer = SpeechSynthesizer::new().map_err(speech_error)?;
        synthesizer.SetVoice(&voice).map_err(speech_error)?;
        let stream = synthesizer
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))
            .map_err(speech_error)?
            .get()
            .map_err(speech_error)?;
        let size = stream.Size().map_err(speech_error)?;
        if size > 16 * 1024 * 1024 {
            return Err(AppError::Speech("语音数据过大，请缩短段落".into()));
        }
        let input = stream.GetInputStreamAt(0).map_err(speech_error)?;
        let reader = DataReader::CreateDataReader(&input).map_err(speech_error)?;
        let loaded = reader
            .LoadAsync(size as u32)
            .map_err(speech_error)?
            .get()
            .map_err(speech_error)?;
        if u64::from(loaded) != size {
            return Err(AppError::Speech("语音数据读取不完整".into()));
        }
        let mut bytes = vec![0; loaded as usize];
        reader.ReadBytes(&mut bytes).map_err(speech_error)?;
        Ok(SpeechAudio {
            audio_data_url: format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)),
            voice_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_default_and_round_trip() {
        let defaults: SpeechPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults, SpeechPreferences::default());
        let custom = SpeechPreferences {
            rate: 85,
            chinese_voice: "zh-voice".into(),
            english_voice: "en-voice".into(),
            bilingual: true,
            ..SpeechPreferences::default()
        };
        custom.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<SpeechPreferences>(&serde_json::to_string(&custom).unwrap())
                .unwrap(),
            custom
        );
        assert!(SpeechPreferences {
            rate: 0,
            ..defaults
        }
        .validate()
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires an installed Windows speech voice"]
    fn native_voice_and_audio_smoke() {
        let voices = native::voices().unwrap();
        println!(
            "Installed speech languages: {:?}",
            voices
                .iter()
                .map(|voice| &voice.language)
                .collect::<Vec<_>>()
        );
        let voice = voices
            .iter()
            .find(|voice| voice.language.starts_with("zh") || voice.language.starts_with("en"))
            .expect("install a Chinese or English voice");
        let language = if voice.language.starts_with("zh") {
            "zh"
        } else {
            "en"
        };
        let audio = native::synthesize("QuickTranslate speech test.", &voice.id, language).unwrap();
        assert!(audio
            .audio_data_url
            .starts_with("data:audio/wav;base64,UklGR"));
        assert!(native::synthesize("test", "missing-voice", "en").is_err());
        let workers = (0..6)
            .map(|_| std::thread::spawn(native::voices))
            .collect::<Vec<_>>();
        for worker in workers {
            assert!(!worker.join().unwrap().unwrap().is_empty());
        }
        for _ in 0..3 {
            let audio = native::synthesize("连续调用中文语音测试。", &voice.id, language).unwrap();
            assert!(audio
                .audio_data_url
                .starts_with("data:audio/wav;base64,UklGR"));
        }
    }
}
