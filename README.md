# diff-lsp

A language server for unified diff/patch files and quilt series files.

For patch files it reports diagnostics (parse errors, hunk count
mismatches, duplicate file paths), offers code actions to remove,
reverse, split, or fix the line counts of a hunk, and provides hover
stats, document symbols, document links, folding, selection ranges,
semantic highlighting and inlay hints. Go-to-definition jumps from the
`---`/`+++` header to the actual source file.

For quilt series files it warns about duplicate entries, missing
patches and patches present in the directory but not listed in the
series. Code actions wrap the common quilt commands (push, pop,
delete, refresh, new, import). Patch entries can be reordered up and
down or renamed across the series and on disk, and completions
suggest patches sitting in the directory that haven't been added yet.
Hover shows the patch description and change statistics.

## Building

```sh
cargo build --release
```

The binary will be at `target/release/diff-lsp`.

## SCIP index generation

The `scip` subcommand emits a [SCIP](https://github.com/sourcegraph/scip)
index recording the cross-file references in patch and quilt series files. In
patch files the source paths in `---`/`+++` headers reference the modified
files; in series files each entry references a patch file.

```sh
diff-lsp scip [-o OUTPUT] FILE...
```

Output defaults to `index.scip`. Paths are recorded relative to the current
working directory, which is taken as the project root.

## Editor integration

For VS Code, see [vscode-diff-lsp/](vscode-diff-lsp/) — `npm install`
then `npm run package` produces a `.vsix` you can install.

For coc.nvim, see [coc-diff/](coc-diff/), or install it from npm.

## License

Apache-2.0
