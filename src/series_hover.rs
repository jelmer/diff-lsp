//! Hover information for quilt series files.
//!
//! When hovering over a patch name in a series file, shows the patch
//! description (leading text before diff content) and change statistics.

use patchkit::edit::series::{PatchEntry, SeriesFile};
use rowan::ast::AstNode;
use std::path::Path;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get hover information for a position in a series file.
pub fn get_series_hover(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
    uri: &Uri,
) -> Option<Hover> {
    let offset = try_position_to_offset(source_text, position)?;

    // Find which patch entry the cursor is on
    let entry = find_patch_entry_at_offset(series, offset)?;
    let name = entry.name()?;
    let range = text_range_to_lsp_range(source_text, entry.syntax().text_range());

    // Resolve the patch file path
    let patches_dir = patches_dir_from_uri(uri)?;
    let patch_path = patches_dir.join(&name);

    let content = std::fs::read_to_string(&patch_path).ok()?;
    let hover_text = build_hover_text(&name, &content);

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_text,
        }),
        range: Some(range),
    })
}

/// Find the PatchEntry at a given byte offset.
fn find_patch_entry_at_offset(
    series: &SeriesFile,
    offset: text_size::TextSize,
) -> Option<PatchEntry> {
    series.patch_entries().find(|entry| {
        let range = entry.syntax().text_range();
        range.start() <= offset && offset <= range.end()
    })
}

/// Extract the patches directory from a series file URI.
fn patches_dir_from_uri(uri: &Uri) -> Option<std::path::PathBuf> {
    let path_str = uri.path().as_str();
    let path = Path::new(path_str);
    path.parent().map(|p| p.to_path_buf())
}

/// Extract the description (preamble) from patch file content.
///
/// The description is the text before the first diff marker line
/// (`---`, `diff --git`, `Index:`, etc.).
fn extract_description(content: &str) -> Option<String> {
    let mut lines = Vec::new();

    for line in content.lines() {
        if line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("diff --git")
            || line.starts_with("Index: ")
            || line.starts_with("@@ ")
        {
            break;
        }
        lines.push(line);
    }

    // Trim trailing empty lines
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        None
    } else {
        // Limit to first 10 lines
        let display_lines: Vec<&str> = lines.iter().take(10).copied().collect();
        let mut result = display_lines.join("\n");
        if lines.len() > 10 {
            result.push_str("\n...");
        }
        Some(result)
    }
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

