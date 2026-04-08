//! Go-to-definition for file paths in patch headers and series entries.
//!
//! - In patch files: when the cursor is on a file path in a `---` or `+++`
//!   line, jumps to the actual source file.
//! - In series files: when the cursor is on a patch name, jumps to the
//!   patch file.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::series::SeriesFile;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use std::path::PathBuf;
use tower_lsp_server::ls_types::*;

use crate::position::try_position_to_offset;

/// Handle go-to-definition for a patch file.
///
/// If the cursor is on a file path in a `---`/`+++` header, returns the
/// location of the actual source file.
pub fn goto_definition(
    patch: &Patch,
    source_text: &str,
    position: Position,
    document_uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let offset = try_position_to_offset(source_text, position)?;

    let root = patch.syntax();
    let token = root.token_at_offset(offset).right_biased()?;

    // Only act on PATH tokens inside OLD_FILE or NEW_FILE nodes
    if token.kind() != SyntaxKind::PATH {
        return None;
    }

    let parent = token.parent()?;
    if parent.kind() != SyntaxKind::OLD_FILE && parent.kind() != SyntaxKind::NEW_FILE {
        return None;
    }

    let path_text = token.text();
    if path_text == "/dev/null" {
        return None;
    }

    // Strip a/ or b/ prefix
    let clean_path = path_text
        .strip_prefix("a/")
        .or_else(|| path_text.strip_prefix("b/"))
        .unwrap_or(path_text);

    // Find the project root and resolve the path
    let project_root = find_project_root(document_uri)?;
    let target_path = project_root.join(clean_path);

    if !target_path.is_file() {
        return None;
    }

    let target_uri = Uri::from_file_path(&target_path)?;

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: target_uri,
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
    }))
}

/// Handle go-to-definition for a series file.
///
/// If the cursor is on a patch name, returns the location of the patch file.
pub fn series_goto_definition(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
    document_uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let offset = try_position_to_offset(source_text, position)?;

    let entry = series.patch_entries().find(|entry| {
        let range = entry.syntax().text_range();
        range.start() <= offset && offset < range.end()
    })?;

    let name = entry.name()?;

    let series_path = document_uri.to_file_path()?;
    let patches_dir = series_path.parent()?;
    let target_path = patches_dir.join(&name);

    if !target_path.is_file() {
        return None;
    }

    let target_uri = Uri::from_file_path(&target_path)?;

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: target_uri,
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
    }))
}

