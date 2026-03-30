//! Code actions for patch files in a quilt project.
//!
//! Provides "Refresh patch" (quilt refresh) when the patch is the
//! currently applied top patch.

use std::path::Path;
use tower_lsp_server::ls_types::*;

use crate::series_code_actions::CMD_QUILT_REFRESH;

/// Get quilt code actions for a patch file.
///
/// Currently offers "Refresh patch" when the file is the top applied patch.
pub fn get_patch_quilt_actions(uri: &Uri) -> Vec<CodeAction> {
    let Some(patch_name) = extract_patch_name(uri) else {
        return Vec::new();
    };

    let Some(top_patch) = get_top_applied_patch(uri) else {
        return Vec::new();
    };

    if patch_name != top_patch {
        return Vec::new();
    }

    vec![CodeAction {
        title: "Refresh patch (quilt refresh)".to_string(),
        kind: Some(CodeActionKind::new("source")),
        command: Some(Command {
            title: "Refresh patch".to_string(),
            command: CMD_QUILT_REFRESH.to_string(),
            arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
        }),
        ..Default::default()
    }]
}

/// Extract the file name from a URI.
fn extract_patch_name(uri: &Uri) -> Option<String> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    path.file_name()?.to_str().map(|s| s.to_string())
}

/// Get the top applied patch name by reading .pc/applied-patches.
fn get_top_applied_patch(uri: &Uri) -> Option<String> {
    let path_str = uri.path().as_str();
    let patch_path = Path::new(path_str);
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

    fn setup_quilt_project() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        (dir, patches_dir)
    }

    fn make_uri(path: &Path) -> Uri {
        format!("file://{}", path.display()).parse().unwrap()
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
}
