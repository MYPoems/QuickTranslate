use serde::{Deserialize, Serialize};

use super::types::Language;
use crate::errors::AppError;

pub const MAX_GLOSSARY_ENTRIES: usize = 200;
pub const MAX_GLOSSARY_TERM_CHARS: usize = 80;
pub const MAX_INSTRUCTION_CHARS: usize = 600;

/// A term and the translation the user wants for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct GlossaryEntry {
    pub source: String,
    pub target: String,
}

/// How the user wants translations done: between which two languages, and with which
/// terms and extra requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationPreferences {
    /// The language the user reads. Anything else is translated into it.
    pub primary_language: Language,
    /// What text in the primary language is translated into.
    pub secondary_language: Language,
    pub glossary: Vec<GlossaryEntry>,
    pub instructions: String,
}

impl Default for TranslationPreferences {
    fn default() -> Self {
        Self {
            primary_language: Language::Chinese,
            secondary_language: Language::English,
            glossary: Vec::new(),
            instructions: String::new(),
        }
    }
}

impl TranslationPreferences {
    /// Checks what the settings form sent and returns it tidied: trimmed, without empty or
    /// repeated glossary lines.
    pub fn validated(&self) -> Result<Self, AppError> {
        if self.primary_language == self.secondary_language {
            return Err(AppError::invalid(
                "secondaryLanguage",
                "两种语言不能相同：一种是你阅读的语言，另一种是它要译成的语言",
            ));
        }
        let instructions = self.instructions.trim().to_string();
        if instructions.chars().count() > MAX_INSTRUCTION_CHARS {
            return Err(AppError::invalid(
                "translationInstructions",
                format!("附加翻译要求最多 {MAX_INSTRUCTION_CHARS} 个字符"),
            ));
        }
        let mut glossary: Vec<GlossaryEntry> = Vec::new();
        for entry in &self.glossary {
            let source = entry.source.trim();
            let target = entry.target.trim();
            if source.is_empty() && target.is_empty() {
                continue;
            }
            if source.is_empty() || target.is_empty() {
                return Err(AppError::invalid(
                    "glossary",
                    "术语表每行需要同时有原文和译文，格式为“原文 = 译文”",
                ));
            }
            if source.chars().count() > MAX_GLOSSARY_TERM_CHARS
                || target.chars().count() > MAX_GLOSSARY_TERM_CHARS
            {
                return Err(AppError::invalid(
                    "glossary",
                    format!("术语和译文各自最多 {MAX_GLOSSARY_TERM_CHARS} 个字符"),
                ));
            }
            // A later line for the same term replaces the earlier one.
            glossary.retain(|existing| !existing.source.eq_ignore_ascii_case(source));
            glossary.push(GlossaryEntry {
                source: source.to_string(),
                target: target.to_string(),
            });
        }
        if glossary.len() > MAX_GLOSSARY_ENTRIES {
            return Err(AppError::invalid(
                "glossary",
                format!("术语表最多 {MAX_GLOSSARY_ENTRIES} 条"),
            ));
        }
        Ok(Self {
            primary_language: self.primary_language,
            secondary_language: self.secondary_language,
            glossary,
            instructions,
        })
    }

    /// Which language text in `source` is translated into. An explicit `requested` target
    /// wins unless it is the source language itself.
    pub fn target_for(&self, source: Language, requested: Option<Language>) -> Language {
        match requested {
            Some(target) if target != source => target,
            _ if source == self.primary_language => self.secondary_language,
            _ => self.primary_language,
        }
    }

    /// The glossary entries whose term occurs in `text`. Sending only those keeps the
    /// instructions short however long the glossary grows.
    pub fn glossary_for(&self, text: &str) -> Vec<(String, String)> {
        let lowered = text.to_lowercase();
        self.glossary
            .iter()
            .filter(|entry| lowered.contains(&entry.source.to_lowercase()))
            .map(|entry| (entry.source.clone(), entry.target.clone()))
            .collect()
    }

