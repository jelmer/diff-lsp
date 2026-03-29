//! Smart selection range expansion for patch files.
//!
//! Expands selection: token → line → hunk → file diff → whole patch.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get selection ranges for the given positions.
pub fn get_selection_ranges(
    patch: &Patch,
    source_text: &str,
    positions: &[Position],
) -> Vec<SelectionRange> {
    positions
        .iter()
        .filter_map(|pos| selection_range_at(patch, source_text, *pos))
        .collect()
}

fn selection_range_at(
    patch: &Patch,
    source_text: &str,
    position: Position,
) -> Option<SelectionRange> {
    let offset = try_position_to_offset(source_text, position)?;

    let root = patch.syntax();
    let token = root.token_at_offset(offset).right_biased()?;

    // Collect ancestor nodes from innermost to outermost
    let mut ranges: Vec<Range> = Vec::new();

    // Start with the token itself
    let token_range = text_range_to_lsp_range(source_text, token.text_range());
    ranges.push(token_range);

    // Walk up the tree, collecting interesting nodes
    let mut node = token.parent()?;
    loop {
        match node.kind() {
            SyntaxKind::CONTEXT_LINE
            | SyntaxKind::ADD_LINE
            | SyntaxKind::DELETE_LINE
            | SyntaxKind::OLD_FILE
            | SyntaxKind::NEW_FILE
            | SyntaxKind::HUNK_HEADER
            | SyntaxKind::HUNK
            | SyntaxKind::PATCH_FILE
            | SyntaxKind::ROOT => {
                let r = text_range_to_lsp_range(source_text, node.text_range());
                if ranges.last() != Some(&r) {
                    ranges.push(r);
                }
            }
            _ => {}
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }

    // Build the nested SelectionRange chain (innermost first)
    let mut result: Option<SelectionRange> = None;
    for range in ranges.into_iter().rev() {
        result = Some(SelectionRange {
            range,
            parent: result.map(Box::new),
        });
    }

    result
}
