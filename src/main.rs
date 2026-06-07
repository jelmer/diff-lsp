//! Diff/Patch Language Server Protocol implementation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

mod code_actions;
mod code_lenses;
mod detection;
mod diagnostics;
mod document_links;
mod folding;
mod goto_definition;
mod highlights;
mod hover;
mod patch_quilt_actions;
mod patch_quilt_diagnostics;
mod patch_quilt_lenses;
mod position;
#[cfg(feature = "scip")]
mod scip;
mod selection_ranges;
mod semantic;
mod series_code_actions;
mod series_code_lenses;
mod series_completions;
mod series_diagnostics;
mod series_folding;
mod series_highlights;
mod series_hover;
mod series_inlay_hints;
mod series_links;
mod series_rename;
mod series_reorder;
mod series_selection_ranges;
mod series_semantic;
mod series_symbols;
mod symbols;

use detection::FileKind;
use position::try_lsp_range_to_text_range;

enum ParsedFile {
    Patch(patchkit::edit::Parse<patchkit::edit::Patch>),
    Series(patchkit::edit::Parse<patchkit::edit::series::SeriesFile>),
}

struct FileInfo {
    text: String,
    parsed: ParsedFile,
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
        let (parsed, diags) = if detection::detect_file_kind(&uri) == FileKind::Series {
            let parsed = patchkit::edit::series::parse(&text);
            let mut diags: Vec<Diagnostic> = parsed
                .positioned_errors()
                .iter()
                .map(|e| Diagnostic {
                    range: position::text_range_to_lsp_range(&text, e.position),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("parse-error".to_string())),
                    source: Some("diff-lsp".to_string()),
                    message: e.message.clone(),
                    ..Default::default()
                })
                .collect();
            let series = parsed.tree();
            diags.extend(series_diagnostics::get_series_diagnostics(
                &text, &series, &uri,
            ));
            (ParsedFile::Series(parsed), diags)
        } else {
            let parsed = patchkit::edit::parse(&text);
            let mut diags = diagnostics::get_diagnostics(&text, &parsed);
            diags.extend(patch_quilt_diagnostics::get_patch_quilt_diagnostics(&uri));
            (ParsedFile::Patch(parsed), diags)
        };

        let mut files = self.files.lock().await;
        files.insert(uri.clone(), FileInfo { text, parsed });
        drop(files);

        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: series_code_actions::COMMANDS
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    ..Default::default()
                }),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["/".to_string(), "-".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
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
                                    SemanticTokenType::new("seriesPatchName"),
                                    SemanticTokenType::new("seriesOption"),
                                    SemanticTokenType::new("seriesComment"),
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(_) => None,
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let items = series_completions::get_series_completions(&series, uri);
                if items.is_empty() {
                    None
                } else {
                    Some(CompletionResponse::Array(items))
                }
            }
        };
        drop(files);

        Ok(result)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                Some(folding::generate_folding_ranges(&patch, &file_info.text))
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let ranges =
                    series_folding::generate_series_folding_ranges(&series, &file_info.text);
                if ranges.is_empty() {
                    None
                } else {
                    Some(ranges)
                }
            }
        };
        drop(files);

        Ok(result)
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

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                Some(DocumentSymbolResponse::Nested(
                    symbols::generate_document_symbols(&patch, &file_info.text),
                ))
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let syms = series_symbols::generate_series_symbols(&series, &file_info.text);
                if syms.is_empty() {
                    None
                } else {
                    Some(DocumentSymbolResponse::Nested(syms))
                }
            }
        };
        drop(files);

        Ok(result)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                let mut lenses = code_lenses::get_code_lenses(&patch, &file_info.text);
                lenses.extend(patch_quilt_lenses::get_patch_quilt_lenses(uri));
                if lenses.is_empty() {
                    None
                } else {
                    Some(lenses)
                }
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let lenses =
                    series_code_lenses::get_series_code_lenses(&series, &file_info.text, uri);
                if lenses.is_empty() {
                    None
                } else {
                    Some(lenses)
                }
            }
        };
        drop(files);

        Ok(result)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let range = params.range;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                let mut actions =
                    code_actions::get_code_actions(&patch, &file_info.text, range, uri);
                actions.extend(patch_quilt_actions::get_patch_quilt_actions(uri));
                if actions.is_empty() {
                    None
                } else {
                    Some(
                        actions
                            .into_iter()
                            .map(CodeActionOrCommand::CodeAction)
                            .collect(),
                    )
                }
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let mut actions = series_code_actions::get_series_code_actions(
                    &series,
                    &file_info.text,
                    range,
                    uri,
                );
                actions.extend(series_reorder::get_reorder_actions(
                    &series,
                    &file_info.text,
                    range,
                    uri,
                ));
                if actions.is_empty() {
                    None
                } else {
                    Some(
                        actions
                            .into_iter()
                            .map(CodeActionOrCommand::CodeAction)
                            .collect(),
                    )
                }
            }
        };
        drop(files);

        Ok(result)
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let range = params.range;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(_) => None,
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let hints = series_inlay_hints::get_series_inlay_hints(
                    &series,
                    &file_info.text,
                    range,
                    uri,
                );
                if hints.is_empty() {
                    None
                } else {
                    Some(hints)
                }
            }
        };
        drop(files);

        Ok(result)
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

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                let hl = highlights::get_highlights(&patch, &file_info.text, position);
                if hl.is_empty() {
                    None
                } else {
                    Some(hl)
                }
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let hl =
                    series_highlights::get_series_highlights(&series, &file_info.text, position);
                if hl.is_empty() {
                    None
                } else {
                    Some(hl)
                }
            }
        };
        drop(files);

        Ok(result)
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let links = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                document_links::get_document_links(&patch, &file_info.text, uri)
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                series_links::get_document_links(&series, &file_info.text, uri)
            }
        };
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

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                hover::get_hover(&patch, &file_info.text, position)
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                series_hover::get_series_hover(&series, &file_info.text, position, uri)
            }
        };
        drop(files);

        Ok(result)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                goto_definition::goto_definition(&patch, &file_info.text, position, uri)
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                goto_definition::series_goto_definition(&series, &file_info.text, position, uri)
            }
        };
        drop(files);

        Ok(result)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let position = params.position;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(_) => None,
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                series_rename::prepare_rename(&series, &file_info.text, position)
            }
        };
        drop(files);

        Ok(result)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;

        let files = self.files.lock().await;
        let Some(file_info) = files.get(uri) else {
            return Ok(None);
        };

        let result = match &file_info.parsed {
            ParsedFile::Patch(_) => None,
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                series_rename::rename(&series, &file_info.text, position, new_name, uri)
            }
        };
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

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                let tokens = semantic::generate_semantic_tokens(&patch, &file_info.text);
                Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: tokens,
                }))
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                let tokens =
                    series_semantic::generate_series_semantic_tokens(&series, &file_info.text);
                if tokens.is_empty() {
                    None
                } else {
                    Some(SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data: tokens,
                    }))
                }
            }
        };
        drop(files);

        Ok(result)
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

        let result = match &file_info.parsed {
            ParsedFile::Patch(parsed) => {
                let patch = parsed.tree();
                Some(selection_ranges::get_selection_ranges(
                    &patch,
                    &file_info.text,
                    &params.positions,
                ))
            }
            ParsedFile::Series(parsed) => {
                let series = parsed.tree();
                Some(series_selection_ranges::get_series_selection_ranges(
                    &series,
                    &file_info.text,
                    &params.positions,
                ))
            }
        };
        drop(files);

        Ok(result)
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<tower_lsp_server::ls_types::LSPAny>> {
        use series_code_actions::*;

        let command = params.command.as_str();
        let args = &params.arguments;

        let series_uri = args
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| tower_lsp_server::jsonrpc::Error::invalid_params("missing URI"))?;

        let working_dir = find_quilt_working_dir(series_uri).ok_or_else(|| {
            tower_lsp_server::jsonrpc::Error::invalid_params("cannot determine working directory")
        })?;

        let result = match command {
            CMD_QUILT_PUSH => {
                let patch_name = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("missing patch name")
                })?;
                run_quilt_command(&["push", patch_name], &working_dir)
            }
            CMD_QUILT_POP => {
                let patch_name = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("missing patch name")
                })?;
                run_quilt_command(&["pop", patch_name], &working_dir)
            }
            CMD_QUILT_PUSH_ALL => run_quilt_command(&["push", "-a"], &working_dir),
            CMD_QUILT_POP_ALL => run_quilt_command(&["pop", "-a"], &working_dir),
            CMD_QUILT_DELETE => {
                let patch_name = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("missing patch name")
                })?;
                run_quilt_command(&["delete", patch_name], &working_dir)
            }
            CMD_QUILT_REFRESH => run_quilt_command(&["refresh"], &working_dir),
            CMD_QUILT_NEW => {
                let patch_name = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("missing patch name")
                })?;
                run_quilt_command(&["new", patch_name], &working_dir)
            }
            CMD_QUILT_IMPORT => {
                // The first arg is the patch file URI — convert to a filesystem path
                let patch_uri: Uri = series_uri
                    .parse()
                    .map_err(|_| tower_lsp_server::jsonrpc::Error::invalid_params("invalid URI"))?;
                let patch_path = patch_uri.to_file_path().ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("invalid file URI")
                })?;
                let patch_path_str = patch_path.to_str().ok_or_else(|| {
                    tower_lsp_server::jsonrpc::Error::invalid_params("non-UTF-8 path")
                })?;
                run_quilt_command(&["import", patch_path_str], &working_dir)
            }
            _ => {
                return Err(tower_lsp_server::jsonrpc::Error::method_not_found());
            }
        };

        match result {
            Ok(output) => {
                self.client.log_message(MessageType::INFO, &output).await;
                Ok(Some(serde_json::Value::String(output)))
            }
            Err(err) => {
                self.client.log_message(MessageType::ERROR, &err).await;
                Err(tower_lsp_server::jsonrpc::Error::internal_error())
            }
        }
    }
}