    /// Everything here that changes what a translation of some text looks like, as one
    /// string for the cache key: a result made under other terms or requirements must not
    /// be served for this one.
    pub fn cache_context(&self, glossary: &[(String, String)]) -> String {
        let mut context = String::new();
        for (source, target) in glossary {
            context.push_str(source);
            context.push('\u{1f}');
            context.push_str(target);
            context.push('\u{1e}');
        }
        context.push('\u{1d}');
        context.push_str(self.instructions.trim());
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, target: &str) -> GlossaryEntry {
        GlossaryEntry {
            source: source.into(),
            target: target.into(),
        }
    }

    #[test]
    fn direction_follows_the_two_languages_unless_one_is_asked_for() {
        let preferences = TranslationPreferences::default();
        assert_eq!(
            preferences.target_for(Language::English, None),
            Language::Chinese
        );
        assert_eq!(
            preferences.target_for(Language::Chinese, None),
            Language::English
        );
        assert_eq!(
            preferences.target_for(Language::Japanese, None),
            Language::Chinese
        );
        assert_eq!(
            preferences.target_for(Language::English, Some(Language::Japanese)),
            Language::Japanese
        );
        // Asking for the language the text is already in changes nothing.
        assert_eq!(
            preferences.target_for(Language::English, Some(Language::English)),
            Language::Chinese
        );

        let reader_of_english = TranslationPreferences {
            primary_language: Language::English,
            secondary_language: Language::French,
            ..Default::default()
        };
        assert_eq!(
            reader_of_english.target_for(Language::Chinese, None),
            Language::English
        );
        assert_eq!(
            reader_of_english.target_for(Language::English, None),
            Language::French
        );
    }

    #[test]
    fn validation_tidies_the_glossary_and_rejects_what_cannot_be_used() {
        let tidy = TranslationPreferences {
            glossary: vec![
                entry(" pipeline ", " 流水线 "),
                entry("", ""),
                entry("Pipeline", "管线"),
                entry("token", "令牌"),
            ],
            instructions: "  正式语气  ".into(),
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!(
            tidy.glossary,
            vec![entry("Pipeline", "管线"), entry("token", "令牌")]
        );
        assert_eq!(tidy.instructions, "正式语气");

        let invalid = |preferences: TranslationPreferences| match preferences.validated() {
            Err(AppError::InvalidSetting { field, .. }) => field,
            other => panic!("expected a rejected setting, got {other:?}"),
        };
        assert_eq!(
            invalid(TranslationPreferences {
                secondary_language: Language::Chinese,
                ..Default::default()
            }),
            "secondaryLanguage"
        );
        assert_eq!(
            invalid(TranslationPreferences {
                glossary: vec![entry("term", "")],
                ..Default::default()
            }),
            "glossary"
        );
        assert_eq!(
            invalid(TranslationPreferences {
                glossary: (0..=MAX_GLOSSARY_ENTRIES)
                    .map(|index| entry(&format!("term{index}"), "译"))
                    .collect(),
                ..Default::default()
            }),
            "glossary"
        );
        assert_eq!(
            invalid(TranslationPreferences {
                instructions: "长".repeat(MAX_INSTRUCTION_CHARS + 1),
                ..Default::default()
            }),
            "translationInstructions"
        );
    }

    #[test]
    fn only_terms_in_the_text_are_sent_and_they_shape_the_cache_key() {
        let preferences = TranslationPreferences {
            glossary: vec![entry("pipeline", "流水线"), entry("token", "令牌")],
            instructions: "正式语气".into(),
            ..Default::default()
        };
        let used = preferences.glossary_for("Deploy the Pipeline today.");
        assert_eq!(used, vec![("pipeline".to_string(), "流水线".to_string())]);
        assert!(preferences.glossary_for("Nothing relevant.").is_empty());

        let with_term = preferences.cache_context(&used);
        let without_term = preferences.cache_context(&[]);
        assert_ne!(with_term, without_term);
        assert_ne!(
            without_term,
            TranslationPreferences::default().cache_context(&[]),
            "the extra requirements count too"
        );
        // Old settings files have none of this and deserialize to the defaults.
        let old: TranslationPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(old, TranslationPreferences::default());
    }
}
