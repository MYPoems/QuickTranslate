use super::types::{Language, TranslationRequest};

pub fn is_single_english_word(request: &TranslationRequest) -> bool {
    request.source_language == Language::English
        && request.text.len() <= 64
        && !request.text.is_empty()
        && request.text.chars().all(|character| {
            character.is_ascii_alphabetic() || character == '-' || character == '\''
        })
}

pub fn build_system_prompt(request: &TranslationRequest) -> String {
    if is_single_english_word(request) {
        return concat!(
            "You are a concise English-Chinese dictionary. Return only valid JSON with keys: ",
            "translation (string), phonetic (string or null), part_of_speech (string or null), ",
            "definitions (array of Chinese strings), example (string or null). No markdown."
        )
        .into();
    }

    format!(
        "Translate from {} to {}. Return only the natural, accurate translation. No label, explanation, or markdown.",
        request.source_language.display_name(),
        request.target_language.display_name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> TranslationRequest {
        TranslationRequest {
            text: text.into(),
            source_language: Language::English,
            target_language: Language::Chinese,
        }
    }

    #[test]
    fn word_prompt_requests_json() {
        assert!(build_system_prompt(&request("architecture")).contains("valid JSON"));
    }

    #[test]
    fn sentence_prompt_requests_plain_translation() {
        let prompt = build_system_prompt(&request("Hello world."));
        assert!(prompt.contains("English to Chinese"));
        assert!(!prompt.contains("valid JSON"));
    }
}