/// Find the project root for a patch file.
///
/// Walks up from the patch file looking for:
/// 1. A quilt project: the parent of the `patches/` directory
/// 2. A git repository: a directory containing `.git`
/// 3. Falls back to the patch file's grandparent (assuming `patches/<file>`)
fn find_project_root(uri: &Uri) -> Option<PathBuf> {
    let path = uri.to_file_path()?;

    for ancestor in path.ancestors().skip(1) {
        // Check if this directory's name is "patches" and its parent is a project root
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("patches") {
            if let Some(parent) = ancestor.parent() {
                if parent.join(".pc").is_dir() || parent.join("debian").is_dir() {
                    return Some(parent.to_path_buf());
                }
            }
        }

        // Check for git root
        if ancestor.join(".git").exists() {
            return Some(ancestor.to_path_buf());
        }
    }

    // Fallback: assume the patch is in a patches/ dir, go up two levels
    path.parent()?.parent().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_goto(
        text: &str,
        line: u32,
        col: u32,
        uri: &Uri,
    ) -> Option<GotoDefinitionResponse> {
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree();
        goto_definition(&patch, text, Position::new(line, col), uri)
    }

    #[test]
    fn test_goto_definition_on_new_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        // Create a .git dir so the project root is found
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        // Create the target file
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("foo.rs"), "fn main() {}").unwrap();

        let text = "--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        // Cursor on the path in +++ line (line 1, on "b/src/foo.rs")
        let result = parse_and_goto(text, 1, 6, &uri);
        assert!(result.is_some());
        let GotoDefinitionResponse::Scalar(loc) = result.unwrap() else {
            panic!("expected scalar response");
        };
        assert_eq!(
            loc.uri.to_string(),
            Uri::from_file_path(src_dir.join("foo.rs"))
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_goto_definition_on_old_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();

        let text = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri);
        assert!(result.is_some());
        let GotoDefinitionResponse::Scalar(loc) = result.unwrap() else {
            panic!("expected scalar response");
        };
        assert_eq!(
            loc.uri.to_string(),
            Uri::from_file_path(dir.path().join("file.txt"))
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_goto_definition_dev_null() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let text = "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+line\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_goto_definition_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let text = "--- a/nonexistent.txt\n+++ b/nonexistent.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_goto_definition_not_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let text = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        // Cursor on the hunk header
        let result = parse_and_goto(text, 2, 3, &uri);
        assert_eq!(result, None);

        // Cursor on a diff line
        let result = parse_and_goto(text, 3, 1, &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_goto_definition_quilt_project() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir_all(dir.path().join("debian").join(".pc")).unwrap();
        // Target file is relative to debian/ (the quilt project root)
        std::fs::write(dir.path().join("debian").join("control"), "").unwrap();

        let text = "--- a/control\n+++ b/control\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri);
        assert!(result.is_some());
        let GotoDefinitionResponse::Scalar(loc) = result.unwrap() else {
            panic!("expected scalar response");
        };
        assert_eq!(
            loc.uri.to_string(),
            Uri::from_file_path(dir.path().join("debian").join("control"))
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_goto_definition_no_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();

        let text = "--- file.txt\n+++ file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri);
        assert!(result.is_some());
    }

    #[test]
    fn test_goto_definition_location_starts_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();

        let text = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        let result = parse_and_goto(text, 0, 6, &uri).unwrap();
        let GotoDefinitionResponse::Scalar(loc) = result else {
            panic!("expected scalar response");
        };
        assert_eq!(loc.range.start, Position::new(0, 0));
        assert_eq!(loc.range.end, Position::new(0, 0));
    }

    // --- find_project_root tests ---

    #[test]
    fn test_find_project_root_git() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        assert_eq!(find_project_root(&uri), Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_quilt() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::create_dir_all(dir.path().join("debian").join(".pc")).unwrap();

        let patch_path = patches_dir.join("test.patch");
        let uri = Uri::from_file_path(&patch_path).unwrap();

        assert_eq!(find_project_root(&uri), Some(dir.path().join("debian")));
    }

    // --- series_goto_definition tests ---

    fn parse_series_and_goto(
        text: &str,
        line: u32,
        col: u32,
        uri: &Uri,
    ) -> Option<GotoDefinitionResponse> {
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        series_goto_definition(&series, text, Position::new(line, col), uri)
    }

    #[test]
    fn test_series_goto_patch_file() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::write(patches_dir.join("a.patch"), "content").unwrap();

        let series_path = patches_dir.join("series");
        let uri = Uri::from_file_path(&series_path).unwrap();

        let result = parse_series_and_goto("a.patch\n", 0, 2, &uri);
        assert!(result.is_some());
        let GotoDefinitionResponse::Scalar(loc) = result.unwrap() else {
            panic!("expected scalar response");
        };
        assert_eq!(
            loc.uri.to_string(),
            Uri::from_file_path(patches_dir.join("a.patch"))
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_series_goto_second_entry() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::write(patches_dir.join("a.patch"), "").unwrap();
        std::fs::write(patches_dir.join("b.patch"), "").unwrap();

        let series_path = patches_dir.join("series");
        let uri = Uri::from_file_path(&series_path).unwrap();

        let result = parse_series_and_goto("a.patch\nb.patch\n", 1, 2, &uri);
        assert!(result.is_some());
        let GotoDefinitionResponse::Scalar(loc) = result.unwrap() else {
            panic!("expected scalar response");
        };
        assert_eq!(
            loc.uri.to_string(),
            Uri::from_file_path(patches_dir.join("b.patch"))
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_series_goto_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_path = patches_dir.join("series");
        let uri = Uri::from_file_path(&series_path).unwrap();

        let result = parse_series_and_goto("missing.patch\n", 0, 2, &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_series_goto_on_comment() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();

        let series_path = patches_dir.join("series");
        let uri = Uri::from_file_path(&series_path).unwrap();

        let result = parse_series_and_goto("# comment\na.patch\n", 0, 2, &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_series_goto_location_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        std::fs::create_dir_all(&patches_dir).unwrap();
        std::fs::write(patches_dir.join("a.patch"), "").unwrap();

        let series_path = patches_dir.join("series");
        let uri = Uri::from_file_path(&series_path).unwrap();

        let result = parse_series_and_goto("a.patch\n", 0, 0, &uri).unwrap();
        let GotoDefinitionResponse::Scalar(loc) = result else {
            panic!("expected scalar response");
        };
        assert_eq!(loc.range.start, Position::new(0, 0));
        assert_eq!(loc.range.end, Position::new(0, 0));
    }
}
