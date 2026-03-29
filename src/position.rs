//! UTF-16 ↔ byte offset position conversion utilities.

use text_size::{TextRange, TextSize};
use tower_lsp_server::ls_types::{Position, Range};

/// Convert a byte offset to an LSP Position (line, UTF-16 column).
pub fn offset_to_position(text: &str, offset: TextSize) -> Position {
    let mut line = 0u32;
    let mut utf16_col = 0u32;

    for (i, ch) in text.char_indices() {
        let current_offset = TextSize::try_from(i).unwrap();

        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            utf16_col = 0;
        } else {
            utf16_col += ch.len_utf16() as u32;
        }
    }

    Position {
        line,
        character: utf16_col,
    }
}

/// Convert a TextRange to an LSP Range.
pub fn text_range_to_lsp_range(text: &str, range: TextRange) -> Range {
    Range {
        start: offset_to_position(text, range.start()),
        end: offset_to_position(text, range.end()),
    }
}

/// Convert an LSP Position to a byte offset, returning None if out of bounds.
pub fn try_position_to_offset(text: &str, position: Position) -> Option<TextSize> {
    let mut line = 0u32;
    let mut line_start = 0usize;

    for (i, ch) in text.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }

    if line < position.line {
        return None;
    }

    let mut utf16_col = 0u32;
    for (i, ch) in text[line_start..].char_indices() {
        if utf16_col >= position.character {
            return TextSize::try_from(line_start + i).ok();
        }
        if ch == '\n' {
            break;
        }
        utf16_col += ch.len_utf16() as u32;
    }

    if utf16_col >= position.character {
        let line_end = text[line_start..]
            .find('\n')
            .map(|rel| line_start + rel)
            .unwrap_or(text.len());
        return TextSize::try_from(line_end).ok();
    }

    None
}

/// Convert an LSP Range to a TextRange, returning None if out of bounds.
pub fn try_lsp_range_to_text_range(text: &str, range: &Range) -> Option<TextRange> {
    let start = try_position_to_offset(text, range.start)?;
    let end = try_position_to_offset(text, range.end)?;
    Some(TextRange::new(start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_position_simple() {
        let text = "first\nsecond\nthird\n";
        let pos = offset_to_position(text, TextSize::from(0));
        assert_eq!(pos, Position::new(0, 0));

        let pos = offset_to_position(text, TextSize::from(6));
        assert_eq!(pos, Position::new(1, 0));

        let pos = offset_to_position(text, TextSize::from(8));
        assert_eq!(pos, Position::new(1, 2));
    }

    #[test]
    fn test_roundtrip() {
        let text = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        for offset in 0..text.len() {
            let ts = TextSize::from(offset as u32);
            let pos = offset_to_position(text, ts);
            let back = try_position_to_offset(text, pos).unwrap();
            assert_eq!(ts, back, "roundtrip failed for offset {offset}");
        }
    }

    #[test]
    fn test_utf16_multibyte() {
        // '😀' is 4 bytes in UTF-8 but 2 UTF-16 code units
        let text = "a😀b\n";
        let pos = offset_to_position(text, TextSize::from(5)); // byte offset of 'b'
        assert_eq!(pos, Position::new(0, 3)); // a(1) + 😀(2) = 3 UTF-16 units
    }
}
