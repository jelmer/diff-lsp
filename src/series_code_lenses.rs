//! Code lens generation for quilt series files.
//!
//! Shows the applied/unapplied status of each patch by reading the
//! `.pc/applied-patches` file from the quilt metadata directory.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use std::collections::HashSet;
use std::path::Path;
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

/// Read the list of applied patches from `.pc/applied-patches`.
///
/// The `.pc/` directory is expected to be a sibling of the patches directory.
/// For example, if the series file is at `/project/debian/patches/series`,
/// the applied-patches file is at `/project/debian/.pc/applied-patches`.
fn read_applied_patches(series_uri: &Uri) -> Option<Vec<String>> {
    let path_str = series_uri.path().as_str();
    let series_path = Path::new(path_str);
    let patches_dir = series_path.parent()?;

    // .pc/ is a sibling of patches/, so go up one more level
    let project_dir = patches_dir.parent()?;
    let applied_path = project_dir.join(".pc").join("applied-patches");

    let content = std::fs::read_to_string(applied_path).ok()?;
    Some(
        content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
    )
}

/// Generate code lenses showing applied/unapplied status for series entries.
pub fn get_series_code_lenses(series: &SeriesFile, source_text: &str, uri: &Uri) -> Vec<CodeLens> {
    let applied = read_applied_patches(uri);
    let Some(applied_list) = applied else {
        // No .pc/applied-patches found — no status to show
        return Vec::new();
    };

    let applied_set: HashSet<&str> = applied_list.iter().map(|s| s.as_str()).collect();
    let top_patch = applied_list.last().map(|s| s.as_str());

    let total = series.patch_entries().count();
    let mut lenses = Vec::new();
    let mut index = 0;

    for entry in series.patch_entries() {
        index += 1;
        let Some(name) = entry.name() else {
            continue;
        };

        let range = text_range_to_lsp_range(source_text, entry.syntax().text_range());

        let label = if Some(name.as_str()) == top_patch {
            format!("applied (top, {}/{})", index, total)
        } else if applied_set.contains(name.as_str()) {
            format!("applied ({}/{})", index, total)
        } else {
            format!("unapplied ({}/{})", index, total)
        };

        lenses.push(CodeLens {
            range,
            command: Some(Command {
                title: label,
                command: String::new(),
                arguments: None,
            }),
            data: None,
        });
    }

    lenses
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Find the `.pc/` directory for a series file URI.
    fn find_pc_dir(series_uri: &Uri) -> Option<PathBuf> {
        let path_str = series_uri.path().as_str();
        let series_path = Path::new(path_str);
        let patches_dir = series_path.parent()?;
        let project_dir = patches_dir.parent()?;
        let pc_dir = project_dir.join(".pc");
        if pc_dir.is_dir() {
            return Some(pc_dir);
        }
        None
    }

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    // --- read_applied_patches tests ---

    #[test]
    fn test_read_applied_patches() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(
            pc_dir.join("applied-patches"),
            "0001-fix.patch\n0002-feature.patch\n",
        )
        .unwrap();

        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let applied = read_applied_patches(&uri).unwrap();
        assert_eq!(applied, vec!["0001-fix.patch", "0002-feature.patch"]);
    }

    #[test]
    fn test_read_applied_patches_no_pc_dir() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        assert!(read_applied_patches(&uri).is_none());
    }

    #[test]
    fn test_read_applied_patches_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let applied = read_applied_patches(&uri).unwrap();
        assert!(applied.is_empty());
    }

    // --- find_pc_dir tests ---

    #[test]
    fn test_find_pc_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();

        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        assert_eq!(find_pc_dir(&uri).unwrap(), pc_dir);
    }

    #[test]
    fn test_find_pc_dir_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        assert!(find_pc_dir(&uri).is_none());
    }

    // --- get_series_code_lenses tests ---

    #[test]
    fn test_lenses_all_applied() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\nb.patch\n").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert_eq!(lenses.len(), 2);
        assert_eq!(lenses[0].command.as_ref().unwrap().title, "applied (1/2)");
        assert_eq!(
            lenses[1].command.as_ref().unwrap().title,
            "applied (top, 2/2)"
        );
    }

    #[test]
    fn test_lenses_partially_applied() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\n").unwrap();

        let text = "a.patch\nb.patch\nc.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert_eq!(lenses.len(), 3);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "applied (top, 1/3)"
        );
        assert_eq!(lenses[1].command.as_ref().unwrap().title, "unapplied (2/3)");
        assert_eq!(lenses[2].command.as_ref().unwrap().title, "unapplied (3/3)");
    }

    #[test]
    fn test_lenses_none_applied() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert_eq!(lenses.len(), 2);
        assert_eq!(lenses[0].command.as_ref().unwrap().title, "unapplied (1/2)");
        assert_eq!(lenses[1].command.as_ref().unwrap().title, "unapplied (2/2)");
    }

    #[test]
    fn test_lenses_no_pc_dir() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert!(lenses.is_empty());
    }

    #[test]
    fn test_lenses_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "a.patch\n").unwrap();

        let text = "a.patch\n# comment\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        // Only patch entries get lenses, not comments
        assert_eq!(lenses.len(), 2);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "applied (top, 1/2)"
        );
        assert_eq!(lenses[1].command.as_ref().unwrap().title, "unapplied (2/2)");
    }

    #[test]
    fn test_lenses_empty_series() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert!(lenses.is_empty());
    }

    #[test]
    fn test_lenses_position_on_correct_line() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        let pc_dir = dir.path().join("debian").join(".pc");
        std::fs::create_dir_all(&pc_dir).unwrap();
        std::fs::write(pc_dir.join("applied-patches"), "").unwrap();

        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let lenses = get_series_code_lenses(&series, text, &uri);
        assert_eq!(lenses[0].range.start.line, 0);
        assert_eq!(lenses[1].range.start.line, 1);
    }
}
