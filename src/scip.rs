//! SCIP index generation for diff/patch and quilt series files.
//!
//! Produces a [SCIP](https://github.com/sourcegraph/scip) index that records
//! the cross-file references contained in patch and series files:
//!
//! - In patch files, the source paths in `---`/`+++` headers reference the
//!   files the patch modifies.
//! - In series files, each entry references a patch file.
//!
//! Each referenced file is minted as a SCIP symbol, and an occurrence pointing
//! at that symbol is emitted over the path token. This mirrors the
//! go-to-definition behaviour, so SCIP consumers (e.g. Sourcegraph) can
//! navigate from a patch to the files it touches.

use std::path::{Path, PathBuf};

use ::scip::types::{
    Document, Index, Metadata, Occurrence, SymbolInformation, SymbolRole, TextEncoding, ToolInfo,
};
use patchkit::edit::lex::SyntaxKind;
use patchkit::edit::series::SeriesFile;
use patchkit::edit::Patch;
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::detection::{self, FileKind};

/// Re-export of the SCIP crate's file writer so callers can serialize an
/// [`Index`] without depending on the protobuf crate directly.
pub use ::scip::write_message_to_file;

/// The SCIP symbol scheme used for files referenced by diff/series files.
const SCHEME: &str = "scip-diff";

/// Build a SCIP [`Index`] for the given input files.
///
/// `project_root` is recorded in the index metadata and used to make document
/// and symbol paths relative. Files that fail to read are reported as errors;
/// files that simply contain no references produce an empty document.
pub fn build_index(files: &[PathBuf], project_root: &Path) -> std::io::Result<Index> {
    let mut documents = Vec::new();

    for file in files {
        let text = std::fs::read_to_string(file)?;
        documents.push(build_document(file, &text, project_root));
    }

    let mut metadata = Metadata::new();
    metadata.project_root = path_to_uri(project_root);
    metadata.text_document_encoding = TextEncoding::UTF8.into();

    let mut tool_info = ToolInfo::new();
    tool_info.name = "diff-lsp".to_string();
    tool_info.version = env!("CARGO_PKG_VERSION").to_string();
    metadata.tool_info = Some(tool_info).into();

    let mut index = Index::new();
    index.metadata = Some(metadata).into();
    index.documents = documents;

    Ok(index)
}

/// Build a single SCIP [`Document`] for one input file.
fn build_document(path: &Path, text: &str, project_root: &Path) -> Document {
    let kind = detection::detect_file_kind(&path_to_lsp_uri(path));

    let mut occurrences = Vec::new();
    let mut symbols = Vec::new();

    match kind {
        FileKind::Patch => {
            let parsed = patchkit::edit::parse(text);
            collect_patch_references(
                &parsed.tree(),
                text,
                project_root,
                &mut occurrences,
                &mut symbols,
            );
        }
        FileKind::Series => {
            let parsed = patchkit::edit::series::parse(text);
            collect_series_references(
                &parsed.tree(),
                text,
                path,
                project_root,
                &mut occurrences,
                &mut symbols,
            );
        }
    }

    let mut document = Document::new();
    document.language = "Diff".to_string();
    document.relative_path = relative_path(project_root, path);
    document.occurrences = occurrences;
    document.symbols = symbols;
    document
}

/// Collect references from a patch file's `---`/`+++` headers.
fn collect_patch_references(
    patch: &Patch,
    text: &str,
    project_root: &Path,
    occurrences: &mut Vec<Occurrence>,
    symbols: &mut Vec<SymbolInformation>,
) {
    let mut seen_symbols = std::collections::HashSet::new();

    for file in patch.patch_files() {
        for node in [
            file.old_file().map(|f| f.syntax().clone()),
            file.new_file().map(|f| f.syntax().clone()),
        ]
        .into_iter()
        .flatten()
        {
            let Some(token) = node
                .children_with_tokens()
                .filter_map(|it| it.into_token())
                .find(|t| t.kind() == SyntaxKind::PATH)
            else {
                continue;
            };

            let path_text = token.text();
            if path_text == "/dev/null" {
                continue;
            }

            let clean = strip_ab_prefix(path_text);
            let Some(target) = resolve_against_root(project_root, clean) else {
                continue;
            };

            let symbol = file_symbol(project_root, &target);
            push_reference(text, token.text_range(), &symbol, occurrences);
            push_symbol(&symbol, clean, &mut seen_symbols, symbols);
        }
    }
}

