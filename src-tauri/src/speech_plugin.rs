//! Optional neural voices: verified upstream packages, no Windows voice redistribution.
use crate::{
    errors::AppError,
    speech::{SpeechAudio, SpeechVoice},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const RUNTIME: &str = "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release";
const MODEL: &str = "kokoro-int8-multi-lang-v1_1";
const VERSION: &str = "kokoro-1.1-int8/sherpa-1.13.8";
const PACKAGES: [(&str, &str, u64, &str); 2] = [
    ("runtime.tar.bz2", "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-win-x64-shared-MT-Release.tar.bz2", 24_805_859, "6DFFDC715A4465B989446A6105265D2CB345E7101591A17D35534B6758F6E8DF"),
    ("model.tar.bz2", "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2", 147_031_220, "A1E94694776049035C4F2C6529F003AAECE993C76AAE9A78995831C3C4DCAFC6"),
];
// Independently pinned executable, DLL and model hashes, never trusted from local metadata.
const FILES: [(&str, u64, &str); 8] = [
    (
        "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release/bin/sherpa-onnx-offline-tts.exe",
        2_768_896,
        "17AE204C3D82E05A15D96C37E57DDD6210E9FFE27D1F0408D99EA54BA1B2D2F6",
    ),
    (
        "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release/bin/onnxruntime.dll",
        17_799_168,
        "7F66F939A881BAF4F46A2216496798EDF4A1429878B646D12674AA62F27D8A25",
    ),
    (
        "sherpa-onnx-v1.13.8-win-x64-shared-MT-Release/bin/onnxruntime_providers_shared.dll",
        104_960,
        "551D0E1FE4C227D8542314BA718D52F4379E0C7BFE729A37C59833A884E27B4D",
    ),
    (
        "kokoro-int8-multi-lang-v1_1/model.int8.onnx",
        114_299_010,
        "BDA15858163726A492D02A9A727BC263551B86AC77F90812C4B30FF41D380E26",
    ),
    (
        "kokoro-int8-multi-lang-v1_1/voices.bin",
        53_790_720,
        "E64A5A581D8C2A350D848F51C3121657CD83AA07ED6109172177345874A7244C",
    ),
    (
        "kokoro-int8-multi-lang-v1_1/tokens.txt",
        1_111,
        "931AB2DF2400CD65D580A22402024C2347CED8AE9EA300E545144B1AACC48E14",
    ),
    (
        "kokoro-int8-multi-lang-v1_1/lexicon-us-en.txt",
        5_956_885,
        "7DAAAB53A181BE9885B853A8582BF1838186317E5DADACBCEF9C426D6FA0DA14",
    ),
    (
        "kokoro-int8-multi-lang-v1_1/lexicon-zh.txt",
        2_119_465,
        "11111D8CD695FBA2ACE1367A1D0A708B586E6EF5C1F9BE91DA5D7EEF129B651C",
    ),
];

#[derive(Clone, Default, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub installed: bool,
    pub present: bool,
    pub phase: String,
    pub downloaded: u64,
    pub download_bytes: u64,
    pub version: String,
    pub message: String,
}
pub struct SpeechPlugin {
    pub root: PathBuf,
    gate: Arc<tokio::sync::RwLock<()>>,
    progress: Mutex<PluginStatus>,
    cancel: Mutex<CancellationToken>,
    synthesis: Mutex<()>,
    generation: AtomicU64,
}
fn error(message: impl std::fmt::Display) -> AppError {
    AppError::Speech(format!("音色插件：{message}"))
}
impl SpeechPlugin {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("speech-plugins/kokoro-v1.1"),
            gate: Arc::new(tokio::sync::RwLock::new(())),
            progress: Mutex::new(PluginStatus {
                phase: "idle".into(),
                download_bytes: PACKAGES.iter().map(|p| p.2).sum(),
                version: VERSION.into(),
                ..Default::default()
            }),
            cancel: Mutex::new(CancellationToken::new()),
            synthesis: Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }
    pub fn status(&self) -> Result<PluginStatus, AppError> {
        let mut status = self.progress.lock().map_err(error)?.clone();
        // Full hashes before exposing voices; running synthesis checks again before execution.
        status.installed = verified_root(&self.root).is_ok();
        status.present = self.root.exists();
        Ok(status)
    }
    pub fn voices(&self) -> Vec<SpeechVoice> {
        if verified_root(&self.root).is_ok() {
            plugin_voices()
        } else {
            Vec::new()
        }
    }
    pub fn cancel(&self) -> Result<(), AppError> {
        self.cancel.lock().map_err(error)?.cancel();
        Ok(())
    }
    pub fn stop_synthesis(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    pub async fn install(
        self: Arc<Self>,
        client: reqwest::Client,
        publish: impl Fn(&PluginStatus) + Send + Sync,
    ) -> Result<PluginStatus, AppError> {
        let _guard = self
            .gate
            .clone()
            .try_write_owned()
            .map_err(|_| error("任务正在进行，请稍后重试"))?;
        if self.status()?.installed {
            return self.status();
        }
        let token = CancellationToken::new();
        *self.cancel.lock().map_err(error)? = token.clone();
        {
            let mut state = self.progress.lock().map_err(error)?;
            state.phase = "downloading".into();
            state.downloaded = 0;
            state.message = "正在下载中英文音色与运行时…".into();
            publish(&state);
        }
        let result = async {
            let parent = self.root.parent().ok_or_else(|| error("无效的插件路径"))?;
            fs::create_dir_all(parent).map_err(error)?;
            let stage = tempfile::Builder::new()
                .prefix(".speech-install-")
                .tempdir_in(parent)
                .map_err(error)?;
            for (name, url, size, hash) in PACKAGES {
                let path = stage.path().join(name);
                let mut response = token
                    .run_until_cancelled(client.get(url).timeout(Duration::from_secs(900)).send())
                    .await
                    .ok_or_else(|| error("下载已取消"))?
                    .map_err(error)?
                    .error_for_status()
                    .map_err(error)?;
                if response.content_length().is_some_and(|n| n != size) {
                    return Err(error("下载大小不匹配"));
                }
                let mut file = fs::File::create(&path).map_err(error)?;
                let mut count = 0;
                let mut digest = Sha256::new();
                let mut last_publish = Instant::now();
                while let Some(chunk) = token
                    .run_until_cancelled(response.chunk())
                    .await
                    .ok_or_else(|| error("下载已取消"))?
                    .map_err(error)?
                {
                    if token.is_cancelled() {
                        return Err(error("下载已取消"));
                    }
                    count += chunk.len() as u64;
                    if count > size {
                        return Err(error("下载超过固定大小限制"));
                    }
                    file.write_all(&chunk).map_err(error)?;
                    digest.update(&chunk);
                    let mut state = self.progress.lock().map_err(error)?;
                    state.downloaded += chunk.len() as u64;
                    if last_publish.elapsed() >= Duration::from_millis(200) || count == size {
                        publish(&state);
                        last_publish = Instant::now();
                    }
                }
                file.sync_all().map_err(error)?;
                if count != size || hex::encode_upper(digest.finalize()) != hash {
                    return Err(error("下载 SHA-256 校验失败，未启用插件"));
                }
            }
            {
                let mut state = self.progress.lock().map_err(error)?;
                state.phase = "verifying".into();
                state.message = "正在安全解压与验证插件…".into();
                publish(&state);
            }
            let root = self.root.clone();
            let verify_token = token.clone();
            tokio::task::spawn_blocking(move || {
                let expanded = stage.path().join("expanded");
                fs::create_dir(&expanded).map_err(error)?;
                for (index, package) in PACKAGES.iter().enumerate() {
                    extract_archive(
                        &stage.path().join(package.0),
                        &expanded,
                        if index == 0 { RUNTIME } else { MODEL },
                    )?;
                    if verify_token.is_cancelled() {
                        return Err(error("下载已取消"));
                    }
                }
                verified_root(&expanded)?;
                fs::write(
                    expanded.join("THIRD-PARTY-NOTICES.md"),
                    include_str!("../../docs/SPEECH-PLUGIN.md"),
                )
                .map_err(error)?;
                // Failed/cancelled downloads never replace an existing installation.
                if verify_token.is_cancelled() {
                    return Err(error("下载已取消"));
                }
                if root.exists() {
                    return Err(error("已有不完整插件目录，请先卸载后重装"));
                }
                fs::rename(expanded, root).map_err(error)?;
                Ok::<_, AppError>(())
            })
            .await
            .map_err(error)??;
            Ok::<_, AppError>(())
        }
        .await;
        {
            let mut state = self.progress.lock().map_err(error)?;
            state.phase = if result.is_ok() {
                "ready"
            } else if token.is_cancelled() {
                "cancelled"
            } else {
                "error"
            }
            .into();
            state.message = match &result {
                Ok(_) => "中英文音色插件已安装，离线可用".into(),
                Err(err) => err.user_message(),
            };
        }
        let state = self.status()?;
        publish(&state);
        result?;
        Ok(state)
    }
    pub fn uninstall(&self) -> Result<PluginStatus, AppError> {
        let _guard = self
            .gate
            .clone()
            .try_write_owned()
            .map_err(|_| error("请先停止朗读或取消安装并等待完成"))?;
        // This target is constructed only by new(), never from frontend input.
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(error)?;
        }
        {
            let mut state = self.progress.lock().map_err(error)?;
            state.phase = "idle".into();
            state.message = "插件已卸载，系统音色仍可使用".into();
            state.downloaded = 0;
        }
        self.status()
    }
    #[cfg(windows)]
    pub fn synthesize(
        &self,
        text: &str,
        voice: &str,
        language: &str,
    ) -> Result<SpeechAudio, AppError> {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let sid = speaker_id(voice, language)?;
        if text.trim().is_empty() || text.chars().count() > 600 {
            return Err(error("段落为空或超过 600 字符"));
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _guard = self
            .gate
            .clone()
            .try_read_owned()
            .map_err(|_| error("正在安装/卸载，请稍后朗读"))?;
        let _synthesis = self.synthesis.lock().map_err(error)?;
        verified_root(&self.root)?;
        if self.generation.load(Ordering::SeqCst) != generation {
            return Err(error("朗读已取消"));
        }
        // Private short-lived WAV only; no OCR screenshots or user text saved to disk.
        let output = tempfile::Builder::new()
            .prefix(".speech-audio-")
            .tempdir_in(&self.root)
            .map_err(error)?;
        let wav = output.path().join("audio.wav");
        let relative = wav
            .strip_prefix(&self.root)
            .map_err(error)?
            .to_string_lossy();
        let mut command = Command::new(self.root.join(FILES[0].0));
        command
            .current_dir(&self.root)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "--print-args=false",
                "--num-threads=4",
                "--debug=false",
                "--provider=cpu",
            ])
            .arg(format!("--kokoro-model={MODEL}/model.int8.onnx"))
            .arg(format!("--kokoro-voices={MODEL}/voices.bin"))
            .arg(format!("--kokoro-tokens={MODEL}/tokens.txt"))
            .arg(format!("--kokoro-data-dir={MODEL}/espeak-ng-data"))
            .arg(format!(
                "--kokoro-lexicon={MODEL}/lexicon-us-en.txt,{MODEL}/lexicon-zh.txt"
            ))
            .arg(format!("--sid={sid}"))
            .arg(format!("--output-filename={relative}"))
            .arg("--")
            .arg(text);
        let mut child = command
            .spawn()
            .map_err(|_| error("无法启动离线语音运行时，请重新安装插件"))?;
        let start = Instant::now();
        loop {
            if self.generation.load(Ordering::SeqCst) != generation {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("朗读已取消"));
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) => return Err(error("离线合成失败，请重试或改用系统音色")),
                Ok(None) if start.elapsed() < Duration::from_secs(120) => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error("离线合成超时，请缩短文字或改用系统音色"));
                }
            }
        }
        if fs::metadata(&wav).map_err(error)?.len() > 16 * 1024 * 1024 {
            return Err(error("生成音频超出大小限制"));
        }
        let bytes = fs::read(&wav).map_err(error)?;
        if !valid_wav(&bytes) {
            return Err(error("语音文件无效"));
        }
        let name = plugin_voices()
            .into_iter()
            .find(|v| v.id == voice)
            .map(|v| v.name)
            .unwrap_or_else(|| "Kokoro 离线音色".into());
        Ok(SpeechAudio {
            audio_data_url: format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)),
            voice_name: name,
        })
    }
}
fn speaker_id(voice: &str, language: &str) -> Result<u16, AppError> {
    if !matches!(language, "zh" | "en") {
        return Err(error("仅支持中英文"));
    }
    if voice.is_empty() {
        return Ok(if language == "zh" { 3 } else { 0 });
    }
    let selected = plugin_voices()
        .into_iter()
        .find(|v| v.id == voice && v.language.starts_with(language))
        .ok_or_else(|| error("音色与语言不匹配，或插件音色已失效"))?;
    selected
        .id
        .rsplit(':')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| error("无效音色"))
}
pub fn plugin_voices() -> Vec<SpeechVoice> {
    [
        ("zh-CN", 3, "中文女声 · Kokoro 001"),
        ("zh-CN", 4, "中文女声 · Kokoro 002"),
        ("zh-CN", 58, "中文男声 · Kokoro 001"),
        ("en-US", 0, "美式女声 · Maple"),
        ("en-US", 1, "美式女声 · Sol"),
        ("en-GB", 2, "英式女声 · Vale"),
    ]
    .into_iter()
    .map(|(language, sid, name)| SpeechVoice {
        id: format!("plugin:kokoro:{sid}"),
        name: name.into(),
        language: language.into(),
    })
    .collect()
}
fn file_hash(path: &Path) -> Result<String, AppError> {
    let mut reader = fs::File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = reader.read(&mut buffer).map_err(error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode_upper(hash.finalize()))
}
fn verified_root(root: &Path) -> Result<(), AppError> {
    for (name, size, hash) in FILES {
        let path = root.join(name);
        if !fs::symlink_metadata(&path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() == size)
            || file_hash(&path)? != hash
        {
            return Err(error("插件缺失或完整性校验失败，请安装/重新安装"));
        }
    }
    if !root.join(MODEL).join("espeak-ng-data").is_dir() {
        return Err(error("缺少发音数据"));
    }
    Ok(())
}
fn safe_archive_path(path: &Path, expected_root: &str) -> bool {
    let mut components = path.components();
    if components.next() != Some(Component::Normal(expected_root.as_ref())) {
        return false;
    }
    components.all(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(|s| {
            let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
            let device = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
                || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.len() == 4
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'));
            !s.contains([':', '\\']) && !s.ends_with(['.', ' ']) && !s.is_empty() && !device
        }),
        _ => false,
    })
}
fn extract_archive(path: &Path, dest: &Path, root: &str) -> Result<(), AppError> {
    let file = fs::File::open(path).map_err(error)?;
    let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(file));
    let mut total = 0u64;
    for (index, entry) in archive.entries().map_err(error)?.enumerate() {
        let mut entry = entry.map_err(error)?;
        let kind = entry.header().entry_type();
        if index > 10000
            || !(kind.is_file() || kind.is_dir())
            || !safe_archive_path(&entry.path().map_err(error)?, root)
        {
            return Err(error("压缩包含不安全路径或链接"));
        }
        total += entry.size();
        if total > 1024 * 1024 * 1024 {
            return Err(error("插件解压超出大小限制"));
        }
        if !entry.unpack_in(dest).map_err(error)? {
            return Err(error("压缩包路径越界"));
        }
    }
    Ok(())
}
fn valid_wav(bytes: &[u8]) -> bool {
    bytes.len() > 44 && bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WAVE")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_guard_prevents_uninstall_during_reading() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = SpeechPlugin::new(temp.path());
        let guard = plugin.gate.clone().try_read_owned().unwrap();
        assert!(plugin.uninstall().is_err());
        drop(guard);
        assert!(plugin.uninstall().is_ok());
    }
    #[test]
    fn extraction_rejects_links_without_creating_targets() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("unsafe.tar.bz2");
        let encoder = bzip2::write::BzEncoder::new(
            fs::File::create(&archive_path).unwrap(),
            bzip2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("../escape").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "model/link", std::io::empty())
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let dest = temp.path().join("expanded");
        fs::create_dir(&dest).unwrap();
        assert!(extract_archive(&archive_path, &dest, "model").is_err());
        assert!(!dest.join("model/link").exists());
    }
    #[test]
    #[ignore = "downloads verified upstream packages to QUICKTRANSLATE_SPEECH_INSTALL_TEST_DIR"]
    fn real_one_click_download_and_install() {
        let data = PathBuf::from(
            std::env::var_os("QUICKTRANSLATE_SPEECH_INSTALL_TEST_DIR")
                .expect("isolated destination"),
        );
        assert!(data.is_absolute());
        let plugin = Arc::new(SpeechPlugin::new(&data));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        let status = runtime
            .block_on(plugin.clone().install(client, |_| {}))
            .unwrap();
        assert!(status.installed);
        assert_eq!(status.phase, "ready");
        assert_eq!(plugin.voices().len(), 6);
        assert!(plugin.root.join("THIRD-PARTY-NOTICES.md").exists());
        // Reopening the application preserves the installed state.
        assert!(SpeechPlugin::new(&data).status().unwrap().installed);
        #[cfg(windows)]
        {
            let audio = plugin
                .synthesize(
                    "Hello from the installed voice plugin.",
                    "plugin:kokoro:0",
                    "en",
                )
                .unwrap();
            assert!(audio
                .audio_data_url
                .starts_with("data:audio/wav;base64,UklGR"));
            let worker = plugin.clone();
            let reading = std::thread::spawn(move || {
                worker.synthesize("这是一段会被停止的中文测试。", "plugin:kokoro:3", "zh")
            });
            std::thread::sleep(Duration::from_millis(500));
            plugin.stop_synthesis();
            assert!(reading.join().unwrap().is_err());
        }
    }
    #[test]
    fn voices_route_and_reject_foreign_ids() {
        assert_eq!(speaker_id("", "zh").unwrap(), 3);
        assert_eq!(speaker_id("", "en").unwrap(), 0);
        assert!(speaker_id("plugin:kokoro:58", "en").is_err());
        assert!(speaker_id("../../evil", "zh").is_err());
        assert!(speaker_id("", "fr").is_err());
    }
    #[test]
    fn archive_paths_are_confined() {
        assert!(safe_archive_path(Path::new("model/tokens.txt"), "model"));
        for path in [
            "../outside",
            "model/../outside",
            "/model/out",
            "model/a:stream",
            "model/evil.",
            "wrong/file",
            "model/NUL.txt",
            "model/COM1",
        ] {
            assert!(!safe_archive_path(Path::new(path), "model"), "{path}");
        }
    }
    #[test]
    fn absent_plugin_and_uninstall_are_safe() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = SpeechPlugin::new(temp.path());
        assert!(!plugin.status().unwrap().installed);
        assert!(plugin.voices().is_empty());
        assert!(!plugin.uninstall().unwrap().installed);
        assert_eq!(plugin.status().unwrap().download_bytes, 171_837_079);
    }
    #[test]
    fn wav_validation_and_hash() {
        assert!(!valid_wav(b"not wave"));
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("hash");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(
            file_hash(&file).unwrap(),
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        );
    }
    #[test]
    #[ignore = "requires verified upstream archives in QUICKTRANSLATE_SPEECH_TEST_DIR"]
    fn real_packages_extract_verify_and_speak_both_languages() {
        let source = PathBuf::from(std::env::var_os("QUICKTRANSLATE_SPEECH_TEST_DIR").unwrap());
        let temp = tempfile::tempdir().unwrap();
        let plugin = SpeechPlugin::new(temp.path());
        fs::create_dir_all(&plugin.root).unwrap();
        for (index, package) in PACKAGES.iter().enumerate() {
            let archive = source.join(package.0);
            assert_eq!(fs::metadata(&archive).unwrap().len(), package.2);
            assert_eq!(file_hash(&archive).unwrap(), package.3);
            extract_archive(
                &archive,
                &plugin.root,
                if index == 0 { RUNTIME } else { MODEL },
            )
            .unwrap();
        }
        assert!(plugin.status().unwrap().installed);
        assert_eq!(plugin.voices().len(), 6);
        #[cfg(windows)]
        for (text, voice, lang) in [
            ("你好，这是中文音色测试。", "plugin:kokoro:3", "zh"),
            (
                "Hello. This is the English voice test.",
                "plugin:kokoro:0",
                "en",
            ),
        ] {
            assert!(plugin
                .synthesize(text, voice, lang)
                .unwrap()
                .audio_data_url
                .starts_with("data:audio/wav;base64,UklGR"));
        }
        fs::write(plugin.root.join(FILES[0].0), b"tampered").unwrap();
        assert!(!plugin.status().unwrap().installed);
    }
}
