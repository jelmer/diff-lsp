//! Diff/Patch Language Server Protocol implementation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

mod code_actions;
mod diagnostics;
mod document_links;
mod folding;
mod highlights;
mod hover;
mod inlay_hints;
mod position;
mod selection_ranges;
mod semantic;
mod symbols;

use position::try_lsp_range_to_text_range;

/// Information about an open file.
struct FileInfo {
    /// The current source text.
    text: String,
    /// The parsed patch (green node for thread safety).
    parsed: patchkit::edit::Parse<patchkit::edit::Patch>,
}

struct Backend {
    client: Client,
    files: Arc<Mutex<HashMap<Uri, FileInfo>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            files: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn update_file(&self, uri: Uri, text: String) {
        let parsed = patchkit::edit::parse(&text);
        let diagnostics = diagnostics::get_diagnostics(&text, &parsed);

        let mut files = self.files.lock().await;
        files.insert(uri.clone(), FileInfo { text, parsed });
        drop(files);

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::new("diffFileHeader"),
                                    SemanticTokenType::new("diffHunkHeader"),
                                    SemanticTokenType::new("diffAddedLine"),
                                    SemanticTokenType::new("diffDeletedLine"),
                                    SemanticTokenType::new("diffContextLine"),
                                ],
                                token_modifiers: vec![],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "diff-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "diff-lsp initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_file(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        if params.content_changes.is_empty() {
            return;
        }

        let files = self.files.lock().await;
        let mut text = files.get(&uri).map(|f| f.text.clone()).unwrap_or_default();
        drop(files);

        for change in &params.content_changes {
            if let Some(range) = &change.range {
                if let Some(text_range) = try_lsp_range_to_text_range(&text, range) {
                    let start: usize = text_range.start().into();
                    let end: usize = text_range.end().into();
                    text.replace_range(start..end, &change.text);
                }
            } else {
                text = change.text.clone();
            }
        }

        self.update_file(uri, text).await;
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let ranges = folding::generate_folding_ranges(&patch, &file_info.text);
        drop(files);

        Ok(Some(ranges))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let syms = symbols::generate_document_symbols(&patch, &file_info.text);
        drop(files);

        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let range = params.range;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let actions = code_actions::get_code_actions(&patch, &file_info.text, range, uri);
        drop(files);

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                actions
                    .into_iter()
                    .map(CodeActionOrCommand::CodeAction)
                    .collect(),
            ))
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let range = params.range;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let hints = inlay_hints::get_inlay_hints(&patch, &file_info.text, range);
        drop(files);

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let hl = highlights::get_highlights(&patch, &file_info.text, position);
        drop(files);

        if hl.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hl))
        }
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let links = document_links::get_document_links(&patch, &file_info.text, uri);
        drop(files);

        if links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(links))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let result = hover::get_hover(&patch, &file_info.text, position);
        drop(files);

        Ok(result)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let tokens = semantic::generate_semantic_tokens(&patch, &file_info.text);
        drop(files);

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let patch = file_info.parsed.tree_lossy();
        let ranges =
            selection_ranges::get_selection_ranges(&patch, &file_info.text, &params.positions);
        drop(files);

        Ok(Some(ranges))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
