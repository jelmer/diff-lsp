//! Diagnostic generation from parse errors and semantic checks.

use patchkit::edit::{Parse, Patch, PositionedParseError};
use rowan::ast::AstNode;
use std::collections::HashMap;
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

/// Collect diagnostics from parse errors and semantic analysis.
pub fn get_diagnostics(source_text: &str, parsed: &Parse<Patch>) -> Vec<Diagnostic> {
    let mut diagnostics = parse_error_diagnostics(source_text, parsed);

    let patch = parsed.tree_lossy();
    diagnostics.extend(check_hunk_line_counts(source_text, &patch));
    diagnostics.extend(check_duplicate_file_paths(source_text, &patch));
    diagnostics.extend(check_missing_file_headers(source_text, &patch));

    diagnostics
}

fn parse_error_diagnostics(source_text: &str, parsed: &Parse<Patch>) -> Vec<Diagnostic> {
    let errors: Vec<&PositionedParseError> = parsed.positioned_errors().iter().collect();
    let string_errors = parsed.errors();

    let mut diagnostics: Vec<Diagnostic> = errors
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
        .collect();

    if errors.is_empty() {
        for msg in string_errors {
            diagnostics.push(make_diagnostic(
                Range::new(Position::new(0, 0), Position::new(0, 0)),
                DiagnosticSeverity::ERROR,
                "parse-error",
                msg.clone(),
            ));
        }
    }

    diagnostics
}

/// Check that hunk line counts match what the header declares.
fn check_hunk_line_counts(source_text: &str, patch: &Patch) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for file in patch.patch_files() {
        for hunk in file.hunks() {
            let Some(header) = hunk.header() else {
                continue;
            };

            for mismatch in header.check_counts(&hunk) {
                let range = text_range_to_lsp_range(source_text, header.syntax().text_range());
                diagnostics.push(make_diagnostic(
                    range,
                    DiagnosticSeverity::WARNING,
                    "hunk-line-count-mismatch",
                    format!(
                        "{} side declares {} lines but hunk contains {}",
                        mismatch.side, mismatch.expected, mismatch.actual,
                    ),
                ));
            }
        }
    }

    diagnostics
}

/// Warn if the same file path appears in multiple PatchFile sections.
fn check_duplicate_file_paths(source_text: &str, patch: &Patch) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Range> = HashMap::new();

    for file in patch.patch_files() {
        let Some(path) = file.path() else {
            continue;
        };

        let file_range = text_range_to_lsp_range(source_text, file.syntax().text_range());

        if let Some(first_range) = seen.get(&path) {
            diagnostics.push(make_diagnostic(
                file_range,
                DiagnosticSeverity::WARNING,
                "duplicate-file-path",
                format!(
                    "file '{}' already modified on line {}",
                    path,
                    first_range.start.line + 1
                ),
            ));
        } else {
            seen.insert(path, file_range);
        }
    }

    diagnostics
}

/// Warn about hunks that lack file headers.
fn check_missing_file_headers(source_text: &str, patch: &Patch) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for file in patch.patch_files() {
        if file.old_file().is_none() && file.new_file().is_none() {
            let range = text_range_to_lsp_range(source_text, file.syntax().text_range());
            diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "missing-file-header",
                "hunk without --- / +++ file headers".to_string(),
            ));
        }
    }

    diagnostics
}
