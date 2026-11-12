use super::types::Language;

/// Guesses the language of a piece of text from its script and, for Latin script, from a
/// few very common words. It decides the translation direction, so it only has to tell the
/// supported languages apart; anything it cannot place is treated as English.
pub fn detect_language(text: &str) -> Language {
    let mut han = 0usize;
    let mut kana = 0usize;
    let mut hangul = 0usize;
    let mut cyrillic = 0usize;
    let mut latin = 0usize;

    for character in text.chars() {
        match character as u32 {
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D => kana += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => hangul += 1,
            0x0400..=0x04FF => cyrillic += 1,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F => han += 1,
            _ if character.is_alphabetic() && (character as u32) < 0x0250 => latin += 1,
            _ => {}
        }
    }

    // Japanese is written with Chinese characters too; kana is what gives it away, and even
    // a little of it does.
    if kana > 0 && (kana + han) * 2 >= latin {
        return Language::Japanese;
    }
    if hangul > 0 && hangul * 2 >= latin {
        return Language::Korean;
    }
    if cyrillic > 0 && cyrillic * 2 >= latin {
        return Language::Russian;
    }
    // Chinese technical prose often contains long English identifiers. Two or more Chinese
    // characters are therefore a stronger signal than a raw 1:1 count.
    let chinese_dominant = han > 0 && han * 2 >= latin;
    let chinese_mixed_prose = han >= 2 && han * 14 >= latin;
    if chinese_dominant || chinese_mixed_prose {
        return Language::Chinese;
    }
    latin_language(text)
}

/// Tells French, German and Spanish from English by their most frequent short words. A
/// single hit is not enough: "die", "in", "son" and "a" are words in more than one of them.
fn latin_language(text: &str) -> Language {
    const WORDS: [(Language, &[&str]); 4] = [
        (
            Language::English,
            &[
                "the", "and", "is", "are", "of", "to", "in", "that", "it", "for", "with", "this",
                "you", "not", "was", "have",
            ],
        ),
        (
            Language::French,
            &[
                "le", "la", "les", "des", "est", "et", "une", "un", "que", "pour", "dans", "pas",
                "nous", "vous", "avec", "sur", "ce", "cette", "sont", "du",
            ],
        ),
        (
            Language::German,
            &[
                "der", "die", "das", "und", "ist", "nicht", "ein", "eine", "ich", "mit", "für",
                "auf", "den", "dem", "sich", "auch", "wir", "sie", "zu", "von",
            ],
        ),
        (
            Language::Spanish,
            &[
                "el", "los", "las", "es", "y", "una", "que", "por", "para", "con", "no", "está",
                "como", "pero", "muy", "del", "sus", "este", "esta", "más",
            ],
        ),
    ];
    let lowered = text.to_lowercase();
    let tokens: Vec<&str> = lowered
        .split(|character: char| !character.is_alphabetic())
        .filter(|token| !token.is_empty())
        .collect();
    let mut best = (Language::English, 0usize);
    let mut english = 0usize;
    for (language, words) in WORDS {
        let hits = tokens.iter().filter(|token| words.contains(token)).count();
        if language == Language::English {
            english = hits;
        } else if hits > best.1 {
            best = (language, hits);
        }
    }
    if best.1 >= 2 && best.1 > english {
        best.0
    } else {
        Language::English
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_english_samples() {
        for input in ["hello", "architecture", "Hello world."] {
            assert_eq!(detect_language(input), Language::English);
        }
    }

    #[test]
    fn detects_chinese_samples() {
        for input in [
            "你好",
            "这是一个测试。",
            "Transformer 是一种 neural network architecture.",
        ] {
            assert_eq!(detect_language(input), Language::Chinese);
        }
    }

    #[test]
    fn defaults_empty_input_to_english() {
        assert_eq!(detect_language(""), Language::English);
    }

    #[test]
    fn a_single_quoted_chinese_character_does_not_flip_english() {
        assert_eq!(
            detect_language("The Chinese character 中 is common."),
            Language::English
        );
    }

    #[test]
    fn japanese_is_not_mistaken_for_chinese() {
        for input in [
            "これは日本語の文章です。",
            "東京都の人口は増加している。",
            "カタカナ",
            "新しいAPIを使ってください。",
        ] {
            assert_eq!(detect_language(input), Language::Japanese, "{input}");
        }
        // No kana at all: indistinguishable from Chinese by script, and treated as such.
        assert_eq!(detect_language("東京都"), Language::Chinese);
    }

    #[test]
    fn detects_korean_and_russian_by_script() {
        assert_eq!(detect_language("안녕하세요, 반갑습니다."), Language::Korean);
        assert_eq!(detect_language("Привет, как дела?"), Language::Russian);
        assert_eq!(
            detect_language("The word 안녕 means hello in a long English sentence about Korean."),
            Language::English
        );
    }

    #[test]
    fn tells_latin_script_languages_apart_by_common_words() {
        assert_eq!(
            detect_language("Le chat est sur la table et il dort dans la cuisine."),
            Language::French
        );
        assert_eq!(
            detect_language("Der Hund ist nicht im Garten, er schläft auf dem Sofa."),
            Language::German
        );
        assert_eq!(
            detect_language("El perro no está en el jardín, pero los gatos sí."),
            Language::Spanish
        );
        // One shared word is not evidence.
        assert_eq!(detect_language("We die in the end."), Language::English);
        assert_eq!(detect_language("déjà vu"), Language::English);
    }
}
