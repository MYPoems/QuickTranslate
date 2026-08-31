use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum AppError {
    #[error("no selected text")]
    NoSelectedText,
    #[error("clipboard operation failed: {0}")]
    Clipboard(String),
    #[error("selected text exceeds {0} characters")]
    TextTooLong(usize),
    #[error("provider is not configured")]
    ProviderNotConfigured,
    #[error("invalid API key")]
    InvalidApiKey,
    #[error("network request failed: {0}")]
    Network(String),
    #[error("provider request failed: {0}")]
    Provider(String),
    #[error("database operation failed: {0}")]
    Database(String),
    #[error("settings operation failed: {0}")]
    Settings(String),
    #[error("global shortcut is invalid: {0}")]
    InvalidShortcut(String),
    #[error("autostart operation failed: {0}")]
    AutoStart(String),
    #[cfg_attr(windows, allow(dead_code))]
    #[error("this platform is not supported yet")]
    UnsupportedPlatform,
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSelectedText => "NO_SELECTED_TEXT",
            Self::Clipboard(_) => "CLIPBOARD_ERROR",
            Self::TextTooLong(_) => "TEXT_TOO_LONG",
            Self::ProviderNotConfigured => "PROVIDER_NOT_CONFIGURED",
            Self::InvalidApiKey => "INVALID_API_KEY",
            Self::Network(_) => "NETWORK_ERROR",
            Self::Provider(_) => "PROVIDER_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Settings(_) => "SETTINGS_ERROR",
            Self::InvalidShortcut(_) => "INVALID_SHORTCUT",
            Self::AutoStart(_) => "AUTOSTART_ERROR",
            Self::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::NoSelectedText => "未检测到选中文字".into(),
            Self::Clipboard(_) => "无法读取当前选中文字".into(),
            Self::TextTooLong(limit) => format!("选中文本过长（最多 {limit} 个字符）"),
            Self::ProviderNotConfigured => "请先在设置中配置 API Key".into(),
            Self::InvalidApiKey => "API Key 无效，请检查设置".into(),
            Self::Network(_) => "网络连接失败".into(),
            Self::Provider(message) => message.clone(),
            Self::Database(_) => "本地缓存暂时不可用".into(),
            Self::Settings(_) => "设置保存失败".into(),
            Self::InvalidShortcut(_) => "全局快捷键格式无效或已被占用".into(),
            Self::AutoStart(_) => "无法更新开机启动设置".into(),
            Self::UnsupportedPlatform => "当前平台暂不支持获取选中文字".into(),
            Self::Internal(_) => "发生内部错误，请稍后重试".into(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("AppError", 2)?;
        value.serialize_field("code", self.code())?;
        value.serialize_field("message", &self.user_message())?;
        value.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}