/// Collect references from a series file's patch entries.
fn collect_series_references(
    series: &SeriesFile,
    text: &str,
    series_path: &Path,
    project_root: &Path,
    occurrences: &mut Vec<Occurrence>,
    symbols: &mut Vec<SymbolInformation>,
) {
    let mut seen_symbols = std::collections::HashSet::new();

    let patches_dir = match series_path.parent() {
        Some(dir) => dir,
        None => return,
    };

    for entry in series.patch_entries() {
        let Some(token) = entry.name_token() else {
            continue;
        };
        let name = token.text();
        let target = patches_dir.join(name);

        let symbol = file_symbol(project_root, &target);
        push_reference(text, token.text_range(), &symbol, occurrences);
        push_symbol(&symbol, name, &mut seen_symbols, symbols);
    }
}

/// Append a reference occurrence over `range` pointing at `symbol`.
fn push_reference(text: &str, range: TextRange, symbol: &str, occurrences: &mut Vec<Occurrence>) {
    let mut occ = Occurrence::new();
    occ.range = scip_range(text, range);
    occ.symbol = symbol.to_string();
    occ.symbol_roles = SymbolRole::ReadAccess as i32;
    occurrences.push(occ);
}

/// Append a [`SymbolInformation`] for `symbol` once per distinct symbol.
fn push_symbol(
    symbol: &str,
    display_name: &str,
    seen: &mut std::collections::HashSet<String>,
    symbols: &mut Vec<SymbolInformation>,
) {
    if !seen.insert(symbol.to_string()) {
        return;
    }
    let mut info = SymbolInformation::new();
    info.symbol = symbol.to_string();
    info.display_name = display_name.to_string();
    symbols.push(info);
}

/// Mint a SCIP symbol string for a referenced file.
///
/// The file's project-relative path is encoded with one namespace (package)
/// descriptor per path segment, e.g. `src/foo.rs` becomes
/// `scip-diff . . . src/ `foo.rs`/`.
fn file_symbol(project_root: &Path, target: &Path) -> String {
    let rel = relative_path(project_root, target);
    let descriptors: String = rel
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| format!("{}/", escape_descriptor(seg)))
        .collect();
    format!("{SCHEME} . . . {descriptors}")
}

/// Escape a descriptor name for inclusion in a SCIP symbol string.
///
/// Names that are not made up of the simple identifier characters are wrapped
/// in backticks, with literal backticks doubled.
fn escape_descriptor(name: &str) -> String {
    let simple = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '$'));
    if simple {
        name.to_string()
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}

/// Convert a byte [`TextRange`] to a SCIP range.
///
/// SCIP ranges are `[startLine, startChar, endLine, endChar]`, zero-based and
/// half-open, with character offsets in UTF-16 code units (the default
/// position encoding).
fn scip_range(text: &str, range: TextRange) -> Vec<i32> {
    let start = crate::position::offset_to_position(text, range.start());
    let end = crate::position::offset_to_position(text, range.end());
    if start.line == end.line {
        vec![
            start.line as i32,
            start.character as i32,
            end.character as i32,
        ]
    } else {
        vec![
            start.line as i32,
            start.character as i32,
            end.line as i32,
            end.character as i32,
        ]
    }
}

/// Strip a leading `a/` or `b/` prefix from a patch path.
fn strip_ab_prefix(path: &str) -> &str {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
}

/// Resolve a patch-relative path against the project root, returning the
/// target only if it exists on disk.
fn resolve_against_root(project_root: &Path, clean: &str) -> Option<PathBuf> {
    let target = project_root.join(clean);
    target.is_file().then_some(target)
}

