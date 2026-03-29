//! Folding range generation for patch files.

use patchkit::edit::{Hunk, Patch, PatchFile};
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

/// Generate folding ranges for a patch file.
///
/// Each file diff (`PatchFile`) and each hunk within it gets a folding range.
pub fn generate_folding_ranges(patch: &Patch, source_text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();

    for file in patch.patch_files() {
        add_patch_file_range(&mut ranges, &file, source_text);

        for hunk in file.hunks() {
            add_hunk_range(&mut ranges, &hunk, source_text);
        }
    }

    ranges
}

fn add_patch_file_range(ranges: &mut Vec<FoldingRange>, file: &PatchFile, text: &str) {
    let text_range = file.syntax().text_range();
    let start = offset_to_position(text, text_range.start());
    let end = offset_to_position(text, text_range.end());

    if start.line < end.line {
        ranges.push(FoldingRange {
            start_line: start.line,
            start_character: Some(start.character),
            end_line: end.line,
            end_character: Some(end.character),
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: file
                .old_file()
                .and_then(|f| f.path())
                .map(|p| p.text().to_string()),
        });
    }
}

fn add_hunk_range(ranges: &mut Vec<FoldingRange>, hunk: &Hunk, text: &str) {
    let text_range = hunk.syntax().text_range();
    let start = offset_to_position(text, text_range.start());
    let end = offset_to_position(text, text_range.end());

    if start.line < end.line {
        ranges.push(FoldingRange {
            start_line: start.line,
            start_character: Some(start.character),
            end_line: end.line,
            end_character: Some(end.character),
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
}
