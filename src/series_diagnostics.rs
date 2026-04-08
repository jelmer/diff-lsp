//! Diagnostic generation for quilt series files.
//!
//! Checks for:
//! - Duplicate patch entries in the series
//! - Patch files listed in series but missing from the patches directory
//! - Patch files present in the patches directory but not listed in series

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

fn make_diagnostic(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("diff-lsp".to_string()),
        message,
        ..Default::default()
    }
}

/// Extract the patches directory path from a series file URI.
///
/// The series file is typically at `<project>/debian/patches/series` or
/// `<project>/patches/series`, so the patches directory is its parent.
fn patches_dir_from_uri(uri: &Uri) -> Option<PathBuf> {
    let path = uri.to_file_path()?;
    path.parent().map(|p| p.to_path_buf())
}

/// List patch files (`.patch`, `.diff`, or extensionless files) in the patches directory.
///
/// Returns file names relative to the patches directory.
fn list_patch_files(patches_dir: &Path) -> HashSet<String> {
    let mut files = HashSet::new();
    let Ok(entries) = std::fs::read_dir(patches_dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip the series file itself and hidden files
        if name == "series" || name == "series.conf" || name.starts_with('.') {
            continue;
        }
        files.insert(name);
    }
    files
}

/// Collect all diagnostics for a series file.
pub fn get_series_diagnostics(
    source_text: &str,
    series: &SeriesFile,
    uri: &Uri,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    diagnostics.extend(check_duplicate_entries(source_text, series));

    if let Some(patches_dir) = patches_dir_from_uri(uri) {
        if patches_dir.is_dir() {
            let patch_files = list_patch_files(&patches_dir);
            diagnostics.extend(check_missing_patches(source_text, series, &patches_dir));
            diagnostics.extend(check_unlisted_patches(source_text, series, &patch_files));
        }
    }

    diagnostics
}

/// Warn about duplicate patch names in the series file.
fn check_duplicate_entries(source_text: &str, series: &SeriesFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Range> = HashMap::new();

    for entry in series.patch_entries() {
        let Some(name) = entry.name() else {
            continue;
        };
        let range = text_range_to_lsp_range(source_text, entry.syntax().text_range());

        if let Some(first_range) = seen.get(&name) {
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "duplicate-series-entry",
                format!(
                    "patch '{}' already listed on line {}",
                    name,
                    first_range.start.line + 1
                ),
            ));
        } else {
            seen.insert(name, range);
        }
    }

    diagnostics
}

/// Warn about patches listed in series but not found in the patches directory.
fn check_missing_patches(
    source_text: &str,
    series: &SeriesFile,
    patches_dir: &Path,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for entry in series.patch_entries() {
        let Some(name) = entry.name() else {
            continue;
        };
        let patch_path = patches_dir.join(&name);
        if !patch_path.is_file() {
            let range = text_range_to_lsp_range(source_text, entry.syntax().text_range());
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "missing-patch-file",
                format!("patch file '{}' not found", name),
            ));
        }
    }

    diagnostics
}

