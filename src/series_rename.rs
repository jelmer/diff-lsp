//! Rename support for quilt series files.
//!
//! Renaming a patch entry updates the name in the series file and renames
//! the patch file on disk using a workspace edit with a RenameFile operation.

use patchkit::edit::series::{PatchEntry, SeriesFile};
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Check if rename is valid at the given position and return the current name and range.
pub fn prepare_rename(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let offset = try_position_to_offset(source_text, position)?;
    let entry = find_patch_entry_at_offset(series, offset)?;
    let token = entry.name_token()?;
    let range = text_range_to_lsp_range(source_text, token.text_range());
    let name = token.text().to_string();

    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range,
        placeholder: name,
    })
}

/// Perform the rename: update series text and rename the file on disk.
pub fn rename(
    series: &SeriesFile,
    source_text: &str,
    position: Position,
    new_name: &str,
    uri: &Uri,
) -> Option<WorkspaceEdit> {
    let offset = try_position_to_offset(source_text, position)?;
    let entry = find_patch_entry_at_offset(series, offset)?;
    let token = entry.name_token()?;
    let old_name = token.text().to_string();
    let range = text_range_to_lsp_range(source_text, token.text_range());

    if old_name == new_name {
        return None;
    }

    // Text edit: replace the patch name in the series file
    let text_edit = TextEdit {
        range,
        new_text: new_name.to_string(),
    };

    // File rename: rename the patch file on disk
    let patches_dir = patches_dir_from_uri(uri)?;
    let old_file_uri = Uri::from_file_path(patches_dir.join(&old_name))?;
    let new_file_uri = Uri::from_file_path(patches_dir.join(new_name))?;

    let rename_file = DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile {
        old_uri: old_file_uri,
        new_uri: new_file_uri,
        options: None,
        annotation_id: None,
    }));

    let text_change = DocumentChangeOperation::Edit(TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: None,
        },
        edits: vec![OneOf::Left(text_edit)],
    });

    Some(WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(vec![text_change, rename_file])),
        ..Default::default()
    })
}

fn find_patch_entry_at_offset(
    series: &SeriesFile,
    offset: text_size::TextSize,
) -> Option<PatchEntry> {
    series.patch_entries().find(|entry| {
        let range = entry.syntax().text_range();
        range.start() <= offset && offset < range.end()
    })
}

fn patches_dir_from_uri(uri: &Uri) -> Option<std::path::PathBuf> {
    let path = uri.to_file_path()?;
    path.parent().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn dummy_patches_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let patches = dir.path().join("debian").join("patches");
        std::fs::create_dir_all(&patches).unwrap();
        dir
    }

    fn dummy_uri(patches_dir: &std::path::Path) -> Uri {
        let series_path = patches_dir.join("debian").join("patches").join("series");
        Uri::from_file_path(series_path).unwrap()
    }

    // --- prepare_rename tests ---

    #[test]
    fn test_prepare_rename_on_patch_name() {
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();

        let result = prepare_rename(&series, text, Position::new(0, 2));
        assert!(result.is_some());
        match result.unwrap() {
            PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                assert_eq!(placeholder, "a.patch");
                assert_eq!(range.start.line, 0);
                assert_eq!(range.start.character, 0);
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn test_prepare_rename_second_entry() {
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();

        let result = prepare_rename(&series, text, Position::new(1, 2));
        assert!(result.is_some());
        match result.unwrap() {
            PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
                assert_eq!(placeholder, "b.patch");
                assert_eq!(range.start.line, 1);
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn test_prepare_rename_on_comment() {
        let text = "# comment\na.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();

        let result = prepare_rename(&series, text, Position::new(0, 2));
        assert_eq!(result, None);
    }

    #[test]
    fn test_prepare_rename_empty() {
        let text = "";
        let parsed = parse_series(text);
        let series = parsed.tree();

        let result = prepare_rename(&series, text, Position::new(0, 0));
        assert_eq!(result, None);
    }

    // --- rename tests ---

    #[test]
    fn test_rename_updates_text_and_file() {
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let dir = dummy_patches_dir();
        let uri = dummy_uri(dir.path());
        let patches_dir = dir.path().join("debian").join("patches");

        let edit = rename(&series, text, Position::new(0, 2), "renamed.patch", &uri).unwrap();

        // Should have document_changes with a text edit and a rename op
        let changes = edit.document_changes.unwrap();
        let DocumentChanges::Operations(ops) = changes else {
            panic!("expected Operations");
        };
        assert_eq!(ops.len(), 2);

        // First op: text edit
        match &ops[0] {
            DocumentChangeOperation::Edit(edit) => {
                assert_eq!(edit.edits.len(), 1);
                match &edit.edits[0] {
                    OneOf::Left(text_edit) => {
                        assert_eq!(text_edit.new_text, "renamed.patch");
                    }
                    _ => panic!("expected Left text edit"),
                }
            }
            _ => panic!("expected Edit"),
        }

        // Second op: file rename
        let expected_old = Uri::from_file_path(patches_dir.join("a.patch")).unwrap();
        let expected_new = Uri::from_file_path(patches_dir.join("renamed.patch")).unwrap();
        match &ops[1] {
            DocumentChangeOperation::Op(ResourceOp::Rename(rename)) => {
                assert_eq!(rename.old_uri, expected_old);
                assert_eq!(rename.new_uri, expected_new);
            }
            _ => panic!("expected Rename op"),
        }
    }

    #[test]
    fn test_rename_same_name_returns_none() {
        let text = "a.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let dir = dummy_patches_dir();
        let uri = dummy_uri(dir.path());

        let result = rename(&series, text, Position::new(0, 2), "a.patch", &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_rename_on_comment_returns_none() {
        let text = "# comment\na.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let dir = dummy_patches_dir();
        let uri = dummy_uri(dir.path());

        let result = rename(&series, text, Position::new(0, 2), "new.patch", &uri);
        assert_eq!(result, None);
    }

    #[test]
    fn test_rename_text_edit_range() {
        let text = "a.patch\nb.patch\n";
        let parsed = parse_series(text);
        let series = parsed.tree();
        let dir = dummy_patches_dir();
        let uri = dummy_uri(dir.path());

        let edit = rename(&series, text, Position::new(1, 2), "c.patch", &uri).unwrap();
        let DocumentChanges::Operations(ops) = edit.document_changes.unwrap() else {
            panic!("expected Operations");
        };
        match &ops[0] {
            DocumentChangeOperation::Edit(edit) => {
                match &edit.edits[0] {
                    OneOf::Left(text_edit) => {
                        // b.patch starts at line 1
                        assert_eq!(text_edit.range.start.line, 1);
                        assert_eq!(text_edit.range.start.character, 0);
                    }
                    _ => panic!("expected Left text edit"),
                }
            }
            _ => panic!("expected Edit"),
        }
    }
}