fn print_usage() {
    eprintln!(
        "Usage:\n  \
         diff-lsp                       run the language server (reads/writes stdio)"
    );
    #[cfg(feature = "scip")]
    eprintln!(
        "  diff-lsp scip [-o OUTPUT] FILE...  generate a SCIP index for the given patch/series files\n\n\
         The scip subcommand writes a binary SCIP index (default: index.scip).\n\
         Paths in the index are made relative to the current working directory."
    );
}

/// Run the `scip` subcommand.
///
/// Parses `[-o OUTPUT] FILE...` and writes a SCIP index covering the given
/// patch and quilt series files.
#[cfg(feature = "scip")]
fn run_scip(args: &[String]) -> std::process::ExitCode {
    let mut output = std::path::PathBuf::from("index.scip");
    let mut inputs: Vec<std::path::PathBuf> = Vec::new();

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                let Some(value) = iter.next() else {
                    eprintln!("error: {arg} requires an argument");
                    return std::process::ExitCode::FAILURE;
                };
                output = std::path::PathBuf::from(value);
            }
            "-h" | "--help" => {
                print_usage();
                return std::process::ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown option {other}");
                return std::process::ExitCode::FAILURE;
            }
            other => inputs.push(std::path::PathBuf::from(other)),
        }
    }

    if inputs.is_empty() {
        eprintln!("error: no input files given");
        print_usage();
        return std::process::ExitCode::FAILURE;
    }

    let project_root = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("error: cannot determine current directory: {err}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let index = match scip::build_index(&inputs, &project_root) {
        Ok(index) => index,
        Err(err) => {
            eprintln!("error: failed to build index: {err}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut references = 0usize;
    let mut diagnostics = 0usize;
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.diagnostics.is_empty() {
                references += 1;
            } else {
                diagnostics += occ.diagnostics.len();
            }
        }
    }

    if let Err(err) = scip::write_message_to_file(&output, index) {
        eprintln!("error: failed to write {}: {err}", output.display());
        return std::process::ExitCode::FAILURE;
    }

    eprintln!(
        "wrote {} ({} document(s), {references} reference(s), {diagnostics} diagnostic(s))",
        output.display(),
        inputs.len()
    );
    std::process::ExitCode::SUCCESS
}

async fn run_lsp() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        #[cfg(feature = "scip")]
        Some("scip") => run_scip(&args[1..]),
        Some("-h") | Some("--help") => {
            print_usage();
            std::process::ExitCode::SUCCESS
        }
        Some(other) if other.starts_with('-') => {
            // No options are accepted before the LSP server mode; treat
            // unknown leading options as an error rather than silently
            // starting the server.
            eprintln!("error: unknown option {other}");
            print_usage();
            std::process::ExitCode::FAILURE
        }
        Some(other) => {
            eprintln!("error: unknown subcommand {other}");
            print_usage();
            std::process::ExitCode::FAILURE
        }
        None => {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(err) => {
                    eprintln!("error: failed to start tokio runtime: {err}");
                    return std::process::ExitCode::FAILURE;
                }
            };
            runtime.block_on(run_lsp());
            std::process::ExitCode::SUCCESS
        }
    }
}