/// Warn about patch files in the directory that aren't listed in the series.
///
/// These diagnostics are reported at the end of the file (line 0, col 0 as a
/// fallback) since there's no specific location in the series file to attach them to.
fn check_unlisted_patches(
    source_text: &str,
    series: &SeriesFile,
    patch_files: &HashSet<String>,
) -> Vec<Diagnostic> {
    let listed: HashSet<String> = series.patch_entries().filter_map(|e| e.name()).collect();

    let mut unlisted: Vec<&String> = patch_files.difference(&listed).collect();
    unlisted.sort();

    // Report at the end of the file
    let end_offset = text_size::TextSize::from(source_text.len() as u32);
    let end_pos = crate::position::offset_to_position(source_text, end_offset);
    let range = Range::new(end_pos, end_pos);

    unlisted
        .into_iter()
        .map(|name| {
            make_diagnostic(
                range,
                DiagnosticSeverity::HINT,
                "unlisted-patch-file",
                format!("patch file '{}' exists but is not listed in series", name),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    // --- check_duplicate_entries tests ---

    #[test]
    fn test_no_duplicates() {
        let text = "0001-fix.patch\n0002-feature.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_duplicate_entry() {
        let text = "0001-fix.patch\n0002-feature.patch\n0001-fix.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("duplicate-series-entry".to_string()))
        );
        assert!(diags[0].message.contains("0001-fix.patch"));
        assert!(diags[0].message.contains("line 1"));
        // The duplicate is on line 2 (0-indexed)
        assert_eq!(diags[0].range.start.line, 2);
    }

    #[test]
    fn test_multiple_duplicates() {
        let text = "a.patch\nb.patch\na.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("a.patch"));
        assert!(diags[1].message.contains("b.patch"));
    }

    #[test]
    fn test_duplicates_with_comments() {
        let text = "a.patch\n# comment\na.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("a.patch"));
    }

    #[test]
    fn test_empty_series_no_duplicates() {
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_comments_only_no_duplicates() {
        let text = "# comment 1\n# comment 2\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert!(diags.is_empty());
    }

    // --- check_missing_patches tests (using tempdir) ---

    #[test]
    fn test_missing_patch_file() {
        let dir = tempfile::tempdir().unwrap();
        // Create one patch file but not the other
        std::fs::write(dir.path().join("0001-fix.patch"), "").unwrap();

        let text = "0001-fix.patch\n0002-missing.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_missing_patches(text, &series, dir.path());
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("missing-patch-file".to_string()))
        );
        assert!(diags[0].message.contains("0002-missing.patch"));
    }

    #[test]
    fn test_all_patches_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.patch"), "").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_missing_patches(text, &series, dir.path());
        assert!(diags.is_empty());
    }

    #[test]
    fn test_all_patches_missing() {
        let dir = tempfile::tempdir().unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_missing_patches(text, &series, dir.path());
        assert_eq!(diags.len(), 2);
    }

    // --- check_unlisted_patches tests ---

    #[test]
    fn test_unlisted_patch_files() {
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let mut patch_files = HashSet::new();
        patch_files.insert("a.patch".to_string());
        patch_files.insert("b.patch".to_string());
        patch_files.insert("c.diff".to_string());

        let diags = check_unlisted_patches(text, &series, &patch_files);
        assert_eq!(diags.len(), 2);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("unlisted-patch-file".to_string()))
        );
        // Sorted alphabetically
        assert!(diags[0].message.contains("b.patch"));
        assert!(diags[1].message.contains("c.diff"));
    }

    #[test]
    fn test_no_unlisted_patches() {
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let mut patch_files = HashSet::new();
        patch_files.insert("a.patch".to_string());
        patch_files.insert("b.patch".to_string());

        let diags = check_unlisted_patches(text, &series, &patch_files);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_unlisted_with_empty_series() {
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let mut patch_files = HashSet::new();
        patch_files.insert("orphan.patch".to_string());

        let diags = check_unlisted_patches(text, &series, &patch_files);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("orphan.patch"));
    }

    #[test]
    fn test_unlisted_severity_is_hint() {
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let mut patch_files = HashSet::new();
        patch_files.insert("orphan.patch".to_string());

        let diags = check_unlisted_patches(text, &series, &patch_files);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::HINT));
    }

    // --- list_patch_files tests ---

    #[test]
    fn test_list_patch_files_basic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.diff"), "").unwrap();
        std::fs::write(dir.path().join("series"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();

        let files = list_patch_files(dir.path());
        assert!(files.contains("a.patch"));
        assert!(files.contains("b.diff"));
        assert!(!files.contains("series"));
        assert!(!files.contains(".hidden"));
    }

    #[test]
    fn test_list_patch_files_skips_series_conf() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("series.conf"), "").unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();

        let files = list_patch_files(dir.path());
        assert!(files.contains("a.patch"));
        assert!(!files.contains("series.conf"));
    }

    #[test]
    fn test_list_patch_files_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();

        let files = list_patch_files(dir.path());
        assert!(files.contains("a.patch"));
        assert!(!files.contains("subdir"));
    }

    #[test]
    fn test_list_patch_files_nonexistent_dir() {
        let files = list_patch_files(Path::new("/nonexistent/path"));
        assert!(files.is_empty());
    }

    // --- patches_dir_from_uri tests ---

    #[test]
    fn test_patches_dir_from_uri() {
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();
        let dir = patches_dir_from_uri(&uri).unwrap();
        assert_eq!(dir, PathBuf::from("/project/debian/patches"));
    }

    #[test]
    fn test_patches_dir_from_uri_nested() {
        let uri: Uri = "file:///a/b/c/series".parse().unwrap();
        let dir = patches_dir_from_uri(&uri).unwrap();
        assert_eq!(dir, PathBuf::from("/a/b/c"));
    }

    // --- Integration tests with get_series_diagnostics ---

    #[test]
    fn test_full_diagnostics_with_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("orphan.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        std::fs::write(&series_path, "a.patch\nmissing.patch\na.patch\n").unwrap();

        let text = "a.patch\nmissing.patch\na.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = Uri::from_file_path(&series_path).unwrap();

        let diags = get_series_diagnostics(text, &series, &uri);

        // Should have: 1 duplicate, 1 missing, 1 unlisted
        let duplicate_count = diags
            .iter()
            .filter(|d| {
                d.code == Some(NumberOrString::String("duplicate-series-entry".to_string()))
            })
            .count();
        let missing_count = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("missing-patch-file".to_string())))
            .count();
        let unlisted_count = diags
            .iter()
            .filter(|d| d.code == Some(NumberOrString::String("unlisted-patch-file".to_string())))
            .count();

        assert_eq!(duplicate_count, 1);
        assert_eq!(missing_count, 1);
        assert_eq!(unlisted_count, 1);
    }

    #[test]
    fn test_clean_series_no_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        std::fs::write(&series_path, "a.patch\nb.patch\n").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = Uri::from_file_path(&series_path).unwrap();

        let diags = get_series_diagnostics(text, &series, &uri);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_duplicate_severity_is_error() {
        let text = "a.patch\na.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_duplicate_entries(text, &series);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn test_missing_severity_is_warning() {
        let dir = tempfile::tempdir().unwrap();
        let text = "missing.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let diags = check_missing_patches(text, &series, dir.path());
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
    }
}
