//! Isolated, reusable Kokoro runtime. ABI is pinned to sherpa-onnx v1.13.8.
//! The main app never loads third-party native speech libraries.
use crate::{errors::AppError, speech::SpeechAudio};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::{
    ffi::{c_char, c_void, CString},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
struct Request {
    text: String,
    sid: u16,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    audio: Option<String>,
    error: Option<String>,
}
fn fail(message: impl std::fmt::Display) -> AppError {
    AppError::Speech(format!("离线语音：{message}"))
}
pub struct Worker {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<Reply>,
    pub threads: u16,
    used: Instant,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Worker {
    pub fn start(root: &Path, threads: u16) -> Result<Self, AppError> {
        let executable = std::env::current_exe().map_err(fail)?;
        // The test harness cannot act as a speech worker; smoke tests supply a built app.
        #[cfg(test)]
        let executable = std::env::var_os("QUICKTRANSLATE_SPEECH_WORKER_EXE")
            .map(std::path::PathBuf::from)
            .unwrap_or(executable);
        let mut child = Command::new(executable)
            .arg("--speech-worker")
            .arg(root)
            .arg(threads.to_string())
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(fail)?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| fail("无法打开工作进程输入"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| fail("无法打开工作进程输出"))?;
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = String::new();
                // Bound any malformed/hostile worker output before deserializing.
                use std::io::Read;
                match reader.by_ref().take(24 * 1024 * 1024).read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if !line.ends_with('\n') => break,
                    Ok(_) => {
                        if let Ok(reply) = serde_json::from_str(&line) {
                            if sender.send(reply).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            replies,
            threads,
            used: Instant::now(),
        })
    }
    pub fn is_alive(&mut self) -> bool {
        self.used.elapsed() < Duration::from_secs(295) && matches!(self.child.try_wait(), Ok(None))
    }
    pub fn synthesize(
        &mut self,
        text: &str,
        sid: u16,
        generation: &AtomicU64,
        expected: u64,
    ) -> Result<SpeechAudio, AppError> {
        self.used = Instant::now();
        serde_json::to_writer(
            &mut self.input,
            &Request {
                text: text.into(),
                sid,
            },
        )
        .map_err(fail)?;
        self.input.write_all(b"\n").map_err(fail)?;
        self.input.flush().map_err(fail)?;
        let start = Instant::now();
        loop {
            if generation.load(Ordering::SeqCst) != expected {
                return Err(fail("朗读已取消"));
            }
            match self.replies.recv_timeout(Duration::from_millis(50)) {
                Ok(reply) => {
                    return match reply.audio {
                        Some(audio) => Ok(SpeechAudio {
                            audio_data_url: format!("data:audio/wav;base64,{audio}"),
                            voice_name: "Kokoro 离线音色".into(),
                        }),
                        None => Err(fail(reply.error.unwrap_or_else(|| "合成失败".into()))),
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout)
                    if start.elapsed() < Duration::from_secs(120) => {}
                _ => return Err(fail("工作进程超时或退出，请重试")),
            }
        }
    }
}

#[repr(C)]
struct Vits {
    model: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data: *const c_char,
    noise: f32,
    noise_w: f32,
    length: f32,
    dict: *const c_char,
}
#[repr(C)]
struct Matcha {
    model: *const c_char,
    vocoder: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data: *const c_char,
    noise: f32,
    length: f32,
    dict: *const c_char,
}
#[repr(C)]
struct Kokoro {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data: *const c_char,
    length: f32,
    dict: *const c_char,
    lexicon: *const c_char,
    lang: *const c_char,
}
#[repr(C)]
struct Kitten {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data: *const c_char,
    length: f32,
}
#[repr(C)]
struct ZipVoice {
    tokens: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    vocoder: *const c_char,
    data: *const c_char,
    lexicon: *const c_char,
    feat: f32,
    shift: f32,
    rms: f32,
    guidance: f32,
}
#[repr(C)]
struct Pocket {
    files: [*const c_char; 7],
    capacity: i32,
}
#[repr(C)]
struct Supertonic {
    files: [*const c_char; 7],
}
#[repr(C)]
struct Model {
    vits: Vits,
    threads: i32,
    debug: i32,
    provider: *const c_char,
    matcha: Matcha,
    kokoro: Kokoro,
    kitten: Kitten,
    zip: ZipVoice,
    pocket: Pocket,
    supertonic: Supertonic,
}
#[repr(C)]
struct Config {
    model: Model,
    fsts: *const c_char,
    sentences: i32,
    fars: *const c_char,
    silence: f32,
}
#[repr(C)]
struct Audio {
    samples: *const f32,
    n: i32,
    rate: i32,
}
type Create = unsafe extern "C" fn(*const Config) -> *const c_void;
type Destroy = unsafe extern "C" fn(*const c_void);
type Generate = unsafe extern "C" fn(*const c_void, *const c_char, i32, f32) -> *const Audio;
type FreeAudio = unsafe extern "C" fn(*const Audio);
struct Engine {
    _library: libloading::Library,
    handle: *const c_void,
    destroy: Destroy,
    generate: Generate,
    free: FreeAudio,
}
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}
impl Engine {
    fn load(root: &Path, threads: u16) -> Result<Self, AppError> {
        crate::speech_plugin::verified_root(root)?;
        let runtime = root.join("sherpa-onnx-v1.13.8-win-x64-shared-MT-Release/lib");
        // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32:
        // dependencies cannot be resolved from the working directory or arbitrary PATH.
        let library: libloading::Library = unsafe {
            libloading::os::windows::Library::load_with_flags(
                runtime.join("sherpa-onnx-c-api.dll"),
                0x100 | 0x800,
            )
            .map_err(fail)?
            .into()
        };
        let model = root.join("kokoro-int8-multi-lang-v1_1");
        let path =
            |name: &str| CString::new(model.join(name).to_string_lossy().as_bytes()).map_err(fail);
        let model_path = path("model.int8.onnx")?;
        let voices = path("voices.bin")?;
        let tokens = path("tokens.txt")?;
        let data = path("espeak-ng-data")?;
        let lexicon = CString::new(format!(
            "{},{}",
            model.join("lexicon-us-en.txt").display(),
            model.join("lexicon-zh.txt").display()
        ))
        .map_err(fail)?;
        let cpu = CString::new("cpu").unwrap();
        // Null pointers mean unused model types in the upstream C ABI.
        let mut config: Config = unsafe { std::mem::zeroed() };
        config.model.threads = i32::from(threads);
        config.model.provider = cpu.as_ptr();
        config.model.kokoro = Kokoro {
            model: model_path.as_ptr(),
            voices: voices.as_ptr(),
            tokens: tokens.as_ptr(),
            data: data.as_ptr(),
            length: 1.0,
            dict: std::ptr::null(),
            lexicon: lexicon.as_ptr(),
            lang: std::ptr::null(),
        };
        config.sentences = 1;
        config.silence = 0.2;
        unsafe {
            let create: Create = *library.get(b"SherpaOnnxCreateOfflineTts\0").map_err(fail)?;
            let destroy: Destroy = *library
                .get(b"SherpaOnnxDestroyOfflineTts\0")
                .map_err(fail)?;
            let generate: Generate = *library
                .get(b"SherpaOnnxOfflineTtsGenerate\0")
                .map_err(fail)?;
            let free: FreeAudio = *library
                .get(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio\0")
                .map_err(fail)?;
            let handle = create(&config);
            if handle.is_null() {
                return Err(fail("模型初始化失败，请重新安装插件"));
            }
            Ok(Self {
                _library: library,
                handle,
                destroy,
                generate,
                free,
            })
        }
    }
    fn synthesize(&self, request: Request) -> Result<String, AppError> {
        if request.text.trim().is_empty()
            || request.text.chars().count() > 600
            || ![0, 1, 2, 3, 4, 58].contains(&request.sid)
        {
            return Err(fail("合成参数无效"));
        }
        let text = CString::new(request.text).map_err(fail)?;
        unsafe {
            let audio = (self.generate)(self.handle, text.as_ptr(), i32::from(request.sid), 1.0);
            if audio.is_null() {
                return Err(fail("合成失败"));
            }
            let result = if (*audio).n > 0
                && (*audio).n <= 8 * 1024 * 1024
                && !(*audio).samples.is_null()
                && (8000..=48000).contains(&(*audio).rate)
            {
                let samples = std::slice::from_raw_parts((*audio).samples, (*audio).n as usize);
                Ok(STANDARD.encode(wav(samples, (*audio).rate as u32)))
            } else {
                Err(fail("音频输出无效"))
            };
            (self.free)(audio);
            result
        }
    }
}
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let length = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(length as usize + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(length + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&length.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    bytes
}
pub fn run(root: &Path, threads: u16) -> Result<(), AppError> {
    if !(1..=8).contains(&threads) {
        return Err(fail("线程数无效"));
    }
    let engine = Engine::load(root, threads)?;
    let (sender, receiver) = mpsc::sync_channel(2);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if line.len() > 8192 || sender.send(line).is_err() {
                break;
            }
        }
    });
    // EOF on parent exit and idle timeout both unload the model; no polling loop.
    while let Ok(line) = receiver.recv_timeout(Duration::from_secs(300)) {
        let result = serde_json::from_str::<Request>(&line)
            .map_err(fail)
            .and_then(|request| engine.synthesize(request));
        let reply = match result {
            Ok(audio) => Reply {
                audio: Some(audio),
                error: None,
            },
            Err(error) => Reply {
                audio: None,
                error: Some(error.user_message()),
            },
        };
        let mut output = std::io::stdout().lock();
        serde_json::to_writer(&mut output, &reply).map_err(fail)?;
        output.write_all(b"\n").map_err(fail)?;
        output.flush().map_err(fail)?;
    }
    Ok(())
}
