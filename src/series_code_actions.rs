//! Code actions for quilt series files.
//!
//! Provides actions to apply/unapply patches using quilt commands:
//! - "Apply patch" (quilt push <patch>) - apply up to a specific patch
//! - "Unapply patch" (quilt pop <patch>) - unapply down to a specific patch
//! - "Apply all patches" (quilt push -a)
//! - "Unapply all patches" (quilt pop -a)
//! - "Remove patch from series" - delete the entry line
//! - "Delete patch" (quilt delete <patch>) - remove from series and delete file

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use std::collections::HashSet;
use std::path::Path;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_lsp_range_to_text_range};

/// Command identifiers for quilt operations.
pub const CMD_QUILT_PUSH: &str = "diff-lsp.quiltPush";
pub const CMD_QUILT_POP: &str = "diff-lsp.quiltPop";
pub const CMD_QUILT_PUSH_ALL: &str = "diff-lsp.quiltPushAll";
pub const CMD_QUILT_POP_ALL: &str = "diff-lsp.quiltPopAll";
pub const CMD_QUILT_DELETE: &str = "diff-lsp.quiltDelete";
pub const CMD_QUILT_REFRESH: &str = "diff-lsp.quiltRefresh";

/// All command identifiers registered by this module.
pub const COMMANDS: &[&str] = &[
    CMD_QUILT_PUSH,
    CMD_QUILT_POP,
    CMD_QUILT_PUSH_ALL,
    CMD_QUILT_POP_ALL,
    CMD_QUILT_DELETE,
    CMD_QUILT_REFRESH,
];

/// Read the list of currently applied patches from `.pc/applied-patches`.
fn read_applied_patches(series_uri: &Uri) -> HashSet<String> {
    let path_str = series_uri.path().as_str();
    let series_path = Path::new(path_str);
    let Some(patches_dir) = series_path.parent() else {
        return HashSet::new();
    };
    let Some(project_dir) = patches_dir.parent() else {
        return HashSet::new();
    };
    let applied_path = project_dir.join(".pc").join("applied-patches");
    let Ok(content) = std::fs::read_to_string(applied_path) else {
        return HashSet::new();
    };
    content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Get code actions for a series file at the given range.
pub fn get_series_code_actions(
    series: &SeriesFile,
    source_text: &str,
    range: Range,
    uri: &Uri,
) -> Vec<CodeAction> {
    let Some(text_range) = try_lsp_range_to_text_range(source_text, &range) else {
        return Vec::new();
    };

    let applied = read_applied_patches(uri);
    let mut actions = Vec::new();

    for entry in series.patch_entries() {
        let entry_range = entry.syntax().text_range();

        // Only offer actions if cursor overlaps this entry
        if entry_range.end() < text_range.start() || entry_range.start() > text_range.end() {
            continue;
        }

        let Some(name) = entry.name() else {
            continue;
        };

        let is_applied = applied.contains(&name);

        if !is_applied {
            actions.push(CodeAction {
                title: format!("Apply patch (quilt push {})", name),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: format!("Apply patch {}", name),
                    command: CMD_QUILT_PUSH.to_string(),
                    arguments: Some(vec![
                        serde_json::Value::String(uri.to_string()),
                        serde_json::Value::String(name.clone()),
                    ]),
                }),
                ..Default::default()
            });
        }

        if is_applied {
            actions.push(CodeAction {
                title: format!("Unapply patch (quilt pop {})", name),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: format!("Unapply patch {}", name),
                    command: CMD_QUILT_POP.to_string(),
                    arguments: Some(vec![
                        serde_json::Value::String(uri.to_string()),
                        serde_json::Value::String(name.clone()),
                    ]),
                }),
                ..Default::default()
            });
        }

        // "Remove from series" - just delete the entry line
        let entry_lsp_range = text_range_to_lsp_range(source_text, entry_range);
        actions.push(CodeAction {
            title: format!("Remove '{}' from series", name),
            kind: Some(CodeActionKind::REFACTOR),
            edit: Some(WorkspaceEdit {
                changes: Some(
                    [(
                        uri.clone(),
                        vec![TextEdit {
                            range: entry_lsp_range,
                            new_text: String::new(),
                        }],
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        });

        // "Delete patch" - run quilt delete to remove from series and delete file
        if !is_applied {
            actions.push(CodeAction {
                title: format!("Delete patch (quilt delete {})", name),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: format!("Delete patch {}", name),
                    command: CMD_QUILT_DELETE.to_string(),
                    arguments: Some(vec![
                        serde_json::Value::String(uri.to_string()),
                        serde_json::Value::String(name.clone()),
                    ]),
                }),
                ..Default::default()
            });
        }
    }

    // Always offer push-all / pop-all when on any patch entry
    if !actions.is_empty() {
        let has_unapplied = series
            .patch_entries()
            .any(|e| e.name().is_some_and(|n| !applied.contains(&n)));
        let has_applied = !applied.is_empty();

        if has_unapplied {
            actions.push(CodeAction {
                title: "Apply all patches (quilt push -a)".to_string(),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: "Apply all patches".to_string(),
                    command: CMD_QUILT_PUSH_ALL.to_string(),
                    arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
                }),
                ..Default::default()
            });
        }

        if has_applied {
            actions.push(CodeAction {
                title: "Unapply all patches (quilt pop -a)".to_string(),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: "Unapply all patches".to_string(),
                    command: CMD_QUILT_POP_ALL.to_string(),
                    arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
                }),
                ..Default::default()
            });
        }
    }

    actions
}

