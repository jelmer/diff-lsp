//! Diagnostics for patch files in a quilt project.
//!
//! Warns if a patch file exists in the patches directory but is not
//! listed in the series file.

use std::path::Path;
use tower_lsp_server::ls_types::*;

/// Check if a patch file is listed in its project's series file.
///
/// Returns a diagnostic if the patch exists in a patches directory
/// with a series file but is not listed in it.
pub fn get_patch_quilt_diagnostics(uri: &Uri) -> Vec<Diagnostic> {
    let Some(patch_name) = extract_patch_name(uri) else {
        return Vec::new();
    };

    let Some(series_content) = read_series_file(uri) else {
        return Vec::new();
    };

    let listed = series_content
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .any(|l| l.split_whitespace().next() == Some(&patch_name));

    if listed {
        return Vec::new();
    }

    vec![Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        severity: Some(DiagnosticSeverity::HINT),
        code: Some(NumberOrString::String("not-in-series".to_string())),
        source: Some("diff-lsp".to_string()),
        message: format!("patch '{}' is not listed in the series file", patch_name),
        ..Default::default()
    }]
}

/// Extract the file name from a patch file URI.
fn extract_patch_name(uri: &Uri) -> Option<String> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    let name = path.file_name()?.to_str()?;
    Some(name.to_string())
}

/// Read the series file from the same directory as the patch.
fn read_series_file(uri: &Uri) -> Option<String> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    let patches_dir = path.parent()?;
    let series_path = patches_dir.join("series");
    std::fs::read_to_string(series_path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_patches_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        (dir, patches_dir)
    }

    fn make_uri(path: &Path) -> Uri {
        format!("file://{}", path.display()).parse().unwrap()
    }

    #[test]
    fn test_patch_not_in_series() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "a.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("b.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("not-in-series".to_string()))
        );
        assert_eq!(
            diags[0].message,
            "patch 'b.patch' is not listed in the series file"
        );
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::HINT));
    }

    #[test]
    fn test_patch_in_series() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "a.patch\nb.patch\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn test_patch_in_series_with_options() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "a.patch -p1\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn test_no_series_file() {
        let (_dir, patches_dir) = setup_patches_dir();
        // No series file

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn test_empty_series() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "patch 'a.patch' is not listed in the series file"
        );
    }

    #[test]
    fn test_series_with_comments_only() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "# comment\n").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_diagnostic_range_is_start() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags[0].range.start, Position::new(0, 0));
        assert_eq!(diags[0].range.end, Position::new(0, 0));
    }

    #[test]
    fn test_diagnostic_source() {
        let (_dir, patches_dir) = setup_patches_dir();
        std::fs::write(patches_dir.join("series"), "").unwrap();

        let uri = make_uri(&patches_dir.join("a.patch"));
        let diags = get_patch_quilt_diagnostics(&uri);
        assert_eq!(diags[0].source, Some("diff-lsp".to_string()));
    }
}