/// Compute a path relative to `root`, falling back to the original path's
/// string form if it is not under `root`.
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Convert a filesystem path to a `file://` URI string for metadata.
fn path_to_uri(path: &Path) -> String {
    tower_lsp_server::ls_types::Uri::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Convert a filesystem path to an LSP [`Uri`] for file-kind detection.
fn path_to_lsp_uri(path: &Path) -> tower_lsp_server::ls_types::Uri {
    tower_lsp_server::ls_types::Uri::from_file_path(path).unwrap_or_else(|| {
        // Fall back to a bare path URI; detection only needs the file name.
        format!("file://{}", path.to_string_lossy())
            .parse()
            .expect("path URI")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, contents: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn test_patch_index_references_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        write(root, "src/foo.rs", "fn main() {}");

        let patch = write(
            root,
            "patches/fix.patch",
            "--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );

        let index = build_index(&[patch], root).unwrap();
        assert_eq!(index.documents.len(), 1);
        let doc = &index.documents[0];
        assert_eq!(doc.relative_path, "patches/fix.patch");
        // One occurrence per path token (--- and +++), both resolving to the
        // same symbol.
        assert_eq!(doc.occurrences.len(), 2);
        let expected_symbol = "scip-diff . . . src/`foo.rs`/";
        assert_eq!(doc.occurrences[0].symbol, expected_symbol);
        assert_eq!(doc.occurrences[1].symbol, expected_symbol);
        // The symbol is declared once.
        assert_eq!(doc.symbols.len(), 1);
        assert_eq!(doc.symbols[0].symbol, expected_symbol);
        assert_eq!(doc.symbols[0].display_name, "src/foo.rs");
    }

    #[test]
    fn test_patch_index_skips_missing_and_dev_null() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        let patch = write(
            root,
            "patches/new.patch",
            "--- /dev/null\n+++ b/missing.txt\n@@ -0,0 +1 @@\n+line\n",
        );

        let index = build_index(&[patch], root).unwrap();
        let doc = &index.documents[0];
        assert_eq!(doc.occurrences.len(), 0);
        assert_eq!(doc.symbols.len(), 0);
    }

    #[test]
    fn test_patch_occurrence_range() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        write(root, "file.txt", "hello");

        let patch = write(
            root,
            "patches/fix.patch",
            "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n",
        );

        let index = build_index(&[patch], root).unwrap();
        let occ = &index.documents[0].occurrences[0];
        // "a/file.txt" starts at column 4 on line 0 and is 10 chars long.
        assert_eq!(occ.range, vec![0, 4, 14]);
    }

    #[test]
    fn test_series_index_references_patches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let patches = root.join("debian/patches");
        std::fs::create_dir_all(&patches).unwrap();
        std::fs::write(patches.join("0001-fix.patch"), "").unwrap();
        let series = write(root, "debian/patches/series", "0001-fix.patch\n# c\n");

        let index = build_index(&[series], root).unwrap();
        let doc = &index.documents[0];
        assert_eq!(doc.relative_path, "debian/patches/series");
        assert_eq!(doc.occurrences.len(), 1);
        assert_eq!(
            doc.occurrences[0].symbol,
            "scip-diff . . . debian/patches/`0001-fix.patch`/"
        );
    }

    #[test]
    fn test_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        let patch = write(root, "patches/empty.patch", "");

        let index = build_index(&[patch], root).unwrap();
        let meta = index.metadata.as_ref().unwrap();
        assert_eq!(meta.tool_info.name, "diff-lsp");
        assert_eq!(meta.tool_info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(meta.text_document_encoding, TextEncoding::UTF8.into());
    }

    #[test]
    fn test_written_index_roundtrips() {
        use protobuf::Message;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        write(root, "src/foo.rs", "fn main() {}");
        let patch = write(
            root,
            "patches/fix.patch",
            "--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );

        let index = build_index(&[patch], root).unwrap();
        let out = root.join("index.scip");
        write_message_to_file(&out, index).unwrap();

        let bytes = std::fs::read(&out).unwrap();
        let parsed = Index::parse_from_bytes(&bytes).unwrap();
        assert_eq!(parsed.documents.len(), 1);
        assert_eq!(parsed.documents[0].relative_path, "patches/fix.patch");
        assert_eq!(parsed.documents[0].occurrences.len(), 2);
        assert_eq!(parsed.metadata.tool_info.name, "diff-lsp");
    }

    #[test]
    fn test_escape_descriptor() {
        assert_eq!(escape_descriptor("src/foo.rs"), "`src/foo.rs`");
        assert_eq!(escape_descriptor("foo-bar_1"), "foo-bar_1");
        assert_eq!(escape_descriptor("a`b"), "`a``b`");
    }
}
