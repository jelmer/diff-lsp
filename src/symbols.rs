//! Document symbol generation for patch files.

use patchkit::edit::{Patch, PatchFile};
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

/// Generate document symbols for a patch file.
///
/// Each file diff is a top-level symbol, with hunks as children.
pub fn generate_document_symbols(patch: &Patch, source_text: &str) -> Vec<DocumentSymbol> {
    patch
        .patch_files()
        .map(|file| file_symbol(&file, source_text))
        .collect()
}

fn file_symbol(file: &PatchFile, text: &str) -> DocumentSymbol {
    let range = text_range_to_lsp_range(text, file.syntax().text_range());

    let name = file_name(file);

    let children: Vec<DocumentSymbol> = file
        .hunks()
        .map(|hunk| {
            let hunk_range = text_range_to_lsp_range(text, hunk.syntax().text_range());
            let hunk_name = hunk
                .header()
                .map(|h| h.syntax().text().to_string().trim().to_string())
                .unwrap_or_else(|| "hunk".to_string());

            #[allow(deprecated)]
            DocumentSymbol {
                name: hunk_name,
                detail: None,
                kind: SymbolKind::STRUCT,
                tags: None,
                deprecated: None,
                range: hunk_range,
                selection_range: hunk_range,
                children: None,
            }
        })
        .collect();

    #[allow(deprecated)]
    DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::FILE,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

fn file_name(file: &PatchFile) -> String {
    let old = file
        .old_file()
        .and_then(|f| f.path())
        .map(|t| t.text().to_string());
    let new = file
        .new_file()
        .and_then(|f| f.path())
        .map(|t| t.text().to_string());

    match (old, new) {
        (Some(o), Some(n)) if o == n => o,
        (Some(o), Some(n)) => format!("{o} → {n}"),
        (Some(o), None) => o,
        (None, Some(n)) => n,
        (None, None) => "<unknown>".to_string(),
    }
}
