//! Document link generation for patch file paths.

use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

/// Generate document links for file paths in --- / +++ headers.
pub fn get_document_links(
    patch: &Patch,
    source_text: &str,
    document_uri: &Uri,
) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    for file in patch.patch_files() {
        if let Some(old_file) = file.old_file() {
            if let Some(link) = path_link(source_text, old_file.syntax(), document_uri) {
                links.push(link);
            }
        }
        if let Some(new_file) = file.new_file() {
            if let Some(link) = path_link(source_text, new_file.syntax(), document_uri) {
                links.push(link);
            }
        }
    }

    links
}

fn path_link(
    source_text: &str,
    node: &rowan::SyntaxNode<patchkit::edit::Lang>,
    document_uri: &Uri,
) -> Option<DocumentLink> {
    let path_token = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|t| t.kind() == SyntaxKind::PATH)?;

    let path_text = path_token.text();

    // Skip /dev/null and other non-file paths
    if path_text == "/dev/null" {
        return None;
    }

    // Strip common prefixes like a/ or b/
    let clean_path = path_text
        .strip_prefix("a/")
        .or_else(|| path_text.strip_prefix("b/"))
        .unwrap_or(path_text);

    // Resolve relative to the document's directory
    let base = document_uri.to_string();
    let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or(&base);
    let target = format!("{}/{}", dir, clean_path);
    let target_uri: Uri = target.parse().ok()?;

    let range = text_range_to_lsp_range(source_text, path_token.text_range());

    Some(DocumentLink {
        range,
        target: Some(target_uri),
        tooltip: Some(clean_path.to_string()),
        data: None,
    })
}
