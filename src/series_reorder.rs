//! Code actions for reordering patches in a quilt series file.
//!
//! Provides "Move patch up" and "Move patch down" actions that swap
//! adjacent entries in the series file.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_lsp_range_to_text_range};

/// Get reorder code actions for a series file at the given range.
pub fn get_reorder_actions(
    series: &SeriesFile,
    source_text: &str,
    range: Range,
    uri: &Uri,
) -> Vec<CodeAction> {
    let Some(text_range) = try_lsp_range_to_text_range(source_text, &range) else {
        return Vec::new();
    };

    let entries: Vec<_> = series.entries().collect();
    let mut actions = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let entry_range = entry.syntax().text_range();
        if entry_range.end() <= text_range.start() || entry_range.start() > text_range.end() {
            continue;
        }

        // Only offer reorder for patch entries, not comments
        if entry.as_patch_entry().is_none() {
            continue;
        }

        let entry_lsp_range = text_range_to_lsp_range(source_text, entry_range);
        let entry_text = entry.syntax().text().to_string();

        // "Move patch up" - swap with previous entry
        if i > 0 {
            let prev = &entries[i - 1];
            let prev_range = prev.syntax().text_range();
            let prev_lsp_range = text_range_to_lsp_range(source_text, prev_range);
            let prev_text = prev.syntax().text().to_string();

            actions.push(CodeAction {
                title: "Move patch up".to_string(),
                kind: Some(CodeActionKind::REFACTOR),
                edit: Some(WorkspaceEdit {
                    changes: Some(
                        [(
                            uri.clone(),
                            vec![
                                TextEdit {
                                    range: prev_lsp_range,
                                    new_text: entry_text.clone(),
                                },
                                TextEdit {
                                    range: entry_lsp_range,
                                    new_text: prev_text,
                                },
                            ],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }

        // "Move patch down" - swap with next entry
        if i + 1 < entries.len() {
            let next = &entries[i + 1];
            let next_range = next.syntax().text_range();
            let next_lsp_range = text_range_to_lsp_range(source_text, next_range);
            let next_text = next.syntax().text().to_string();

            actions.push(CodeAction {
                title: "Move patch down".to_string(),
                kind: Some(CodeActionKind::REFACTOR),
                edit: Some(WorkspaceEdit {
                    changes: Some(
                        [(
                            uri.clone(),
                            vec![
                                TextEdit {
                                    range: entry_lsp_range,
                                    new_text: next_text,
                                },
                                TextEdit {
                                    range: next_lsp_range,
                                    new_text: entry_text.clone(),
                                },
                            ],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn dummy_uri() -> Uri {
        "file:///project/patches/series".parse().unwrap()
    }

    fn get_actions(text: &str, line: u32) -> Vec<CodeAction> {
        let parsed = parse_series(text);
        let series = parsed.tree();
        let range = Range::new(Position::new(line, 0), Position::new(line, 0));
        get_reorder_actions(&series, text, range, &dummy_uri())
    }

    #[test]
    fn test_middle_entry_has_both_actions() {
        let text = "a.patch\nb.patch\nc.patch\n";
        let actions = get_actions(text, 1);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, vec!["Move patch up", "Move patch down"]);
    }

    #[test]
    fn test_first_entry_only_down() {
        let text = "a.patch\nb.patch\n";
        let actions = get_actions(text, 0);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, vec!["Move patch down"]);
    }

    #[test]
    fn test_last_entry_only_up() {
        let text = "a.patch\nb.patch\n";
        let actions = get_actions(text, 1);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, vec!["Move patch up"]);
    }

    #[test]
    fn test_single_entry_no_actions() {
        let text = "a.patch\n";
        let actions = get_actions(text, 0);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_comment_line_no_actions() {
        let text = "a.patch\n# comment\nb.patch\n";
        let actions = get_actions(text, 1); // on the comment
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_outside_file_no_actions() {
        let text = "a.patch\n";
        let actions = get_actions(text, 5);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_move_down_edits() {
        let text = "a.patch\nb.patch\n";
        let actions = get_actions(text, 0);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Move patch down");

        let edit = actions[0].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.values().next().unwrap();
        assert_eq!(edits.len(), 2);
        // First edit replaces a.patch with b.patch
        assert_eq!(edits[0].new_text, "b.patch\n");
        // Second edit replaces b.patch with a.patch
        assert_eq!(edits[1].new_text, "a.patch\n");
    }

    #[test]
    fn test_move_up_edits() {
        let text = "a.patch\nb.patch\n";
        let actions = get_actions(text, 1);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Move patch up");

        let edit = actions[0].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.values().next().unwrap();
        assert_eq!(edits.len(), 2);
        // First edit replaces a.patch with b.patch
        assert_eq!(edits[0].new_text, "b.patch\n");
        // Second edit replaces b.patch with a.patch
        assert_eq!(edits[1].new_text, "a.patch\n");
    }

    #[test]
    fn test_move_across_comment() {
        let text = "a.patch\n# comment\nb.patch\n";
        let actions = get_actions(text, 0); // on a.patch
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        // a.patch can move down (swap with comment)
        assert_eq!(titles, vec!["Move patch down"]);
    }

    #[test]
    fn test_action_kind_is_refactor() {
        let text = "a.patch\nb.patch\n";
        let actions = get_actions(text, 0);
        assert_eq!(actions[0].kind, Some(CodeActionKind::REFACTOR));
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let actions = get_actions(text, 0);
        assert_eq!(actions.len(), 0);
    }
}
