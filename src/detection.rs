//! File type detection for diff-lsp.
//!
//! Determines whether a file is a patch, a series file, or unknown.
//! Uses both URI path heuristics and filesystem context (e.g. presence
//! of `.pc/` directory indicating a quilt-managed project).

use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types::Uri;

/// The type of file being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A patch/diff file.
    Patch,
    /// A quilt series file.
    Series,
}

/// Detect the file kind from a URI.
///
/// Detection rules:
/// 1. Files named `series` or `series.conf` → Series
/// 2. Files with `.patch` or `.diff` extension → Patch
/// 3. Files under a `patches/` directory that has a sibling `.pc/` → Patch
/// 4. Files under a `patches/` directory that has a sibling `debian/` → Patch
/// 5. Otherwise → Patch (default for this LSP)
pub fn detect_file_kind(uri: &Uri) -> FileKind {
    if let Some(path) = uri_to_path(uri) {
        detect_from_path(&path)
    } else {
        // Fall back to string-based detection
        let uri_str = uri.path().as_str();
        detect_from_uri_path(uri_str)
    }
}

fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let path_str = uri.path().as_str();
    if path_str.starts_with('/') {
        Some(PathBuf::from(path_str))
    } else {
        None
    }
}

fn detect_from_uri_path(path: &str) -> FileKind {
    if path.ends_with("/series") || path.ends_with("/series.conf") {
        FileKind::Series
    } else {
        FileKind::Patch
    }
}

fn detect_from_path(path: &Path) -> FileKind {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Series files by name
    if file_name == "series" || file_name == "series.conf" {
        return FileKind::Series;
    }

    // Explicit patch/diff extensions
    if file_name.ends_with(".patch") || file_name.ends_with(".diff") {
        return FileKind::Patch;
    }

    // Check filesystem context: is this file under a patches/ dir
    // with a quilt .pc/ or debian/ sibling?
    if is_under_quilt_patches(path) {
        return FileKind::Patch;
    }

    // Default
    FileKind::Patch
}

/// Check if a path is inside a `patches/` directory that belongs to a
/// quilt-managed project (has `.pc/` or `debian/` as sibling).
fn is_under_quilt_patches(path: &Path) -> bool {
    for ancestor in path.ancestors().skip(1) {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("patches") {
            let parent = match ancestor.parent() {
                Some(p) => p,
                None => continue,
            };
            if parent.join(".pc").is_dir() || parent.join("debian").is_dir() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_series_by_name() {
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();
        assert_eq!(detect_file_kind(&uri), FileKind::Series);
    }

    #[test]
    fn test_series_conf() {
        let uri: Uri = "file:///project/series.conf".parse().unwrap();
        assert_eq!(detect_file_kind(&uri), FileKind::Series);
    }

    #[test]
    fn test_patch_by_extension() {
        let uri: Uri = "file:///project/patches/fix.patch".parse().unwrap();
        assert_eq!(detect_file_kind(&uri), FileKind::Patch);
    }

    #[test]
    fn test_diff_by_extension() {
        let uri: Uri = "file:///project/changes.diff".parse().unwrap();
        assert_eq!(detect_file_kind(&uri), FileKind::Patch);
    }

    #[test]
    fn test_default_is_patch() {
        let uri: Uri = "file:///project/some-random-file".parse().unwrap();
        assert_eq!(detect_file_kind(&uri), FileKind::Patch);
    }

    #[test]
    fn test_detect_from_uri_path() {
        assert_eq!(detect_from_uri_path("/foo/series"), FileKind::Series);
        assert_eq!(detect_from_uri_path("/foo/series.conf"), FileKind::Series);
        assert_eq!(detect_from_uri_path("/foo/bar.patch"), FileKind::Patch);
        assert_eq!(detect_from_uri_path("/foo/bar"), FileKind::Patch);
    }
}
