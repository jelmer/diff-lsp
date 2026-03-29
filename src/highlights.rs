//! Document highlight support for patch files.
//!
//! When the cursor is on a file header or within a hunk, highlights all
//! related sections for the same file.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get document highlights at the given position.
pub fn get_highlights(
    patch: &Patch,
    source_text: &str,
    position: Position,
) -> Vec<DocumentHighlight> {
    let Some(offset) = try_position_to_offset(source_text, position) else {
        return Vec::new();
    };

    let root = patch.syntax();
    let Some(token) = root.token_at_offset(offset).right_biased() else {
        return Vec::new();
    };

    // Walk up to find the containing PatchFile
    let mut node = match token.parent() {
        Some(n) => n,
        None => return Vec::new(),
    };

    let target_path = loop {
        match node.kind() {
            SyntaxKind::PATCH_FILE => {
                let file = patchkit::edit::PatchFile::cast(node).unwrap();
                break file.path();
            }
            SyntaxKind::ROOT => return Vec::new(),
            _ => match node.parent() {
                Some(parent) => node = parent,
                None => return Vec::new(),
            },
        }
    };

    let Some(target_path) = target_path else {
        return Vec::new();
    };

    // Highlight all PatchFile sections that share the same path
    let mut highlights = Vec::new();

    for file in patch.patch_files() {
        if file.path().as_deref() != Some(&target_path) {
            continue;
        }

        // Highlight file headers as Write (definition)
        if let Some(old_file) = file.old_file() {
            highlights.push(DocumentHighlight {
                range: text_range_to_lsp_range(source_text, old_file.syntax().text_range()),
                kind: Some(DocumentHighlightKind::WRITE),
            });
        }
        if let Some(new_file) = file.new_file() {
            highlights.push(DocumentHighlight {
                range: text_range_to_lsp_range(source_text, new_file.syntax().text_range()),
                kind: Some(DocumentHighlightKind::WRITE),
            });
        }

        // Highlight hunks as Read (usage)
        for hunk in file.hunks() {
            if let Some(header) = hunk.header() {
                highlights.push(DocumentHighlight {
                    range: text_range_to_lsp_range(source_text, header.syntax().text_range()),
                    kind: Some(DocumentHighlightKind::READ),
                });
            }
        }
    }

    highlights
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_highlight(text: &str, line: u32, character: u32) -> Vec<DocumentHighlight> {
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree_lossy();
        get_highlights(&patch, text, Position::new(line, character))
    }

    #[test]
    fn test_highlight_from_file_header() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let highlights = parse_and_highlight(text, 0, 5); // cursor on "a/file.txt"
                                                          // Should highlight: old file header, new file header, hunk header
        assert_eq!(highlights.len(), 3);
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::WRITE));
        assert_eq!(highlights[1].kind, Some(DocumentHighlightKind::WRITE));
        assert_eq!(highlights[2].kind, Some(DocumentHighlightKind::READ));
    }

    #[test]
    fn test_highlight_from_hunk_line() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let highlights = parse_and_highlight(text, 3, 1); // cursor on "-old"
        assert_eq!(highlights.len(), 3);
    }

    #[test]
    fn test_highlight_multiple_files_only_matches() {
        let text = "\
--- a/foo.txt
+++ b/foo.txt
@@ -1 +1 @@
-a
+b
--- a/bar.txt
+++ b/bar.txt
@@ -1 +1 @@
-c
+d
";
        // Cursor on foo.txt header
        let highlights = parse_and_highlight(text, 0, 5);
        assert_eq!(highlights.len(), 3); // old header, new header, hunk

        // Cursor on bar.txt hunk
        let highlights = parse_and_highlight(text, 8, 1);
        assert_eq!(highlights.len(), 3); // old header, new header, hunk
    }

    #[test]
    fn test_highlight_multiple_hunks_same_file() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+b
@@ -10 +10 @@
-c
+d
";
        let highlights = parse_and_highlight(text, 0, 5);
        // old header, new header, hunk1 header, hunk2 header
        assert_eq!(highlights.len(), 4);
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::WRITE));
        assert_eq!(highlights[1].kind, Some(DocumentHighlightKind::WRITE));
        assert_eq!(highlights[2].kind, Some(DocumentHighlightKind::READ));
        assert_eq!(highlights[3].kind, Some(DocumentHighlightKind::READ));
    }

    #[test]
    fn test_highlight_outside_patch() {
        let text = "some junk text\n";
        let highlights = parse_and_highlight(text, 0, 5);
        assert!(highlights.is_empty());
    }
}
