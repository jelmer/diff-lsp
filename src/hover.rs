//! Hover information for patch files.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::{text_range_to_lsp_range, try_position_to_offset};

/// Get hover information at a position.
pub fn get_hover(patch: &Patch, source_text: &str, position: Position) -> Option<Hover> {
    let offset = try_position_to_offset(source_text, position)?;

    let root = patch.syntax();
    let token = root.token_at_offset(offset).right_biased()?;

    // Walk up to find the most relevant parent node
    let mut node = token.parent()?;
    loop {
        match node.kind() {
            SyntaxKind::HUNK_HEADER => {
                return hunk_header_hover(source_text, &node);
            }
            SyntaxKind::OLD_FILE | SyntaxKind::NEW_FILE => {
                return file_header_hover(&node);
            }
            SyntaxKind::HUNK => {
                return hunk_hover(source_text, &node);
            }
            SyntaxKind::PATCH_FILE => {
                return patch_file_hover(source_text, &node);
            }
            SyntaxKind::ROOT => break,
            _ => {}
        }
        node = node.parent()?;
    }

    None
}

fn hunk_header_hover(
    source_text: &str,
    node: &rowan::SyntaxNode<patchkit::edit::Lang>,
) -> Option<Hover> {
    let header = patchkit::edit::HunkHeader::cast(node.clone())?;
    let range = text_range_to_lsp_range(source_text, node.text_range());

    let mut parts = Vec::new();

    if let Some(old) = header.old_range() {
        let start = old.start().unwrap_or(0);
        let count = old.count().unwrap_or(1);
        parts.push(format!(
            "Old: lines {}–{}",
            start,
            start + count.saturating_sub(1)
        ));
    }
    if let Some(new) = header.new_range() {
        let start = new.start().unwrap_or(0);
        let count = new.count().unwrap_or(1);
        parts.push(format!(
            "New: lines {}–{}",
            start,
            start + count.saturating_sub(1)
        ));
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: parts.join("  \n"),
        }),
        range: Some(range),
    })
}

fn file_header_hover(node: &rowan::SyntaxNode<patchkit::edit::Lang>) -> Option<Hover> {
    let is_old = node.kind() == SyntaxKind::OLD_FILE;
    let label = if is_old { "Old file" } else { "New file" };

    let path = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|t| t.kind() == SyntaxKind::PATH)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**{label}**: `{}`", path.text()),
        }),
        range: None,
    })
}

fn hunk_hover(source_text: &str, node: &rowan::SyntaxNode<patchkit::edit::Lang>) -> Option<Hover> {
    let hunk = patchkit::edit::Hunk::cast(node.clone())?;
    let range = text_range_to_lsp_range(source_text, node.text_range());
    let stats = hunk.stats();

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**Hunk**: +{} −{} ({} context lines)",
                stats.additions, stats.deletions, stats.context
            ),
        }),
        range: Some(range),
    })
}

fn patch_file_hover(
    source_text: &str,
    node: &rowan::SyntaxNode<patchkit::edit::Lang>,
) -> Option<Hover> {
    let file = patchkit::edit::PatchFile::cast(node.clone())?;
    let range = text_range_to_lsp_range(source_text, node.text_range());

    let hunk_count = file.hunks().count();
    let mut total_add = 0u32;
    let mut total_del = 0u32;
    for hunk in file.hunks() {
        let s = hunk.stats();
        total_add += s.additions;
        total_del += s.deletions;
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**{}**  \n{} hunks, +{} −{}",
                file.display_name(),
                hunk_count,
                total_add,
                total_del
            ),
        }),
        range: Some(range),
    })
}
