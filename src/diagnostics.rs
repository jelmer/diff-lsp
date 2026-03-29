//! Diagnostic generation from parse errors.

use patchkit::edit::{Parse, Patch};
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

fn make_diagnostic(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("diff-lsp".to_string()),
        message,
        ..Default::default()
    }
}

/// Collect diagnostics from parse errors.
pub fn get_diagnostics(source_text: &str, parsed: &Parse<Patch>) -> Vec<Diagnostic> {
    parsed
        .positioned_errors()
        .iter()
        .map(|error| {
            let range = text_range_to_lsp_range(source_text, error.position);
            make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "parse-error",
                error.message.clone(),
            )
        })
        .collect()
}
