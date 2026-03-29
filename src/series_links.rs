//! Document link generation for series files.
//!
//! Makes patch names clickable, resolving them relative to the series file's
//! directory (typically `debian/patches/`).

use patchkit::edit::series::SeriesFile;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::text_range_to_lsp_range;

/// Generate document links for patch names in a series file.
pub fn get_document_links(
    series: &SeriesFile,
    source_text: &str,
    document_uri: &Uri,
) -> Vec<DocumentLink> {
    let base = document_uri.to_string();
    let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or(&base);

    series
        .patch_entries()
        .filter_map(|entry| {
            let token = entry.name_token()?;
            let name = token.text();
            let range = text_range_to_lsp_range(source_text, token.text_range());
            let target: Uri = format!("{}/{}", dir, name).parse().ok()?;

            Some(DocumentLink {
                range,
                target: Some(target),
                tooltip: Some(name.to_string()),
                data: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_links() {
        let text = "0001-fix.patch\n# comment\n0002-feature.patch -p1\n";
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();

        let links = get_document_links(&series, text, &uri);
        assert_eq!(links.len(), 2);

        assert_eq!(
            links[0].target.as_ref().unwrap().to_string(),
            "file:///project/debian/patches/0001-fix.patch"
        );
        assert_eq!(links[0].tooltip.as_deref(), Some("0001-fix.patch"));

        assert_eq!(
            links[1].target.as_ref().unwrap().to_string(),
            "file:///project/debian/patches/0002-feature.patch"
        );
    }

    #[test]
    fn test_no_links_for_comments() {
        let text = "# just a comment\n";
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();

        let links = get_document_links(&series, text, &uri);
        assert!(links.is_empty());
    }

    #[test]
    fn test_empty_series() {
        let text = "";
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();

        let links = get_document_links(&series, text, &uri);
        assert!(links.is_empty());
    }

    #[test]
    fn test_subdirectory_patch() {
        let text = "subdir/0001-fix.patch\n";
        let parsed = patchkit::edit::series::parse(text);
        let series = parsed.tree();
        let uri: Uri = "file:///project/debian/patches/series".parse().unwrap();

        let links = get_document_links(&series, text, &uri);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].target.as_ref().unwrap().to_string(),
            "file:///project/debian/patches/subdir/0001-fix.patch"
        );
    }
}
