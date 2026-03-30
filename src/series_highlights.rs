//! Document highlight support for quilt series files.
//!
//! When the cursor is on a patch name, highlights all entries with the
//! same name. This is useful for spotting duplicate entries.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get document highlights at the given position in a series file.
pub fn get_series_highlights(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
) -> Vec<DocumentHighlight> {
    let Some(offset) = try_position_to_offset(source_text, position) else {
        return Vec::new();
    };

    // Find which patch entry the cursor is on
    let target_name = series
        .patch_entries()
        .find(|entry| {
            let range = entry.syntax().text_range();
            range.start() <= offset && offset < range.end()
        })
        .and_then(|entry| entry.name());

    let Some(target_name) = target_name else {
        return Vec::new();
    };

    // Highlight all entries with the same name
    series
        .patch_entries()
        .filter_map(|entry| {
            let name = entry.name()?;
            if name != target_name {
                return None;
            }
            let token = entry.name_token()?;
            let range = text_range_to_lsp_range(source_text, token.text_range());
            Some(DocumentHighlight {
                range,
                kind: Some(DocumentHighlightKind::READ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn get_highlights(text: &str, line: u32, col: u32) -> Vec<DocumentHighlight> {
        let parsed = parse_series(text);
        let series = parsed.tree();
        get_series_highlights(&series, text, Position::new(line, col))
    }

    #[test]
    fn test_highlight_single_entry() {
        let text = "a.patch\nb.patch\n";
        let highlights = get_highlights(text, 0, 2);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].range.start.line, 0);
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::READ));
    }

    #[test]
    fn test_highlight_duplicate_entries() {
        let text = "a.patch\nb.patch\na.patch\n";
        let highlights = get_highlights(text, 0, 2);
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0].range.start.line, 0);
        assert_eq!(highlights[1].range.start.line, 2);
    }

    #[test]
    fn test_highlight_from_second_duplicate() {
        let text = "a.patch\nb.patch\na.patch\n";
        // Cursor on the second a.patch
        let highlights = get_highlights(text, 2, 2);
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0].range.start.line, 0);
        assert_eq!(highlights[1].range.start.line, 2);
    }

    #[test]
    fn test_no_highlight_on_comment() {
        let text = "# comment\na.patch\n";
        let highlights = get_highlights(text, 0, 2);
        assert_eq!(highlights.len(), 0);
    }

    #[test]
    fn test_no_highlight_on_empty() {
        let text = "";
        let highlights = get_highlights(text, 0, 0);
        assert_eq!(highlights.len(), 0);
    }

    #[test]
    fn test_unique_entries_single_highlight() {
        let text = "a.patch\nb.patch\nc.patch\n";
        let highlights = get_highlights(text, 1, 2);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].range.start.line, 1);
    }

    #[test]
    fn test_highlight_range_covers_name_only() {
        let text = "a.patch -p1\n";
        let highlights = get_highlights(text, 0, 2);
        assert_eq!(highlights.len(), 1);
        // Should highlight just "a.patch", not the options
        assert_eq!(highlights[0].range.start.character, 0);
        assert_eq!(highlights[0].range.end.character, 7); // "a.patch" = 7 chars
    }

    #[test]
    fn test_highlight_with_comments_between() {
        let text = "a.patch\n# section\na.patch\n";
        let highlights = get_highlights(text, 0, 2);
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0].range.start.line, 0);
        assert_eq!(highlights[1].range.start.line, 2);
    }

    #[test]
    fn test_three_duplicates() {
        let text = "a.patch\na.patch\na.patch\n";
        let highlights = get_highlights(text, 1, 2);
        assert_eq!(highlights.len(), 3);
        assert_eq!(highlights[0].range.start.line, 0);
        assert_eq!(highlights[1].range.start.line, 1);
        assert_eq!(highlights[2].range.start.line, 2);
    }
}
