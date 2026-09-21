use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::errors::AppError;

const PLUGIN_VERSION: &str = "PP-OCRv6 Small";
const DETECTION_FILE: &str = "PP-OCRv6_small_det.tar";
const RECOGNITION_FILE: &str = "PP-OCRv6_small_rec.tar";
const DETECTION_URL: &str = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_small_det_onnx_infer.tar";
const RECOGNITION_URL: &str = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_small_rec_onnx_infer.tar";
const DETECTION_SHA256: &str = "D218F6FBF0F1C23D2161BD6AC7F5EAA6104FA89955C09290497E31008E2618E4";
const RECOGNITION_SHA256: &str = "D267AB077A44A0EEDB1EA8F8C542D263F211DE8E9D7A029BF9FCFFF7E5A88FB1";
const DETECTION_BYTES: u64 = 9_891_840;
const RECOGNITION_BYTES: u64 = 21_319_680;
const MAX_MODEL_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaddleOcrPluginStatus {
    pub installed: bool,
    pub version: &'static str,
    pub installed_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct PaddleOcrPluginPaths {
    pub detection: PathBuf,
    pub recognition: PathBuf,
}

pub fn plugin_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("ocr-plugins").join("ppocrv6-small")
}

pub fn status(plugin_dir: &Path) -> PaddleOcrPluginStatus {
    let paths = model_paths(plugin_dir);
    let installed = verify_file(&paths.detection, DETECTION_BYTES, DETECTION_SHA256)
        && verify_file(&paths.recognition, RECOGNITION_BYTES, RECOGNITION_SHA256);
    PaddleOcrPluginStatus {
        installed,
        version: PLUGIN_VERSION,
        installed_bytes: if installed {
            DETECTION_BYTES + RECOGNITION_BYTES
        } else {
            0
        },
        download_bytes: DETECTION_BYTES + RECOGNITION_BYTES,
    }
}

pub fn verified_paths(plugin_dir: &Path) -> Result<PaddleOcrPluginPaths, AppError> {
    if !status(plugin_dir).installed {
        return Err(AppError::Ocr(
            "请先在设置中安装 PP-OCRv6 Small 高精度插件".into(),
        ));
    }
    Ok(model_paths(plugin_dir))
}

pub async fn install(plugin_dir: &Path) -> Result<PaddleOcrPluginStatus, AppError> {
    if status(plugin_dir).installed {
        return Ok(status(plugin_dir));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .user_agent(concat!("QuickTranslate/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AppError::Ocr(format!("无法初始化插件下载：{error}")))?;
    let detection =
        download_verified(&client, DETECTION_URL, DETECTION_BYTES, DETECTION_SHA256).await?;
    let recognition = download_verified(
        &client,
        RECOGNITION_URL,
        RECOGNITION_BYTES,
        RECOGNITION_SHA256,
    )
    .await?;

    fs::create_dir_all(plugin_dir)
        .map_err(|error| AppError::Ocr(format!("无法创建插件目录：{error}")))?;
    let paths = model_paths(plugin_dir);
    write_atomically(&paths.detection, &detection)?;
    if let Err(error) = write_atomically(&paths.recognition, &recognition) {
        let _ = fs::remove_file(&paths.detection);
        return Err(error);
    }
    let installed = status(plugin_dir);
    if !installed.installed {
        let _ = fs::remove_file(&paths.detection);
        let _ = fs::remove_file(&paths.recognition);
        return Err(AppError::Ocr("插件安装后的完整性校验失败".into()));
    }
    Ok(installed)
}

pub fn uninstall(plugin_dir: &Path) -> Result<PaddleOcrPluginStatus, AppError> {
    if plugin_dir.exists() {
        fs::remove_dir_all(plugin_dir)
            .map_err(|error| AppError::Ocr(format!("无法删除 OCR 插件：{error}")))?;
    }
    Ok(status(plugin_dir))
}

fn model_paths(plugin_dir: &Path) -> PaddleOcrPluginPaths {
    PaddleOcrPluginPaths {
        detection: plugin_dir.join(DETECTION_FILE),
        recognition: plugin_dir.join(RECOGNITION_FILE),
    }
}

async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    expected_bytes: u64,
    expected_hash: &str,
) -> Result<Vec<u8>, AppError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::Ocr(format!("插件下载失败：{error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Ocr(format!(
            "插件下载失败：HTTP {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODEL_BYTES as u64)
    {
        return Err(AppError::Ocr("插件文件超过安全大小限制".into()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Ocr(format!("读取插件下载内容失败：{error}")))?;
    if bytes.len() > MAX_MODEL_BYTES
        || bytes.len() as u64 != expected_bytes
        || sha256(bytes.as_ref()) != expected_hash
    {
        return Err(AppError::Ocr("插件下载完整性校验失败".into()));
    }
    Ok(bytes.to_vec())
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let temp = path.with_extension("download");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|error| AppError::Ocr(format!("无法写入插件：{error}")))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| AppError::Ocr(format!("无法保存插件：{error}")))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| AppError::Ocr(format!("无法替换旧插件：{error}")))?;
    }
    fs::rename(&temp, path).map_err(|error| AppError::Ocr(format!("无法安装插件：{error}")))
}

fn verify_file(path: &Path, expected_bytes: u64, expected_hash: &str) -> bool {
    fs::metadata(path).is_ok_and(|value| value.len() == expected_bytes)
        && fs::read(path).is_ok_and(|bytes| sha256(&bytes) == expected_hash)
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode_upper(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_plugin_is_not_installed() {
        let path = std::env::temp_dir().join(format!(
            "quicktranslate-missing-plugin-{}",
            std::process::id()
        ));
        assert!(!status(&path).installed);
        assert_eq!(status(&path).download_bytes, 31_211_520);
    }

    #[test]
    fn hashes_are_uppercase_sha256() {
        assert_eq!(
            sha256(b"QuickTranslate"),
            "BE50E6ABE765AF4D76EEAB5C35A0679E72CD3D7ED74ACC7DA5444E9D9A99A91D"
        );
    }
}
