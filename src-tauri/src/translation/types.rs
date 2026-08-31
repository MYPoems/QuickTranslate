use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Chinese,
    English,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Chinese => "zh",
            Self::English => "en",
        }
    }

    pub fn target(self) -> Self {
        match self {
            Self::Chinese => Self::English,
            Self::English => Self::Chinese,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Chinese => "Chinese",
            Self::English => "English",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranslationRequest {
    pub text: String,
    pub source_language: Language,
    pub target_language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub source_text: String,
    pub translation: String,
    pub detected_language: Language,
    pub target_language: Language,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phonetic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_of_speech: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}
