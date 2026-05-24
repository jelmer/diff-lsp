# Diff/Patch Language Support for VS Code

A VS Code client for [diff-lsp](https://github.com/jelmer/diff-lsp), a
language server for unified diff/patch files and quilt series files.

## What it does

In `.patch` and `.diff` files it reports parse errors and warns about
inconsistencies like hunk counts that don't match the header or
duplicate file paths. Code actions let you remove, reverse, split, or
fix the line counts of a hunk. The `---` and `+++` headers turn into
clickable links and support go-to-definition. Hover shows hunk
statistics, and there's the usual scaffolding (document symbols,
folding, selection ranges, inlay hints, semantic highlighting).

In quilt `series` files it warns about duplicates, patches listed but
not present on disk, and patch files in the directory that aren't
listed. Code actions wrap the common quilt commands — push, pop,
delete, refresh, new, import. Patch entries can be renamed (the file
on disk is renamed too) or reordered up and down. Completions suggest
patches not yet in the series. Hover shows the patch description
along with file/line statistics.

## Installation

Grab the `.vsix` for your platform from the
[releases page](https://github.com/jelmer/diff-lsp/releases) and:

```
code --install-extension vscode-diff-lsp-<platform>.vsix
```

The platform-specific packages bundle the `diff-lsp` binary; the
universal package expects `diff-lsp` on your `PATH`.

To build from source, run `npm install && npm run package` in this
directory. You'll need to build the server separately (`cargo build
--release` in the repo root) and either put it on your `PATH` or
point `diff.serverPath` at it.

## Settings

| Setting             | Default | Description                                                                 |
|---------------------|---------|-----------------------------------------------------------------------------|
| `diff.enable`       | `true`  | Enable or disable the language server.                                      |
| `diff.serverPath`   | `""`    | Path to the `diff-lsp` executable. Leave empty to use the bundled binary or find `diff-lsp` in `PATH`. |
| `diff.trace.server` | `"off"` | Trace LSP communication (`"off"`, `"messages"`, or `"verbose"`).            |

## License

Apache-2.0
