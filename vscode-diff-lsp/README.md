# Diff/Patch Language Support for VS Code

Language Server Protocol support for **diff/patch files** and **quilt series files**, powered by [diff-lsp](https://github.com/jelmer/diff-lsp).

## Features

### Diff/Patch Files (`.patch`, `.diff`)

- **Diagnostics** - parse errors, hunk line count mismatches, duplicate file paths
- **Code actions** - remove hunk, reverse hunk, split hunk, fix hunk line counts
- **Go-to-definition** - jump to files referenced in patch headers
- **Document symbols** - outline of patched files and hunks
- **Document links** - clickable file paths in patch headers
- **Hover** - hunk statistics (additions, deletions, context lines)
- **Semantic highlighting** - file headers, hunk headers, added/deleted/context lines
- **Folding** - collapse/expand hunks and file sections
- **Inlay hints** and **selection ranges**

### Quilt Series Files (`series`, `series.conf`)

- **Diagnostics** - duplicate entries, missing patch files, unlisted patches
- **Code actions** - quilt push, pop, delete, refresh, new, import
- **Go-to-definition** - jump to patch files listed in the series
- **Document symbols** - outline of patch entries
- **Document links** - clickable patch filenames
- **Hover** - patch metadata
- **Completions** - patch filename suggestions from the patches directory
- **Rename** - rename a patch across the series file and on disk
- **Reorder** - move patches up/down in the series
- **Semantic highlighting**, **folding**, **inlay hints**

## Installation

### From a release

Download the `.vsix` file for your platform from the
[releases page](https://github.com/jelmer/diff-lsp/releases) and install it:

```
code --install-extension vscode-diff-lsp-<platform>.vsix
```

Platform-specific packages bundle the `diff-lsp` binary. The universal package
requires `diff-lsp` to be available in your `PATH`.

### From source

```sh
cd vscode-diff-lsp
npm install
npm run package
code --install-extension vscode-diff-lsp-0.1.0.vsix
```

This requires building the `diff-lsp` binary separately (`cargo build --release`
in the repository root) and either placing it in your `PATH` or setting the
`diff.serverPath` setting.

## Settings

| Setting             | Default | Description                                                                 |
|---------------------|---------|-----------------------------------------------------------------------------|
| `diff.enable`       | `true`  | Enable or disable the language server.                                      |
| `diff.serverPath`   | `""`    | Path to the `diff-lsp` executable. Leave empty to use the bundled binary or find `diff-lsp` in `PATH`. |
| `diff.trace.server` | `"off"` | Trace LSP communication (`"off"`, `"messages"`, or `"verbose"`).            |

## License

Apache-2.0
