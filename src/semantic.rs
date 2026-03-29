//! Semantic token generation for syntax highlighting.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

// Token type indices (must match the legend in main.rs)
const TOKEN_FILE_HEADER: u32 = 0;
const TOKEN_HUNK_HEADER: u32 = 1;
const TOKEN_ADD_LINE: u32 = 2;
const TOKEN_DELETE_LINE: u32 = 3;
const TOKEN_CONTEXT_LINE: u32 = 4;

/// Generate semantic tokens for a patch file.
pub fn generate_semantic_tokens(patch: &Patch, source_text: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    for file in patch.patch_files() {
        // Old file header
        if let Some(old_file) = file.old_file() {
            emit_node(
                source_text,
                &old_file.syntax().text_range(),
                TOKEN_FILE_HEADER,
                &mut tokens,
                &mut prev_line,
                &mut prev_col,
            );
        }

        // New file header
        if let Some(new_file) = file.new_file() {
            emit_node(
                source_text,
                &new_file.syntax().text_range(),
                TOKEN_FILE_HEADER,
                &mut tokens,
                &mut prev_line,
                &mut prev_col,
            );
        }

        for hunk in file.hunks() {
            // Hunk header
            if let Some(header) = hunk.header() {
                emit_node(
                    source_text,
                    &header.syntax().text_range(),
                    TOKEN_HUNK_HEADER,
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_col,
                );
            }

            // Hunk lines
            for line in hunk.lines() {
                let kind = line.syntax().kind();
                let token_type = match kind {
                    SyntaxKind::ADD_LINE => TOKEN_ADD_LINE,
                    SyntaxKind::DELETE_LINE => TOKEN_DELETE_LINE,
                    SyntaxKind::CONTEXT_LINE => TOKEN_CONTEXT_LINE,
                    _ => continue,
                };

                emit_node(
                    source_text,
                    &line.syntax().text_range(),
                    token_type,
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_col,
                );
            }
        }
    }

    tokens
}

fn emit_node(
    text: &str,
    range: &rowan::TextRange,
    token_type: u32,
    tokens: &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_col: &mut u32,
) {
    let start = offset_to_position(text, range.start());
    let end = offset_to_position(text, range.end());

    // Semantic tokens are per-line; split multi-line nodes
    for line in start.line..=end.line {
        let col = if line == start.line {
            start.character
        } else {
            0
        };
        let length = if start.line == end.line {
            end.character - start.character
        } else if line == start.line {
            // Rest of the first line — approximate with a large value;
            // we'd need to measure the line length for exactness, but
            // the client clips to the actual line length.
            200
        } else if line == end.line {
            end.character
        } else {
            200
        };

        if length == 0 {
            continue;
        }

        let delta_line = line - *prev_line;
        let delta_col = if delta_line == 0 {
            col - *prev_col
        } else {
            col
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start: delta_col,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        *prev_line = line;
        *prev_col = col;
    }
}
