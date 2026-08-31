use crate::errors::AppError;

pub const MAX_TEXT_CHARS: usize = 5_000;

pub fn normalize_text(text: &str) -> Result<String, AppError> {
    let normalized_newlines = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized_newlines.len());

    for (line_index, line) in normalized_newlines.lines().enumerate() {
        if line_index > 0 {
            output.push('\n');
        }
        let mut previous_was_space = false;
        for character in line.trim().chars() {
            if character == ' ' || character == '\t' {
                if !previous_was_space {
                    output.push(' ');
                    previous_was_space = true;
                }
            } else {
                output.push(character);
                previous_was_space = false;
            }
        }
    }

    let output = output.trim().to_string();
    if output.is_empty() {
        return Err(AppError::NoSelectedText);
    }
    if output.chars().count() > MAX_TEXT_CHARS {
        return Err(AppError::TextTooLong(MAX_TEXT_CHARS));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spaces_and_newlines() {
        assert_eq!(
            normalize_text("  Hello   world\r\n next\tline  ").unwrap(),
            "Hello world\nnext line"
        );
    }

    #[test]
    fn rejects_blank_input() {
        assert!(matches!(
            normalize_text(" \n\t "),
            Err(AppError::NoSelectedText)
        ));
    }

    #[test]
    fn rejects_oversized_input() {
        let input = "a".repeat(MAX_TEXT_CHARS + 1);
        assert!(matches!(
            normalize_text(&input),
            Err(AppError::TextTooLong(MAX_TEXT_CHARS))
        ));
    }
}
