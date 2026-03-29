//! Folding range generation for patch files.
//!
//! Supports unified diffs (PatchFile, Hunk), context diffs (ContextDiffFile,
//! ContextHunk), ed diffs (EdCommand), and normal diffs (NormalHunk).

use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

/// Generate folding ranges for a patch file.
pub fn generate_folding_ranges(patch: &Patch, source_text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();

    // Unified diffs
    for file in patch.patch_files() {
        add_node_range(&mut ranges, file.syntax(), source_text, file.path());
        for hunk in file.hunks() {
            add_node_range(&mut ranges, hunk.syntax(), source_text, None);
        }
    }

    // Context diffs
    for file in patch.context_diff_files() {
        let name = file
            .old_file()
            .and_then(|f| f.path())
            .map(|t| t.text().to_string());
        add_node_range(&mut ranges, file.syntax(), source_text, name);
        for hunk in file.hunks() {
            add_node_range(&mut ranges, hunk.syntax(), source_text, None);
        }
    }

    // Normal diffs
    for hunk in patch.normal_hunks() {
        add_node_range(&mut ranges, hunk.syntax(), source_text, None);
    }

    // Ed diffs
    for cmd in patch.ed_commands() {
        add_node_range(&mut ranges, cmd.syntax(), source_text, None);
    }

    ranges
}

fn add_node_range(
    ranges: &mut Vec<FoldingRange>,
    node: &rowan::SyntaxNode<patchkit::edit::Lang>,
    text: &str,
    collapsed_text: Option<String>,
) {
    let text_range = node.text_range();
    let start = offset_to_position(text, text_range.start());
    let end = offset_to_position(text, text_range.end());

    if start.line < end.line {
        ranges.push(FoldingRange {
            start_line: start.line,
            start_character: Some(start.character),
            end_line: end.line,
            end_character: Some(end.character),
            kind: Some(FoldingRangeKind::Region),
            collapsed_text,
        });
    }
}
