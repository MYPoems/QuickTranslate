use super::types::{Language, TranslationRequest};

/// A lone English word going into Chinese is answered as a dictionary entry rather than a
/// plain translation.
pub fn is_single_english_word(request: &TranslationRequest) -> bool {
    request.source_language == Language::English
        && request.target_language == Language::Chinese
        && request.text.len() <= 64
        && !request.text.is_empty()
        && request.text.chars().all(|character| {
            character.is_ascii_alphabetic() || character == '-' || character == '\''
        })
}

pub fn build_system_prompt(request: &TranslationRequest) -> String {
    let mut prompt = if is_single_english_word(request) {
        concat!(
            "You are a concise English-Chinese dictionary. Return only valid JSON with keys: ",
            "translation (string), phonetic (string or null), part_of_speech (string or null), ",
            "definitions (array of Chinese strings), example (string or null). No markdown."
        )
        .to_string()
    } else {
        format!(
            "Translate from {} to {}. Return only the natural, accurate translation. No label, explanation, or markdown.",
            request.source_language.display_name(),
            request.target_language.display_name()
        )
    };
    if !request.glossary.is_empty() {
        prompt.push_str(" Use these translations for the listed terms:");
        for (source, target) in &request.glossary {
            prompt.push_str(&format!(" \"{source}\" => \"{target}\";"));
        }
    }
    let instructions = request.instructions.trim();
    if !instructions.is_empty() {
        // The user's wording is added after the fixed rules, which stay in force.
        prompt.push_str(
            " Additional requirements from the user, which never change the output format: ",
        );
        prompt.push_str(instructions);
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(text: &str) -> TranslationRequest {
        TranslationRequest::new(text, Language::English, Language::Chinese)
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

    #[test]
    fn a_word_is_a_dictionary_lookup_only_from_english_into_chinese() {
        assert!(is_single_english_word(&request("architecture")));
        let into_japanese =
            TranslationRequest::new("architecture", Language::English, Language::Japanese);
        assert!(!is_single_english_word(&into_japanese));
        assert!(build_system_prompt(&into_japanese).contains("English to Japanese"));
    }

    #[test]
    fn glossary_and_instructions_follow_the_fixed_rules() {
        let mut request = request("Deploy the pipeline.");
        request.glossary = vec![("pipeline".into(), "流水线".into())];
        request.instructions = "  Keep a formal tone.  ".into();
        let prompt = build_system_prompt(&request);
        let rules = prompt.find("Return only").unwrap();
        let glossary = prompt.find("\"pipeline\" => \"流水线\"").unwrap();
        let instructions = prompt.find("Keep a formal tone.").unwrap();
        assert!(rules < glossary && glossary < instructions);

        let plain = build_system_prompt(&super::TranslationRequest::new(
            "Deploy the pipeline.",
            Language::English,
            Language::Chinese,
        ));
        assert!(!plain.contains("Additional requirements"));
        assert!(!plain.contains("listed terms"));
    }
}