/// Build the hover markdown text from a patch name and its content.
fn build_hover_text(name: &str, content: &str) -> String {
    let mut parts = Vec::new();

    parts.push(format!("**{}**", name));

    if let Some(desc) = extract_description(content) {
        parts.push(format!("\n{}", desc));
    }

    let (additions, deletions, file_count) = compute_stats(content);
    if file_count > 0 {
        parts.push(format!(
            "\n---\n{} file{}, +{} −{}",
            file_count,
            if file_count == 1 { "" } else { "s" },
            additions,
            deletions
        ));
    }

    parts.join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    // --- extract_description tests ---

    #[test]
    fn test_extract_description_with_preamble() {
        let content = "\
Subject: Fix the widget

This patch fixes a bug in the widget module.

--- a/widget.rs
+++ b/widget.rs
@@ -1 +1 @@
-old
+new
";
        assert_eq!(
            extract_description(content),
            Some(
                "Subject: Fix the widget\n\nThis patch fixes a bug in the widget module."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_extract_description_no_preamble() {
        let content = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        assert_eq!(extract_description(content), None);
    }

    #[test]
    fn test_extract_description_diff_git() {
        let content = "\
Description: something
diff --git a/file b/file
--- a/file
+++ b/file
";
        assert_eq!(
            extract_description(content),
            Some("Description: something".to_string())
        );
    }

    #[test]
    fn test_extract_description_truncated() {
        let mut content = String::new();
        for i in 0..15 {
            content.push_str(&format!("line {}\n", i));
        }
        content.push_str("--- a/file\n+++ b/file\n");

        assert_eq!(
            extract_description(&content),
            Some(
                "line 0\nline 1\nline 2\nline 3\nline 4\n\
                 line 5\nline 6\nline 7\nline 8\nline 9\n..."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_extract_description_strips_trailing_blanks() {
        let content = "Subject: Fix\n\n\n--- a/file\n";
        assert_eq!(
            extract_description(content),
            Some("Subject: Fix".to_string())
        );
    }

    // --- compute_stats tests ---

    #[test]
    fn test_compute_stats_basic() {
        let content = "\
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 ctx
-old
+new
";
        assert_eq!(compute_stats(content), (1, 1, 1));
    }

    #[test]
    fn test_compute_stats_multiple_files() {
        let content = "\
--- a/a.txt
+++ b/a.txt
@@ -1 +1,2 @@
 ctx
+added
--- a/b.txt
+++ b/b.txt
@@ -1,2 +1 @@
 ctx
-removed
";
        assert_eq!(compute_stats(content), (1, 1, 2));
    }

    #[test]
    fn test_compute_stats_empty() {
        assert_eq!(compute_stats(""), (0, 0, 0));
    }

    // --- build_hover_text tests ---

    #[test]
    fn test_build_hover_text_full() {
        let content = "\
Subject: Fix bug

--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 ctx
-old
+new
";
        assert_eq!(
            build_hover_text("fix-bug.patch", content),
            "**fix-bug.patch**\nSubject: Fix bug\n---\n1 file, +1 −1"
        );
    }

    #[test]
    fn test_build_hover_text_no_description() {
        let content = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        assert_eq!(
            build_hover_text("patch.patch", content),
            "**patch.patch**\n---\n1 file, +1 −1"
        );
    }

    #[test]
    fn test_build_hover_text_multiple_files() {
        let content = "\
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-a
+b
--- a/c.txt
+++ b/c.txt
@@ -1 +1 @@
-c
+d
";
        assert_eq!(
            build_hover_text("multi.patch", content),
            "**multi.patch**\n---\n2 files, +2 −2"
        );
    }

    #[test]
    fn test_build_hover_text_no_diff_content() {
        let content = "Just some text\nno diff here\n";
        assert_eq!(
            build_hover_text("empty.patch", content),
            "**empty.patch**\nJust some text\nno diff here"
        );
    }

    // --- get_series_hover integration tests ---

    #[test]
    fn test_hover_on_patch_entry() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let patch_content = "\
Subject: Fix things

--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 ctx
-old
+new
";
        std::fs::write(patches_dir.join("fix.patch"), patch_content).unwrap();

        let series_text = "fix.patch\n";
        let parsed = parse_series(series_text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let hover = get_series_hover(&series, series_text, Position::new(0, 2), &uri).unwrap();
        assert_eq!(
            hover.contents,
            HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**fix.patch**\nSubject: Fix things\n---\n1 file, +1 −1".to_string(),
            })
        );
        assert_eq!(hover.range.unwrap().start.line, 0);
    }

    #[test]
    fn test_hover_on_comment_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_text = "# comment\nfix.patch\n";
        let parsed = parse_series(series_text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        assert_eq!(
            get_series_hover(&series, series_text, Position::new(0, 2), &uri),
            None
        );
    }

    #[test]
    fn test_hover_on_missing_patch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_text = "nonexistent.patch\n";
        let parsed = parse_series(series_text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        assert_eq!(
            get_series_hover(&series, series_text, Position::new(0, 5), &uri),
            None
        );
    }

    #[test]
    fn test_hover_second_entry() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::write(
            patches_dir.join("a.patch"),
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-x\n+y\n",
        )
        .unwrap();
        std::fs::write(
            patches_dir.join("b.patch"),
            "Desc B\n--- a/g\n+++ b/g\n@@ -1 +1 @@\n-1\n+2\n",
        )
        .unwrap();

        let series_text = "a.patch\nb.patch\n";
        let parsed = parse_series(series_text);
        let series = parsed.tree();
        let series_path = patches_dir.join("series");
        let uri: Uri = format!("file://{}", series_path.display()).parse().unwrap();

        let hover = get_series_hover(&series, series_text, Position::new(1, 2), &uri).unwrap();
        assert_eq!(
            hover.contents,
            HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**b.patch**\nDesc B\n---\n1 file, +1 −1".to_string(),
            })
        );
        assert_eq!(hover.range.unwrap().start.line, 1);
    }
}
