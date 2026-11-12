use serde::{Deserialize, Serialize};

/// The languages text can be translated between. The serialized names (`"chinese"`,
/// `"english"`, ...) are stored in history, so existing ones must keep their spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Chinese,
    English,
    Japanese,
    Korean,
    French,
    German,
    Spanish,
    Russian,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Chinese => "zh",
            Self::English => "en",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Russian => "ru",
        }
    }

    /// The name used when telling the model which languages to translate between.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Chinese => "Chinese",
            Self::English => "English",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::French => "French",
            Self::German => "German",
            Self::Spanish => "Spanish",
            Self::Russian => "Russian",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranslationRequest {
    pub text: String,
    pub source_language: Language,
    pub target_language: Language,
    /// Terms the user wants translated a particular way, as (source, translation) pairs.
    /// Only the ones that occur in `text` are passed along.
    pub glossary: Vec<(String, String)>,
    /// Extra wording the user wants added to the translation instructions.
    pub instructions: String,
}

impl TranslationRequest {
    pub fn new(
        text: impl Into<String>,
        source_language: Language,
        target_language: Language,
    ) -> Self {
        Self {
            text: text.into(),
            source_language,
            target_language,
            glossary: Vec::new(),
            instructions: String::new(),
        }
    }
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