/// Find the working directory for quilt commands given a series file URI.
///
/// Quilt expects to be run from the project root (parent of `patches/`).
pub fn find_quilt_working_dir(series_uri: &str) -> Option<std::path::PathBuf> {
    let uri: Uri = series_uri.parse().ok()?;
    let path_str = uri.path().as_str();
    let series_path = Path::new(path_str);
    let patches_dir = series_path.parent()?;
    patches_dir.parent().map(|p| p.to_path_buf())
}

/// Execute a quilt command and return its output or error message.
pub fn run_quilt_command(args: &[&str], working_dir: &Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("quilt")
        .args(args)
        .current_dir(working_dir)
        .output()
        .map_err(|e| format!("Failed to run quilt: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "quilt {} failed:\n{}{}",
            args.join(" "),
            stdout,
            stderr
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn make_uri(path: &std::path::Path) -> Uri {
        format!("file://{}", path.display()).parse().unwrap()
    }

    fn setup_quilt_project() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        (dir, patches_dir, pc_dir)
    }

    // --- read_applied_patches tests ---

    #[test]
    fn test_read_applied_patches_some() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\nb.patch\n").unwrap();

        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);
        let applied = read_applied_patches(&uri);
        assert_eq!(
            applied,
            HashSet::from(["a.patch".to_string(), "b.patch".to_string()])
        );
    }

    #[test]
    fn test_read_applied_patches_none() {
        let (_dir, patches_dir, _pc_dir) = setup_quilt_project();
        // No applied-patches file
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);
        let applied = read_applied_patches(&uri);
        assert_eq!(applied, HashSet::new());
    }

    // --- get_series_code_actions tests ---

    #[test]
    fn test_actions_on_unapplied_patch() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Apply patch (quilt push a.patch)",
                "Remove 'a.patch' from series",
                "Delete patch (quilt delete a.patch)",
                "Apply all patches (quilt push -a)",
            ]
        );
    }

    #[test]
    fn test_actions_on_applied_patch() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\n").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        // Cursor on first line (a.patch, which is applied)
        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Unapply patch (quilt pop a.patch)",
                "Remove 'a.patch' from series",
                "Apply all patches (quilt push -a)",
                "Unapply all patches (quilt pop -a)",
            ]
        );
    }

    #[test]
    fn test_actions_not_on_entry() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        // Cursor beyond the file content
        let range = Range::new(Position::new(5, 0), Position::new(5, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_actions_no_pc_dir() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        // No .pc dir means no applied patches, so push + remove + delete actions
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Apply patch (quilt push a.patch)",
                "Remove 'a.patch' from series",
                "Delete patch (quilt delete a.patch)",
                "Apply all patches (quilt push -a)",
            ]
        );
    }

    #[test]
    fn test_actions_all_applied_no_push_all() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\n").unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        // All applied: pop + remove actions, no push-all, no delete (applied)
        assert_eq!(
            titles,
            vec![
                "Unapply patch (quilt pop a.patch)",
                "Remove 'a.patch' from series",
                "Unapply all patches (quilt pop -a)",
            ]
        );
    }

    #[test]
    fn test_action_command_arguments() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let push_action = &actions[0];
        let cmd = push_action.command.as_ref().unwrap();
        assert_eq!(cmd.command, CMD_QUILT_PUSH);
        let args = cmd.arguments.as_ref().unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], serde_json::Value::String(uri.to_string()));
        assert_eq!(args[1], serde_json::Value::String("a.patch".to_string()));
    }

    #[test]
    fn test_push_all_command_arguments() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let push_all = actions
            .iter()
            .find(|a| a.title.contains("push -a"))
            .unwrap();
        let cmd = push_all.command.as_ref().unwrap();
        assert_eq!(cmd.command, CMD_QUILT_PUSH_ALL);
        let args = cmd.arguments.as_ref().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], serde_json::Value::String(uri.to_string()));
    }

    // --- find_quilt_working_dir tests ---

    #[test]
    fn test_find_quilt_working_dir() {
        assert_eq!(
            find_quilt_working_dir("file:///project/debian/patches/series"),
            Some(std::path::PathBuf::from("/project/debian"))
        );
    }

    #[test]
    fn test_find_quilt_working_dir_simple() {
        assert_eq!(
            find_quilt_working_dir("file:///project/patches/series"),
            Some(std::path::PathBuf::from("/project"))
        );
    }

    // --- COMMANDS constant test ---

    #[test]
    fn test_commands_list() {
        assert_eq!(
            COMMANDS,
            &[
                CMD_QUILT_PUSH,
                CMD_QUILT_POP,
                CMD_QUILT_PUSH_ALL,
                CMD_QUILT_POP_ALL,
                CMD_QUILT_DELETE,
                CMD_QUILT_REFRESH,
            ]
        );
    }

    // --- remove from series test ---

    #[test]
    fn test_remove_from_series_edit() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        let remove = actions
            .iter()
            .find(|a| a.title == "Remove 'a.patch' from series")
            .unwrap();
        assert_eq!(remove.kind, Some(CodeActionKind::REFACTOR));
        // Should have an edit, not a command
        assert_eq!(remove.edit.is_some(), true);
        assert_eq!(remove.command, None);
        let edit = remove.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.values().next().unwrap();
        assert_eq!(edits[0].new_text, "");
    }

    // --- delete patch test ---

    #[test]
    fn test_delete_patch_only_for_unapplied() {
        let (_dir, patches_dir, pc_dir) = setup_quilt_project();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\n").unwrap();

        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri = make_uri(&series_path);

        let range = Range::new(Position::new(0, 0), Position::new(0, 0));
        let actions = get_series_code_actions(&series, text, range, &uri);

        // Applied patch: pop + remove + pop-all, no delete
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Unapply patch (quilt pop a.patch)",
                "Remove 'a.patch' from series",
                "Unapply all patches (quilt pop -a)",
            ]
        );
    }
}
