//! Code lens generation for patch files.
//!
//! Shows fuzz-related information for each hunk: the number of context lines
//! available, which determines the maximum useful --fuzz value.

use patchkit::edit::Patch;
use rowan::ast::AstNode;
use tower_lsp_server::ls_types::*;

use crate::position::offset_to_position;

/// Generate code lenses for a patch file.
pub fn get_code_lenses(patch: &Patch, source_text: &str) -> Vec<CodeLens> {
    let mut lenses = Vec::new();

    for file in patch.patch_files() {
        for hunk in file.hunks() {
            let Some(header) = hunk.header() else {
                continue;
            };

            let stats = hunk.stats();
            let pos = offset_to_position(source_text, header.syntax().text_range().start());

            let label = format!(
                "{} context, +{} −{}",
                stats.context, stats.additions, stats.deletions
            );

            lenses.push(CodeLens {
                range: Range::new(pos, pos),
                command: Some(Command {
                    title: label,
                    command: String::new(),
                    arguments: None,
                }),
                data: None,
            });
        }
    }

    lenses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_lenses(text: &str) -> Vec<CodeLens> {
        let parsed = patchkit::edit::parse(text);
        let patch = parsed.tree();
        get_code_lenses(&patch, text)
    }

    #[test]
    fn test_single_hunk_lens() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,4 +1,4 @@
 ctx1
 ctx2
-old
+new
";
        let lenses = parse_and_lenses(text);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "2 context, +1 −1"
        );
    }

    #[test]
    fn test_multiple_hunks() {
        let text = "\
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,3 @@
 ctx
-old
+new1
+new2
@@ -10,1 +11,1 @@
-a
+b
";
        let lenses = parse_and_lenses(text);
        assert_eq!(lenses.len(), 2);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "1 context, +2 −1"
        );
        assert_eq!(
            lenses[1].command.as_ref().unwrap().title,
            "0 context, +1 −1"
        );
    }

    #[test]
    fn test_no_lenses_for_empty() {
        let text = "";
        let lenses = parse_and_lenses(text);
        assert!(lenses.is_empty());
    }
}
