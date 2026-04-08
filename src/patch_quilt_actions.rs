//! Code actions for patch files in a quilt project.
//!
//! Provides:
//! - "Refresh patch" (quilt refresh) when the patch is the top applied patch
//! - "Import patch" (quilt import) when the patch is not listed in the series

use tower_lsp_server::ls_types::*;

use crate::series_code_actions::{CMD_QUILT_IMPORT, CMD_QUILT_REFRESH};

/// Get quilt code actions for a patch file.
///
/// Offers:
/// - "Refresh patch" when the file is the top applied patch
/// - "Import patch" when the file has a sibling series file but is not listed in it
pub fn get_patch_quilt_actions(uri: &Uri) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let Some(patch_name) = extract_patch_name(uri) else {
        return actions;
    };

    // "Refresh patch" — only for the top applied patch
    if let Some(top_patch) = get_top_applied_patch(uri) {
        if patch_name == top_patch {
            actions.push(CodeAction {
                title: "Refresh patch (quilt refresh)".to_string(),
                kind: Some(CodeActionKind::new("source")),
                command: Some(Command {
                    title: "Refresh patch".to_string(),
                    command: CMD_QUILT_REFRESH.to_string(),
                    arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
                }),
                ..Default::default()
            });
        }
    }

    // "Import patch" — when the patch is not listed in the series
    if is_not_in_series(uri, &patch_name) {
        actions.push(CodeAction {
            title: "Import patch (quilt import)".to_string(),
            kind: Some(CodeActionKind::new("source")),
            command: Some(Command {
                title: "Import patch".to_string(),
                command: CMD_QUILT_IMPORT.to_string(),
                arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
            }),
            ..Default::default()
        });
    }

    actions
}

/// Extract the file name from a URI.
fn extract_patch_name(uri: &Uri) -> Option<String> {
    let path = uri.to_file_path()?;
    path.file_name()?.to_str().map(|s| s.to_string())
}

/// Check if a patch file is not listed in the series file.
///
/// Returns true if there is a sibling `series` file but it does not
/// contain the patch name.
fn is_not_in_series(uri: &Uri, patch_name: &str) -> bool {
    let Some(patch_path) = uri.to_file_path() else {
        return false;
    };
    let Some(patches_dir) = patch_path.parent() else {
        return false;
    };
    let series_path = patches_dir.join("series");
    let Ok(content) = std::fs::read_to_string(series_path) else {
        return false;
    };
    !content
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .any(|l| l.split_whitespace().next() == Some(patch_name))
}

/// Get the top applied patch name by reading .pc/applied-patches.
fn get_top_applied_patch(uri: &Uri) -> Option<String> {
    let patch_path = uri.to_file_path()?;
    let patches_dir = patch_path.parent()?;
    let project_dir = patches_dir.parent()?;
    let applied_path = project_dir.join(".pc").join("applied-patches");
    let content = std::fs::read_to_string(applied_path).ok()?;
    content
        .lines()
        .rfind(|l| !l.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn setup_quilt_project() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        (dir, patches_dir)
    }

    fn make_uri(path: &Path) -> Uri {
        Uri::from_file_path(path).unwrap()
    }

    #[test]
    fn test_refresh_action_for_top_patch() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(
            dir.path().join("debian/.pc/applied-patches"),
            "a.patch\nb.patch\n",
        )
        .unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Refresh patch (quilt refresh)");
        assert_eq!(
            actions[0].command.as_ref().unwrap().command,
            CMD_QUILT_REFRESH
        );
    }

    #[test]
    fn test_no_refresh_for_non_top_patch() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(
            dir.path().join("debian/.pc/applied-patches"),
            "a.patch\nb.patch\n",
        )
        .unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_no_refresh_for_unapplied_patch() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(dir.path().join("debian/.pc/applied-patches"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_no_refresh_without_pc_dir() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_no_refresh_no_applied_patches() {
        let (_dir, patches_dir) = setup_quilt_project();
        // No applied-patches file

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_refresh_command_arguments() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(dir.path().join("debian/.pc/applied-patches"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        let cmd = actions[0].command.as_ref().unwrap();
        let args = cmd.arguments.as_ref().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], serde_json::Value::String(uri.to_string()));
    }

    #[test]
    fn test_refresh_action_kind() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(dir.path().join("debian/.pc/applied-patches"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions[0].kind, Some(CodeActionKind::new("source")));
    }

    // --- Import patch tests ---

    #[test]
    fn test_import_action_for_unlisted_patch() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        // b.patch exists but is not in series
        let uri = make_uri(&patches_dir.join("b.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Import patch (quilt import)");
        assert_eq!(
            actions[0].command.as_ref().unwrap().command,
            CMD_QUILT_IMPORT
        );
    }

    #[test]
    fn test_no_import_for_listed_patch() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        // No import action (a.patch is listed), no refresh (no applied-patches)
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_no_import_without_series_file() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        // No series file

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_import_command_arguments() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let actions = get_patch_quilt_actions(&uri);
        let cmd = actions[0].command.as_ref().unwrap();
        let args = cmd.arguments.as_ref().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], serde_json::Value::String(uri.to_string()));
    }

    #[test]
    fn test_import_action_kind() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions[0].kind, Some(CodeActionKind::new("source")));
    }

    #[test]
    fn test_both_refresh_and_import_not_possible() {
        // If a patch is the top applied patch, it's necessarily in the series,
        // so import should not be offered alongside refresh
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();
        std::fs::write(dir.path().join("debian/.pc/applied-patches"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, vec!["Refresh patch (quilt refresh)"]);
    }

    #[test]
    fn test_import_with_options_in_series() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch -p1\n").unwrap();

        // a.patch is listed (with options), so no import
        let uri = make_uri(&patches_dir.join("a.patch"));
        let actions = get_patch_quilt_actions(&uri);
        assert_eq!(actions.len(), 0);
    }
}
