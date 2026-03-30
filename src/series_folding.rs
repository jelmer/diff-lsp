//! Folding range generation for quilt series files.
//!
//! Collapses groups of entries between comment lines that act as section
//! headers, and also allows folding individual comment blocks.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

/// Generate folding ranges for a series file.
///
/// Creates foldable regions for groups of consecutive patch entries between
/// comment lines. A comment followed by one or more patch entries forms a
/// foldable section.
pub fn generate_series_folding_ranges(series: &SeriesFile, source_text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let mut section_start: Option<(u32, String)> = None;
    let mut section_end_line = 0u32;

    for entry in series.entries() {
        let start = offset_to_position(source_text, entry.syntax().text_range().start());
        let end = offset_to_position(source_text, entry.syntax().text_range().end());

        if entry.as_comment_line().is_some() {
            // Close previous section if it spans multiple lines
            if let Some((start_line, text)) = section_start.take() {
                if start_line < section_end_line {
                    ranges.push(FoldingRange {
                        start_line,
                        start_character: None,
                        end_line: section_end_line,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some(text),
                    });
                }
            }
            // Start a new section from this comment
            let comment = entry.as_comment_line().unwrap();
            let text = comment.full_text().trim().to_string();
            section_start = Some((start.line, text));
            section_end_line = start.line;
        } else if entry.as_patch_entry().is_some() {
            // Extend the current section, or start an implicit one
            if section_start.is_none() && start.line > 0 {
                // No comment header — don't create a section for leading entries
            }
            if end.line > 0 {
                section_end_line = end.line - 1; // don't include trailing newline
            }
        }
    }

    // Close final section
    if let Some((start_line, text)) = section_start.take() {
        if start_line < section_end_line {
            ranges.push(FoldingRange {
                start_line,
                start_character: None,
                end_line: section_end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(text),
            });
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_fold(text: &str) -> Vec<FoldingRange> {
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        generate_series_folding_ranges(&series, text)
    }

    #[test]
    fn test_comment_section() {
        let text = "# Upstream fixes\na.patch\nb.patch\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 2);
        assert_eq!(
            ranges[0].collapsed_text,
            Some("# Upstream fixes".to_string())
        );
    }

    #[test]
    fn test_two_sections() {
        let text = "# Section 1\na.patch\n# Section 2\nb.patch\nc.patch\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 1);
        assert_eq!(ranges[1].start_line, 2);
        assert_eq!(ranges[1].end_line, 4);
    }

    #[test]
    fn test_no_comments() {
        let text = "a.patch\nb.patch\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_comment_only_no_entries() {
        let text = "# Just a comment\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_single_entry_after_comment() {
        let text = "# Section\na.patch\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 1);
    }

    #[test]
    fn test_entries_before_first_comment() {
        let text = "a.patch\n# Section\nb.patch\nc.patch\n";
        let ranges = parse_and_fold(text);
        // Only the section after the comment is foldable
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 3);
    }

    #[test]
    fn test_fold_kind_is_region() {
        let text = "# Section\na.patch\nb.patch\n";
        let ranges = parse_and_fold(text);
        assert_eq!(ranges[0].kind, Some(FoldingRangeKind::Region));
    }
}
