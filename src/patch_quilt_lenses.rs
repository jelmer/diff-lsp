//! Code lenses for patch files in a quilt project.
//!
//! Shows the patch's position in the series and its applied/unapplied status.

use std::path::Path;
use tower_lsp_server::ls_types::*;

/// Generate quilt-aware code lenses for a patch file.
///
/// Shows a lens at the top of the file with the patch's position in the
/// series and whether it is currently applied.
pub fn get_patch_quilt_lenses(uri: &Uri) -> Vec<CodeLens> {
    let Some(info) = get_quilt_info(uri) else {
        return Vec::new();
    };

    let label = if info.is_top {
        format!("quilt: applied (top, {}/{})", info.position, info.total)
    } else if info.is_applied {
        format!("quilt: applied ({}/{})", info.position, info.total)
    } else {
        format!("quilt: unapplied ({}/{})", info.position, info.total)
    };

    vec![CodeLens {
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        command: Some(Command {
            title: label,
            command: String::new(),
            arguments: None,
        }),
        data: None,
    }]
}

struct QuiltInfo {
    position: usize,
    total: usize,
    is_applied: bool,
    is_top: bool,
}

/// Try to find quilt context for a patch file URI.
fn get_quilt_info(uri: &Uri) -> Option<QuiltInfo> {
    let patch_path = uri.to_file_path()?;
    let patch_name = patch_path.file_name()?.to_str()?;

    let patches_dir = patch_path.parent()?;
    let series_path = patches_dir.join("series");

    // Read series file
    let series_content = std::fs::read_to_string(&series_path).ok()?;
    let series_names: Vec<&str> = series_content
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().next().unwrap_or(l))
        .collect();

    let total = series_names.len();
    let position = series_names.iter().position(|&n| n == patch_name)?;

    // Read applied patches
    let applied = read_applied_patches(patches_dir);
    let is_applied = applied.iter().any(|a| a == patch_name);
    let is_top = applied.last().map(|s| s.as_str()) == Some(patch_name);

    Some(QuiltInfo {
        position: position + 1, // 1-indexed
        total,
        is_applied,
        is_top,
    })
}

/// Read applied patches from .pc/applied-patches (sibling of patches dir).
fn read_applied_patches(patches_dir: &Path) -> Vec<String> {
    let Some(project_dir) = patches_dir.parent() else {
        return Vec::new();
    };
    let applied_path = project_dir.join(".pc").join("applied-patches");
    let Ok(content) = std::fs::read_to_string(applied_path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup_quilt_project() -> (tempfile::TempDir, PathBuf) {
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
    fn test_patch_applied_top() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\nb.patch\n").unwrap();
        std::fs::write(
            dir.path().join("debian/.pc/applied-patches"),
            "a.patch\nb.patch\n",
        )
        .unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: applied (top, 2/2)"
        );
    }

    #[test]
    fn test_patch_applied_not_top() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\nb.patch\n").unwrap();
        std::fs::write(
            dir.path().join("debian/.pc/applied-patches"),
            "a.patch\nb.patch\n",
        )
        .unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: applied (1/2)"
        );
    }

    #[test]
    fn test_patch_unapplied() {
        let (dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\nb.patch\n").unwrap();
        std::fs::write(dir.path().join("debian/.pc/applied-patches"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: unapplied (2/2)"
        );
    }

    #[test]
    fn test_no_series_file() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 0);
    }

    #[test]
    fn test_patch_not_in_series() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("unknown.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 0);
    }

    #[test]
    fn test_no_applied_patches_file() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();
        // No applied-patches file

        let uri = make_uri(&patches_dir.join("a.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: unapplied (1/1)"
        );
    }

    #[test]
    fn test_lens_range_is_start_of_file() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses[0].range.start, Position::new(0, 0));
        assert_eq!(lenses[0].range.end, Position::new(0, 0));
    }

    #[test]
    fn test_series_with_comments() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch\n# comment\nb.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: unapplied (2/2)"
        );
    }

    #[test]
    fn test_series_with_options() {
        let (_dir, patches_dir) = setup_quilt_project();
        std::fs::write(patches_dir.join("series"), "a.patch -p1\nb.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let lenses = get_patch_quilt_lenses(&uri);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "quilt: unapplied (1/2)"
        );
    }
}
