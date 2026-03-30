//! Completion support for quilt series files.
//!
//! Provides completions for patch file names by listing `.patch` and `.diff`
//! files in the patches directory that are not already listed in the series.

use patchkit::edit::series::SeriesFile;
use std::collections::HashSet;
use std::path::Path;
use tower_lsp_server::ls_types::*;

/// Get completion items for a series file.
///
/// Returns patch files from the patches directory that are not already
/// listed in the series.
pub fn get_series_completions(series: &SeriesFile, uri: &Uri) -> Vec<CompletionItem> {
    let Some(patches_dir) = patches_dir_from_uri(uri) else {
        return Vec::new();
    };

    if !patches_dir.is_dir() {
        return Vec::new();
    }

    let listed: HashSet<String> = series.patch_entries().filter_map(|e| e.name()).collect();

    let mut candidates = list_patch_files(&patches_dir);
    candidates.retain(|name| !listed.contains(name));
    candidates.sort();

    candidates
        .into_iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FILE),
            detail: Some("patch file".to_string()),
            insert_text: Some(name),
            ..Default::default()
        })
        .collect()
}

/// Extract the patches directory from a series file URI.
fn patches_dir_from_uri(uri: &Uri) -> Option<std::path::PathBuf> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    path.parent().map(|p| p.to_path_buf())
}

/// List patch files in the patches directory.
///
/// Returns file names, excluding series files and hidden files.
fn list_patch_files(patches_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
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
        if name == "series" || name == "series.conf" || name.starts_with('.') {
            continue;
        }
        files.push(name);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    #[test]
    fn test_completions_excludes_listed_patches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.patch"), "").unwrap();
        std::fs::write(dir.path().join("c.diff"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["b.patch", "c.diff"]);
    }

    #[test]
    fn test_completions_all_listed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        assert_eq!(completions.len(), 0);
    }

    #[test]
    fn test_completions_empty_series() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["a.patch", "b.patch"]);
    }

    #[test]
    fn test_completions_no_patch_files() {
        let dir = tempfile::tempdir().unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        assert_eq!(completions.len(), 0);
    }

    #[test]
    fn test_completions_skips_series_and_hidden() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("series"), "").unwrap();
        std::fs::write(dir.path().join("series.conf"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["a.patch"]);
    }

    #[test]
    fn test_completions_sorted_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("c.patch"), "").unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::write(dir.path().join("b.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["a.patch", "b.patch", "c.patch"]);
    }

    #[test]
    fn test_completions_nonexistent_dir() {
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = "file:///nonexistent/patches/series".parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        assert_eq!(completions.len(), 0);
    }

    #[test]
    fn test_completion_item_kind() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].kind, Some(CompletionItemKind::FILE));
        assert_eq!(completions[0].detail.as_deref(), Some("patch file"));
        assert_eq!(completions[0].insert_text.as_deref(), Some("a.patch"));
    }

    #[test]
    fn test_completions_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), "").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let series_path = dir.path().join("series");
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let completions = get_series_completions(&series, &uri);
        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["a.patch"]);
    }
}
