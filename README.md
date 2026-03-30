# diff-lsp

A Language Server Protocol (LSP) implementation for diff/patch files and
quilt series files.

## Features

### Patch/diff files

- **Diagnostics**: parse errors, hunk line count mismatches, duplicate file paths
- **Code actions**: remove hunk, reverse hunk, split hunk, fix hunk line counts
- **Navigation**: go-to-definition for referenced files, document symbols, document links
- **Hover**: hunk statistics (additions, deletions, context lines)
- **Semantic highlighting**: file headers, hunk headers, added/deleted/context lines
- **Folding**: collapse/expand hunks and file sections
- **Inlay hints** and **selection ranges**

### Quilt series files

- **Diagnostics**: duplicate entries, missing patch files, unlisted patches
- **Code actions**: quilt push/pop/delete/refresh/new/import
- **Navigation**: go-to-definition for patch files, document symbols, document links
- **Hover**: patch metadata
- **Completions**: patch name suggestions
- **Rename**: rename patches throughout the series
- **Reorder**: move patches up/down in the series
- **Semantic highlighting**, **folding**, **inlay hints**

## Building

```sh
cargo build --release
```

The binary will be at `target/release/diff-lsp`.

## Editor integration

### VS Code

Install the extension from `vscode-diff-lsp/`:

```sh
cd vscode-diff-lsp
npm install
npm run package
```

Then install the resulting `.vsix` file in VS Code.

### Vim/Neovim (coc.nvim)

Install the plugin from `coc-diff/`:

```sh
cd coc-diff
npm install
```

## License

Apache-2.0
