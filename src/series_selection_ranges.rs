//! Smart selection range expansion for quilt series files.
//!
//! Expands selection: patch name token → entry (with options/newline) → whole file.

use patchkit::edit::series::lex::SyntaxKind;
use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get selection ranges for the given positions in a series file.
pub fn get_series_selection_ranges(
    series: &SeriesFile,
    source_text: &str,
    positions: &[Position],
) -> Vec<SelectionRange> {
    positions
        .iter()
        .filter_map(|pos| selection_range_at(series, source_text, *pos))
        .collect()
}

fn selection_range_at(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
) -> Option<SelectionRange> {
    let offset = try_position_to_offset(source_text, position)?;

    let root = series.syntax();
    let token = root.token_at_offset(offset).right_biased()?;

    let mut ranges: Vec<Range> = Vec::new();

    // Start with the token itself (e.g. patch name, option, comment text)
    let token_range = text_range_to_lsp_range(source_text, token.text_range());
    ranges.push(token_range);

    // Walk up the tree collecting interesting nodes
    let mut node = token.parent()?;
    loop {
        match node.kind() {
            SyntaxKind::PATCH_ENTRY
            | SyntaxKind::COMMENT_LINE
            | SyntaxKind::OPTIONS
            | SyntaxKind::SERIES_ENTRY
            | SyntaxKind::ROOT => {
                let r = text_range_to_lsp_range(source_text, node.text_range());
                if ranges.last() != Some(&r) {
                    ranges.push(r);
                }
            }
            _ => {}
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }

    // Build nested SelectionRange chain (innermost first)
    let mut result: Option<SelectionRange> = None;
    for range in ranges.into_iter().rev() {
        result = Some(SelectionRange {
            range,
            parent: result.map(Box::new),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_select(text: &str, line: u32, col: u32) -> Vec<SelectionRange> {
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        get_series_selection_ranges(&series, text, &[Position::new(line, col)])
    }

    fn collect_ranges(sr: &SelectionRange) -> Vec<Range> {
        let mut ranges = vec![sr.range];
        let mut current = sr;
        while let Some(parent) = &current.parent {
            ranges.push(parent.range);
            current = parent;
        }
        ranges
    }

    #[test]
    fn test_selection_on_patch_name() {
        let text = "a.patch\nb.patch\n";
        let selections = parse_and_select(text, 0, 2);
        assert_eq!(selections.len(), 1);

        let ranges = collect_ranges(&selections[0]);
        // Should have: token (patch name) → patch entry → series entry → root
        assert!(ranges.len() >= 3);
        // Innermost: patch name token
        assert_eq!(ranges[0].start.line, 0);
        assert_eq!(ranges[0].start.character, 0);
        // Outermost: whole file
        let last = ranges.last().unwrap();
        assert_eq!(last.start.line, 0);
        assert_eq!(last.start.character, 0);
    }

    #[test]
    fn test_selection_on_comment() {
        let text = "# comment\na.patch\n";
        let selections = parse_and_select(text, 0, 2);
        assert_eq!(selections.len(), 1);

        let ranges = collect_ranges(&selections[0]);
        assert!(ranges.len() >= 2);
    }

    #[test]
    fn test_selection_on_option() {
        let text = "a.patch -p1\n";
        let selections = parse_and_select(text, 0, 9);
        assert_eq!(selections.len(), 1);

        let ranges = collect_ranges(&selections[0]);
        // Should include: option token → options node → patch entry → ...
        assert!(ranges.len() >= 3);
    }

    #[test]
    fn test_multiple_positions() {
        let text = "a.patch\nb.patch\n";
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        let positions = vec![Position::new(0, 2), Position::new(1, 2)];
        let selections = get_series_selection_ranges(&series, text, &positions);
        assert_eq!(selections.len(), 2);
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let selections = parse_and_select(text, 0, 0);
        assert_eq!(selections.len(), 0);
    }

    #[test]
    fn test_outermost_is_root() {
        let text = "a.patch\nb.patch\n";
        let selections = parse_and_select(text, 0, 2);
        let ranges = collect_ranges(&selections[0]);
        let last = ranges.last().unwrap();
        // Root covers the whole file
        assert_eq!(last.start, Position::new(0, 0));
    }
}
