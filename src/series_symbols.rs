//! Document symbol generation for quilt series files.
//!
//! Each patch entry becomes a symbol, enabling outline view and go-to-symbol.

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

/// Generate document symbols for a series file.
///
/// Each patch entry is a top-level symbol of kind File.
pub fn generate_series_symbols(series: &SeriesFile, source_text: &str) -> Vec<DocumentSymbol> {
    series
        .patch_entries()
        .filter_map(|entry| {
            let name = entry.name()?;
            let range = text_range_to_lsp_range(source_text, entry.syntax().text_range());

            let detail = entry.options().map(|opts| opts.option_strings().join(" "));

            #[allow(deprecated)]
            Some(DocumentSymbol {
                name,
                detail,
                kind: SymbolKind::FILE,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_series(text: &str) -> patchkit::edit::Parse<SeriesFile> {
        patchkit::edit::series::parse(text)
    }

    fn get_symbols(text: &str) -> Vec<DocumentSymbol> {
        let parsed = parse_series(text);
        let series = parsed.tree();
        generate_series_symbols(&series, text)
    }

    #[test]
    fn test_basic_symbols() {
        let text = "a.patch\nb.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "a.patch");
        assert_eq!(symbols[1].name, "b.patch");
    }

    #[test]
    fn test_symbol_kind_is_file() {
        let text = "a.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols[0].kind, SymbolKind::FILE);
    }

    #[test]
    fn test_comments_excluded() {
        let text = "a.patch\n# comment\nb.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "a.patch");
        assert_eq!(symbols[1].name, "b.patch");
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let symbols = get_symbols(text);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_comments_only() {
        let text = "# comment 1\n# comment 2\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_symbol_with_options() {
        let text = "a.patch -p1\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "a.patch");
        assert_eq!(symbols[0].detail, Some("-p1".to_string()));
    }

    #[test]
    fn test_symbol_without_options() {
        let text = "a.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols[0].detail, None);
    }

    #[test]
    fn test_symbol_ranges() {
        let text = "a.patch\nb.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols[0].range.start.line, 0);
        assert_eq!(symbols[1].range.start.line, 1);
    }

    #[test]
    fn test_no_children() {
        let text = "a.patch\n";
        let symbols = get_symbols(text);
        assert_eq!(symbols[0].children, None);
    }
}
