use super::types::Language;

pub fn detect_language(text: &str) -> Language {
    let mut chinese = 0usize;
    let mut english = 0usize;

    for character in text.chars() {
        if is_cjk(character) {
            chinese += 1;
        } else if character.is_ascii_alphabetic() {
            english += 1;
        }
    }

    // Chinese technical prose often contains long English identifiers. Two or
    // more CJK characters are therefore a stronger signal than a raw 1:1 count.
    let chinese_dominant = chinese > 0 && chinese * 2 >= english;
    let chinese_mixed_prose = chinese >= 2 && chinese * 14 >= english;
    if chinese_dominant || chinese_mixed_prose {
        Language::Chinese
    } else {
        Language::English
    }
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F
    )
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
}
