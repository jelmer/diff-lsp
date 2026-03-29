//! Inlay hint generation for patch files.
//!
//! Shows addition/deletion counts at the end of hunk headers.

use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{offset_to_position, try_lsp_range_to_text_range};

/// Generate inlay hints for hunks within the given range.
pub fn get_inlay_hints(patch: &Patch, source_text: &str, range: Range) -> Vec<InlayHint> {
    // Convert range, falling back to the whole document
    let text_range = try_lsp_range_to_text_range(source_text, &range);

    let mut hints = Vec::new();

    for file in patch.patch_files() {
        for hunk in file.hunks() {
            // Skip hunks outside the requested range if we have one
            if let Some(ref tr) = text_range {
                let hunk_range = hunk.syntax().text_range();
                if hunk_range.end() < tr.start() || hunk_range.start() > tr.end() {
                    continue;
                }
            }

            let Some(header) = hunk.header() else {
                continue;
            };

            let stats = hunk.stats();
            if stats.additions == 0 && stats.deletions == 0 {
                continue;
            }

            // Place the hint at the end of the hunk header line
            let header_end = header.syntax().text_range().end();
            let mut pos = offset_to_position(source_text, header_end);
            // Back up before the newline
            if pos.character > 0 {
                pos.character -= 1;
            }

            let label = format!(" +{} −{}", stats.additions, stats.deletions);

            hints.push(InlayHint {
                position: pos,
                label: InlayHintLabel::String(label),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }
    }

    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_hint(text: &str) -> Vec<InlayHint> {
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree_lossy();
        let range = Range::new(Position::new(0, 0), Position::new(1000, 0));
        get_inlay_hints(&patch, text, range)
    }

    #[test]
    fn test_hunk_hint() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 context
-deleted
+added1
+added2
 context2
";
        let hints = parse_and_hint(text);
        assert_eq!(hints.len(), 1);

        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, " +2 −1"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn test_multiple_hunks() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+b
@@ -10,2 +10,3 @@
 ctx
-old
+new1
+new2
";
        let hints = parse_and_hint(text);
        assert_eq!(hints.len(), 2);

        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, " +1 −1"),
            _ => panic!("expected string label"),
        }
        match &hints[1].label {
            InlayHintLabel::String(s) => assert_eq!(s, " +2 −1"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn test_no_hint_for_context_only() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 line1
 line2
";
        let hints = parse_and_hint(text);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_hint_range_filtering() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+b
@@ -100 +100 @@
-c
+d
";
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree_lossy();

        // Request only the first few lines
        let range = Range::new(Position::new(0, 0), Position::new(4, 0));
        let hints = get_inlay_hints(&patch, text, range);
        assert_eq!(hints.len(), 1);
    }
}
