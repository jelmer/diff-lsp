//! Code actions for patch files.
//!
//! Provides:
//! - "Remove hunk" - delete a hunk from the patch
//! - "Reverse hunk" - swap added and deleted lines

use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_lsp_range_to_text_range};

/// Get code actions available at the given range.
pub fn get_code_actions(
    patch: &Patch,
    source_text: &str,
    range: Range,
    uri: &Uri,
) -> Vec<CodeAction> {
    let Some(text_range) = try_lsp_range_to_text_range(source_text, &range) else {
        return Vec::new();
    };

    let mut actions = Vec::new();

    for file in patch.patch_files() {
        for hunk in file.hunks() {
            let hunk_range = hunk.syntax().text_range();

            // Only offer actions if the cursor/selection overlaps with this hunk
            if hunk_range.end() < text_range.start() || hunk_range.start() > text_range.end() {
                continue;
            }

            let lsp_hunk_range = text_range_to_lsp_range(source_text, hunk_range);

            // "Remove hunk"
            actions.push(CodeAction {
                title: "Remove hunk".to_string(),
                kind: Some(CodeActionKind::REFACTOR),
                diagnostics: None,
                edit: Some(WorkspaceEdit {
                    changes: Some(
                        [(
                            uri.clone(),
                            vec![TextEdit {
                                range: lsp_hunk_range,
                                new_text: String::new(),
                            }],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                }),
                command: None,
                is_preferred: None,
                disabled: None,
                data: None,
            });

            // "Fix hunk line counts" (only if counts are wrong)
            if let Some(header) = hunk.header() {
                if !header.check_counts(&hunk).is_empty() {
                    // Clone the tree, fix in-place, extract the corrected header text
                    let header_range = header.syntax().text_range();
                    let lsp_header_range = text_range_to_lsp_range(source_text, header_range);

                    // Re-parse to get a mutable copy, find the same hunk, fix it
                    let reparsed = patchkit::edit::parse(source_text);
                    let fixed_patch = reparsed.tree_lossy();
                    // Walk to the matching hunk by text range
                    for fixed_file in fixed_patch.patch_files() {
                        for fixed_hunk in fixed_file.hunks() {
                            if fixed_hunk.syntax().text_range() == hunk_range {
                                fixed_hunk.fix_counts();
                                let fixed_header_text =
                                    fixed_hunk.header().unwrap().syntax().text().to_string();
                                actions.push(CodeAction {
                                    title: "Fix hunk line counts".to_string(),
                                    kind: Some(CodeActionKind::QUICKFIX),
                                    diagnostics: None,
                                    edit: Some(WorkspaceEdit {
                                        changes: Some(
                                            [(
                                                uri.clone(),
                                                vec![TextEdit {
                                                    range: lsp_header_range,
                                                    new_text: fixed_header_text,
                                                }],
                                            )]
                                            .into_iter()
                                            .collect(),
                                        ),
                                        ..Default::default()
                                    }),
                                    command: None,
                                    is_preferred: Some(true),
                                    disabled: None,
                                    data: None,
                                });
                                break;
                            }
                        }
                    }
                }
            }

            // "Reverse hunk"
            if let Some(reversed) = reverse_hunk(source_text, &hunk) {
                actions.push(CodeAction {
                    title: "Reverse hunk".to_string(),
                    kind: Some(CodeActionKind::REFACTOR),
                    diagnostics: None,
                    edit: Some(WorkspaceEdit {
                        changes: Some(
                            [(
                                uri.clone(),
                                vec![TextEdit {
                                    range: lsp_hunk_range,
                                    new_text: reversed,
                                }],
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..Default::default()
                    }),
                    command: None,
                    is_preferred: None,
                    disabled: None,
                    data: None,
                });
            }
        }
    }

    actions
}

/// Reverse a hunk by swapping + and - lines and swapping the ranges in the header.
fn reverse_hunk(source_text: &str, hunk: &patchkit::edit::Hunk) -> Option<String> {
    let hunk_text = hunk.syntax().text().to_string();
    let mut result = String::new();

    for line in hunk_text.lines() {
        if line.starts_with("@@") {
            // Swap the ranges: @@ -old +new @@ -> @@ -new +old @@
            result.push_str(&reverse_hunk_header(line));
        } else if let Some(rest) = line.strip_prefix('-') {
            result.push('+');
            result.push_str(rest);
        } else if let Some(rest) = line.strip_prefix('+') {
            result.push('-');
            result.push_str(rest);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    // Check if we need a trailing newline
    let hunk_range = hunk.syntax().text_range();
    let hunk_src = &source_text[usize::from(hunk_range.start())..usize::from(hunk_range.end())];
    if !hunk_src.ends_with('\n') {
        // Remove the trailing newline we added
        result.pop();
    }

    Some(result)
}

/// Reverse a hunk header line by swapping the old and new ranges.
fn reverse_hunk_header(header: &str) -> String {
    // Parse: @@ -A,B +C,D @@ optional text
    // Produce: @@ -C,D +A,B @@ optional text
    let header = header.trim_end();

    let Some(inner_start) = header.find("@@") else {
        return header.to_string();
    };
    let rest = &header[inner_start + 2..];
    let Some(inner_end) = rest.find("@@") else {
        return header.to_string();
    };
    let inner = rest[..inner_end].trim();
    let tail = &rest[inner_end + 2..];

    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() < 2 {
        return header.to_string();
    }

    let old_range = parts[0]; // e.g. "-1,3"
    let new_range = parts[1]; // e.g. "+4,5"

    // Swap the signs
    let reversed_old = format!("-{}", new_range.trim_start_matches('+'));
    let reversed_new = format!("+{}", old_range.trim_start_matches('-'));

    format!("@@ {} {} @@{}", reversed_old, reversed_new, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_actions(text: &str, line: u32) -> Vec<CodeAction> {
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree_lossy();
        let range = Range::new(Position::new(line, 0), Position::new(line, 0));
        let uri: Uri = "file:///test.patch".parse().unwrap();
        get_code_actions(&patch, text, range, &uri)
    }

    #[test]
    fn test_remove_hunk_action() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let actions = parse_and_actions(text, 2); // on the hunk header
        let titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(titles.contains(&"Remove hunk"));
    }

    #[test]
    fn test_reverse_hunk_action() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let actions = parse_and_actions(text, 3); // on a hunk line
        let titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(titles.contains(&"Reverse hunk"));

        let reverse = actions.iter().find(|a| a.title == "Reverse hunk").unwrap();
        let edit = reverse.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.values().next().unwrap();
        // Reversed: -old/+new becomes +old/-new
        assert_eq!(edits[0].new_text, "@@ -1 +1 @@\n+old\n-new\n");
    }

    #[test]
    fn test_reverse_hunk_header_swap() {
        assert_eq!(reverse_hunk_header("@@ -1,3 +4,5 @@"), "@@ -4,5 +1,3 @@");
        assert_eq!(
            reverse_hunk_header("@@ -1 +1 @@ context"),
            "@@ -1 +1 @@ context"
        );
    }

    #[test]
    fn test_no_actions_outside_hunk() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let actions = parse_and_actions(text, 0); // on the file header
        assert!(actions.is_empty());
    }

    #[test]
    fn test_actions_with_multiple_hunks() {
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
        // On first hunk line
        let actions = parse_and_actions(text, 3);
        assert_eq!(actions.len(), 2); // Remove + Reverse

        // On second hunk line
        let actions = parse_and_actions(text, 6);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_fix_hunk_counts_action() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,99 +1,99 @@
-old
+new
";
        let actions = parse_and_actions(text, 3);
        let titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(titles.contains(&"Fix hunk line counts"), "got: {titles:?}");

        let fix = actions
            .iter()
            .find(|a| a.title == "Fix hunk line counts")
            .unwrap();
        assert_eq!(fix.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(fix.is_preferred, Some(true));

        // The edit should replace the header with corrected counts
        let edit = fix.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.values().next().unwrap();
        assert_eq!(edits[0].new_text, "@@ -1,1 +1,1 @@\n");
    }

    #[test]
    fn test_no_fix_action_when_counts_correct() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 ctx
-old
+new
";
        let actions = parse_and_actions(text, 3);
        let titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
        assert!(!titles.contains(&"Fix hunk line counts"));
    }
}
