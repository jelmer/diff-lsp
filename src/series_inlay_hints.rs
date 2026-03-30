//! Inlay hints for quilt series files.
//!
//! Shows +/- change statistics inline next to each patch name by reading
//! and parsing the actual patch files from disk.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use std::path::Path;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_lsp_range_to_text_range};

/// Generate inlay hints for series entries within the given range.
pub fn get_series_inlay_hints(
    series: &SeriesFile,
    source_text: &str,
    range: Range,
    uri: &Uri,
) -> Vec<InlayHint> {
    let text_range = try_lsp_range_to_text_range(source_text, &range);
    let Some(patches_dir) = patches_dir_from_uri(uri) else {
        return Vec::new();
    };

    let mut hints = Vec::new();

    for entry in series.patch_entries() {
        let entry_range = entry.syntax().text_range();

        // Skip entries outside the requested range
        if let Some(ref tr) = text_range {
            if entry_range.end() <= tr.start() || entry_range.start() > tr.end() {
                continue;
            }
        }

        let Some(name) = entry.name() else {
            continue;
        };

        let patch_path = patches_dir.join(&name);
        let Ok(content) = std::fs::read_to_string(&patch_path) else {
            continue;
        };

        let (additions, deletions, file_count) = compute_stats(&content);
        if additions == 0 && deletions == 0 {
            continue;
        }

        let lsp_range = text_range_to_lsp_range(source_text, entry_range);
        let label = format!(
            " {} file{}, +{} −{}",
            file_count,
            if file_count == 1 { "" } else { "s" },
            additions,
            deletions
        );

        hints.push(InlayHint {
            position: lsp_range.end,
            label: InlayHintLabel::String(label),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: None,
            data: None,
        });
    }

    hints
}

/// Compute change statistics from patch content.
fn compute_stats(content: &str) -> (u32, u32, usize) {
    let parsed = patchkit::edit::parse(content);
    let patch = parsed.tree();

    let mut additions = 0u32;
    let mut deletions = 0u32;
    let mut file_count = 0usize;

    for file in patch.patch_files() {
        file_count += 1;
        for hunk in file.hunks() {
            let stats = hunk.stats();
            additions += stats.additions;
            deletions += stats.deletions;
        }
    }

    (additions, deletions, file_count)
}

/// Extract the patches directory from a series file URI.
fn patches_dir_from_uri(uri: &Uri) -> Option<std::path::PathBuf> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    path.parent().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn full_range() -> Range {
        Range::new(Position::new(0, 0), Position::new(10000, 0))
    }

    fn make_uri(path: &Path) -> Uri {
        format!("file://{}", path.display()).parse().unwrap()
    }

    #[test]
    fn test_hints_with_stats() {
        let dir = tempfile::tempdir().unwrap();
        let patch_content = "\
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,3 @@
 ctx
-old
+new1
+new2
";
        std::fs::write(dir.path().join("a.patch"), patch_content).unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, " 1 file, +2 −1"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn test_hints_multiple_files_in_patch() {
        let dir = tempfile::tempdir().unwrap();
        let patch_content = "\
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-a
+b
--- a/c.txt
+++ b/c.txt
@@ -1 +1,2 @@
-c
+d
+e
";
        std::fs::write(dir.path().join("multi.patch"), patch_content).unwrap();

        let series_path = dir.path().join("series");
        let text = "multi.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, " 2 files, +3 −2"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn test_no_hint_for_missing_patch() {
        let dir = tempfile::tempdir().unwrap();

        let series_path = dir.path().join("series");
        let text = "missing.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn test_no_hint_for_empty_patch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("empty.patch"), "").unwrap();

        let series_path = dir.path().join("series");
        let text = "empty.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints.len(), 0);
    }

    #[test]
    fn test_hints_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.patch"),
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-x\n+y\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.patch"),
            "--- a/g\n+++ b/g\n@@ -1 +1,2 @@\n-a\n+b\n+c\n",
        )
        .unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints.len(), 2);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert_eq!(s, " 1 file, +1 −1"),
            _ => panic!("expected string label"),
        }
        match &hints[1].label {
            InlayHintLabel::String(s) => assert_eq!(s, " 1 file, +2 −1"),
            _ => panic!("expected string label"),
        }
    }

    #[test]
    fn test_hint_position_at_end_of_entry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.patch"),
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-x\n+y\n",
        )
        .unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        // Hint should be at end of the entry (line 1, col 0 due to trailing newline)
        assert_eq!(hints[0].position.line, 1);
        assert_eq!(hints[0].position.character, 0);
    }

    #[test]
    fn test_hint_kind_is_parameter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.patch"),
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-x\n+y\n",
        )
        .unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        let hints = get_series_inlay_hints(&series, text, full_range(), &uri);
        assert_eq!(hints[0].kind, Some(InlayHintKind::PARAMETER));
        assert_eq!(hints[0].padding_left, Some(true));
    }

    #[test]
    fn test_range_filtering() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.patch"),
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-x\n+y\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.patch"),
            "--- a/g\n+++ b/g\n@@ -1 +1 @@\n-a\n+b\n",
        )
        .unwrap();

        let series_path = dir.path().join("series");
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let uri = make_uri(&series_path);

        // Only request first line (end at start of second line)
        let range = Range::new(Position::new(0, 0), Position::new(0, 7));
        let hints = get_series_inlay_hints(&series, text, range, &uri);
        assert_eq!(hints.len(), 1);
    }
}
