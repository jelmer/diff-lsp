//! Semantic token generation for quilt series files.
//!
//! Highlights patch names, options (-pN), and comments with distinct token types.

use patchkit::edit::series::lex::SyntaxKind;
use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

// Token type indices (must match the legend in main.rs)
const TOKEN_PATCH_NAME: u32 = 5;
const TOKEN_OPTION: u32 = 6;
const TOKEN_COMMENT: u32 = 7;

/// Generate semantic tokens for a series file.
pub fn generate_series_semantic_tokens(
    series: &SeriesFile,
    source_text: &str,
) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    for element in series.syntax().descendants_with_tokens() {
        let rowan::NodeOrToken::Token(token) = element else {
            continue;
        };

        let token_type = match token.kind() {
            SyntaxKind::PATCH_NAME => TOKEN_PATCH_NAME,
            SyntaxKind::OPTION => TOKEN_OPTION,
            SyntaxKind::HASH | SyntaxKind::TEXT => TOKEN_COMMENT,
            _ => continue,
        };

        let range = token.text_range();
        let text = token.text();

        // Skip empty tokens
        if text.is_empty() {
            continue;
        }

        let pos = offset_to_position(source_text, range.start());
        let length = text.len() as u32;

        let delta_line = pos.line - prev_line;
        let delta_start = if delta_line == 0 {
            pos.character - prev_col
        } else {
            pos.character
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = pos.line;
        prev_col = pos.character;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_tokens(text: &str) -> Vec<SemanticToken> {
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        generate_series_semantic_tokens(&series, text)
    }

    #[test]
    fn test_patch_name_token() {
        let text = "a.patch\n";
        let tokens = parse_and_tokens(text);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TOKEN_PATCH_NAME);
        assert_eq!(tokens[0].delta_line, 0);
        assert_eq!(tokens[0].delta_start, 0);
        assert_eq!(tokens[0].length, 7); // "a.patch"
    }

    #[test]
    fn test_option_token() {
        let text = "a.patch -p1\n";
        let tokens = parse_and_tokens(text);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TOKEN_PATCH_NAME);
        assert_eq!(tokens[1].token_type, TOKEN_OPTION);
        assert_eq!(tokens[1].length, 3); // "-p1"
    }

    #[test]
    fn test_comment_tokens() {
        let text = "# this is a comment\n";
        let tokens = parse_and_tokens(text);
        // Should have hash token and text token
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TOKEN_COMMENT); // "#"
        assert_eq!(tokens[0].length, 1);
        assert_eq!(tokens[1].token_type, TOKEN_COMMENT); // " this is a comment"
    }

    #[test]
    fn test_multiple_lines() {
        let text = "a.patch\nb.patch\n";
        let tokens = parse_and_tokens(text);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].delta_line, 0);
        assert_eq!(tokens[1].delta_line, 1);
        assert_eq!(tokens[1].delta_start, 0);
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let tokens = parse_and_tokens(text);
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_mixed_content() {
        let text = "a.patch\n# comment\nb.patch -p1\n";
        let tokens = parse_and_tokens(text);

        // a.patch (name), # (comment), text (comment), b.patch (name), -p1 (option)
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].token_type, TOKEN_PATCH_NAME);
        assert_eq!(tokens[1].token_type, TOKEN_COMMENT); // #
        assert_eq!(tokens[2].token_type, TOKEN_COMMENT); // comment text
        assert_eq!(tokens[3].token_type, TOKEN_PATCH_NAME);
        assert_eq!(tokens[4].token_type, TOKEN_OPTION);
    }

    #[test]
    fn test_delta_encoding() {
        let text = "a.patch -p1\nb.patch\n";
        let tokens = parse_and_tokens(text);
        assert_eq!(tokens.len(), 3);

        // First token: a.patch at (0,0)
        assert_eq!(tokens[0].delta_line, 0);
        assert_eq!(tokens[0].delta_start, 0);

        // Second token: -p1 at (0,8) — same line
        assert_eq!(tokens[1].delta_line, 0);
        assert_eq!(tokens[1].delta_start, 8); // "a.patch " = 8 chars

        // Third token: b.patch at (1,0) — next line
        assert_eq!(tokens[2].delta_line, 1);
        assert_eq!(tokens[2].delta_start, 0);
    }
}
