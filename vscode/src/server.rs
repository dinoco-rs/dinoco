use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;

use tower_lsp::jsonrpc::{Error as LspError, Result as LspResult};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::completion;
use crate::completion::ImportCompletionContext;
use crate::diagnostics::{self, CODE_MISSING_CONFIG, CODE_MISSING_DATABASE_URL, CODE_UNKNOWN_TYPE};
use crate::document::{BlockKind, DocumentIndex, ResolvedSymbol, scalar_types};
use crate::workspace::{ImportGraph, SchemaFile, WorkspaceCache, canonical_path, import_path_suggestions};

#[derive(Debug, Clone)]
struct DocumentState {
    file: Arc<SchemaFile>,
    version: i32,
}

impl DocumentState {
    fn new(text: String, version: i32) -> Self {
        Self { file: Arc::new(SchemaFile::new(text)), version }
    }

    fn text(&self) -> &str {
        &self.file.source
    }

    fn index(&self) -> &DocumentIndex {
        &self.file.index
    }
}

pub struct DinocoLanguageServer {
    client: Client,
    documents: RwLock<HashMap<Url, DocumentState>>,
    workspace: RwLock<WorkspaceCache>,
    project_diagnostic_files: RwLock<HashMap<PathBuf, HashSet<Url>>>,
}

impl DinocoLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
            workspace: RwLock::new(WorkspaceCache::default()),
            project_diagnostic_files: RwLock::new(HashMap::new()),
        }
    }

    fn document(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.read().ok()?.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String, version: i32) {
        let state = DocumentState::new(text, version);
        let diagnostics = if is_main_schema(&uri) {
            diagnostics::analyze(state.text(), state.index())
        } else {
            diagnostics::analyze_imported(state.text(), state.index())
        };
        if let Ok(mut documents) = self.documents.write() {
            documents.insert(uri.clone(), state);
        }
        if let Ok(path) = uri.to_file_path()
            && let Ok(mut workspace) = self.workspace.write()
        {
            workspace.invalidate(&path);
        }
        self.client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
        self.validate_project_imports(&uri).await;
    }

    fn hover_for(&self, state: &DocumentState, index: &DocumentIndex, position: Position) -> Option<Hover> {
        let token = state.index().token_symbol_at(position)?;
        let contents = if let Some(symbol) = index.resolve_symbol(position) {
            hover_resolved_symbol(index, &symbol)?
        } else if scalar_types().contains(&token.name.as_str()) {
            format!("```dinoco\n{}\n```\n\n{}", token.name, scalar_description(&token.name))
        } else if let Some(description) = keyword_description(&token.name) {
            description.to_string()
        } else if let Some(description) = attribute_description(&token.name) {
            description.to_string()
        } else if let Some(description) = config_description(&token.name) {
            description.to_string()
        } else {
            return None;
        };

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: contents }),
            range: Some(token.range),
        })
    }

    fn quick_fixes(
        &self,
        uri: &Url,
        state: &DocumentState,
        index: &DocumentIndex,
        params: &CodeActionParams,
    ) -> Vec<CodeActionOrCommand> {
        let mut actions = Vec::new();
        for item in &params.context.diagnostics {
            let Some(NumberOrString::String(code)) = &item.code else {
                continue;
            };
            let edit = match code.as_str() {
                CODE_MISSING_CONFIG => Some(TextEdit {
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                    new_text: "config {\n    database = \"postgresql\"\n    database_url = env(\"DATABASE_URL\")\n    read_replicas = []\n}\n\n".to_string(),
                }),
                CODE_MISSING_DATABASE_URL => state.index().config().map(|config| TextEdit {
                    range: Range::new(config.body_range.start, config.body_range.start),
                    new_text: "\n    database_url = env(\"DATABASE_URL\")".to_string(),
                }),
                CODE_UNKNOWN_TYPE => unknown_type_fix(index, item),
                _ => None,
            };
            let Some(edit) = edit else {
                continue;
            };
            let title = match code.as_str() {
                CODE_MISSING_CONFIG => "Add Dinoco config block".to_string(),
                CODE_MISSING_DATABASE_URL => "Add environment-backed database URL".to_string(),
                CODE_UNKNOWN_TYPE => format!("Replace with `{}`", edit.new_text),
                _ => continue,
            };
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![item.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
                    ..WorkspaceEdit::default()
                }),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        }
        actions
    }

    fn overlays(&self) -> HashMap<PathBuf, Arc<SchemaFile>> {
        self.documents
            .read()
            .map(|documents| {
                documents
                    .iter()
                    .filter_map(|(uri, state)| {
                        let path = uri.to_file_path().ok()?;
                        Some((canonical_path(&path)?, state.file.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn import_graph(&self, uri: &Url) -> Option<(PathBuf, ImportGraph)> {
        let path = canonical_path(&uri.to_file_path().ok()?)?;
        let overlays = self.overlays();
        let graph = self.workspace.write().ok()?.load_graph(&path, &overlays);
        Some((path, graph))
    }

    fn project_import_graph(&self, uri: &Url) -> Option<(PathBuf, ImportGraph)> {
        let path = canonical_path(&uri.to_file_path().ok()?)?;
        let root = self.project_root(&path)?;
        let overlays = self.overlays();
        let mut workspace = self.workspace.write().ok()?;
        let mut graph = workspace.load_graph(&root, &overlays);
        if !graph.files.contains_key(&path) {
            graph = workspace.load_graph(&path, &overlays);
        }
        Some((path, graph))
    }

    fn semantic_index(&self, uri: &Url, state: &DocumentState) -> (DocumentIndex, Option<(PathBuf, ImportGraph)>) {
        let Some((path, graph)) = self.import_graph(uri) else {
            return (state.index().clone(), None);
        };
        let index = state.index().with_external_declarations(graph.visible_declarations(&path));
        (index, Some((path, graph)))
    }

    fn import_completion(
        &self,
        uri: &Url,
        _state: &DocumentState,
        context: ImportCompletionContext,
    ) -> Option<CompletionResponse> {
        let current = uri.to_file_path().ok()?;
        match context {
            ImportCompletionContext::Symbols { path: Some(import_path) } => {
                if Path::new(&import_path).extension().and_then(|extension| extension.to_str()) != Some("dinoco") {
                    return Some(CompletionResponse::Array(Vec::new()));
                }
                let overlays = self.overlays();
                let (_, target) = self.workspace.write().ok()?.load_import_target(&current, &import_path, &overlays)?;
                Some(completion::import_symbol_completions(&target.index))
            }
            ImportCompletionContext::Symbols { path: None } => Some(CompletionResponse::Array(Vec::new())),
            ImportCompletionContext::Path { fragment, replace, quoted } => {
                Some(completion::import_path_completions(import_path_suggestions(&current, &fragment), replace, quoted))
            }
        }
    }

    fn project_root(&self, file: &Path) -> Option<PathBuf> {
        if file.file_name().and_then(|name| name.to_str()) == Some("schema.dinoco") {
            return canonical_path(file);
        }
        if let Some(root) = self.workspace.read().ok()?.known_root_for(file) {
            return Some(root);
        }
        for directory in file.parent()?.ancestors() {
            let candidate = directory.join("schema.dinoco");
            if candidate.is_file() {
                return canonical_path(&candidate);
            }
        }
        None
    }

    async fn validate_project_on_save(&self, uri: &Url) {
        let Ok(saved_path) = uri.to_file_path() else {
            return;
        };
        if let Ok(mut workspace) = self.workspace.write() {
            workspace.invalidate(&saved_path);
        }
        let Some(root) = self.project_root(&saved_path) else {
            return;
        };
        let overlays = self.overlays();
        let graph = match self.workspace.write() {
            Ok(mut workspace) => workspace.load_graph(&root, &overlays),
            Err(_) => return,
        };
        let compile_error = dinoco_compiler::compile_file(&root).err();
        self.publish_project_diagnostics(&root, graph, compile_error).await;
    }

    async fn validate_project_imports(&self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let Some(root) = self.project_root(&path) else {
            return;
        };
        let overlays = self.overlays();
        let graph = match self.workspace.write() {
            Ok(mut workspace) => workspace.load_graph(&root, &overlays),
            Err(_) => return,
        };
        self.publish_project_diagnostics(&root, graph, None).await;
    }

    async fn publish_project_diagnostics(
        &self,
        root: &Path,
        graph: ImportGraph,
        compile_error: Option<dinoco_compiler::CompileError>,
    ) {
        let mut files = graph.files.keys().filter_map(|path| Url::from_file_path(path).ok()).collect::<HashSet<_>>();
        let previous = self
            .project_diagnostic_files
            .read()
            .ok()
            .and_then(|projects| projects.get(root).cloned())
            .unwrap_or_default();
        let mut project_diagnostic = None;
        if let Some(error) = compile_error {
            project_diagnostic = compile_error_diagnostic(root, &graph, error);
            if let Some((uri, _)) = &project_diagnostic {
                files.insert(uri.clone());
            }
        }

        let publish_files = files.union(&previous).cloned().collect::<Vec<_>>();
        for uri in publish_files {
            let state = self.document(&uri);
            let canonical = uri.to_file_path().ok().and_then(|path| canonical_path(&path));
            let graph_file = canonical.as_ref().and_then(|path| graph.files.get(path).cloned());
            let file = state.as_ref().map(|state| state.file.clone()).or(graph_file);
            let mut diagnostics = file.as_ref().map_or_else(Vec::new, |file| {
                if canonical.as_deref() == Some(root) {
                    diagnostics::analyze(&file.source, &file.index)
                } else {
                    diagnostics::analyze_imported(&file.source, &file.index)
                }
            });
            if let (Some(path), Some(file)) = (canonical.as_deref(), file.as_deref()) {
                diagnostics.extend(import_diagnostics(path, file, &graph));
            }
            if let Some((target, diagnostic)) = &project_diagnostic
                && target == &uri
                && !diagnostics.iter().any(|item| item.message == diagnostic.message)
            {
                diagnostics.push(diagnostic.clone());
            }
            self.client.publish_diagnostics(uri, diagnostics, state.map(|state| state.version)).await;
        }
        if let Ok(mut tracked) = self.project_diagnostic_files.write() {
            tracked.insert(root.to_path_buf(), files);
        }
    }

    /// Resolves the effective `FormatterConfig` for a formatting request.
    ///
    /// `dinoco.formatter.*` settings aren't part of the standard LSP
    /// `FormattingOptions`, so they're pulled directly from the client via
    /// `workspace/configuration` (which `vscode-languageclient` answers using
    /// `vscode.workspace.getConfiguration`, scoped to the document, with no
    /// extra client-side code required). Falls back to the editor's
    /// `tabSize`/`insertSpaces` when a client doesn't answer a given section.
    async fn resolve_formatter_config(&self, params: &DocumentFormattingParams) -> dinoco_formatter::FormatterConfig {
        let scope_uri = Some(params.text_document.uri.clone());
        let sections = [
            "dinoco.formatter.maxWidth",
            "dinoco.formatter.useTabs",
            "dinoco.formatter.useSpaces",
            "dinoco.formatter.indentSize",
            "dinoco.formatter.removeComments",
        ];
        let items = sections
            .iter()
            .map(|section| ConfigurationItem { scope_uri: scope_uri.clone(), section: Some(section.to_string()) })
            .collect();

        let values = self.client.configuration(items).await.unwrap_or_default();
        let value_at = |index: usize| values.get(index);

        let max_width = value_at(0).and_then(|value| value.as_u64()).map(|value| value as usize);
        let use_tabs_setting = value_at(1).and_then(|value| value.as_bool());
        let use_spaces_setting = value_at(2).and_then(|value| value.as_bool());
        let indent_size = value_at(3).and_then(|value| value.as_u64()).map(|value| value as usize);
        let remove_comments = value_at(4).and_then(|value| value.as_bool());

        let use_tabs = use_tabs_setting
            .or(use_spaces_setting.map(|use_spaces| !use_spaces))
            .unwrap_or(!params.options.insert_spaces);

        dinoco_formatter::FormatterConfig {
            indent_width: indent_size.unwrap_or(params.options.tab_size.max(1) as usize).max(1),
            final_newline: true,
            use_tabs,
            max_width: max_width.unwrap_or(100).max(1),
            strip_comments: remove_comments.unwrap_or(false),
        }
    }
}

fn import_diagnostics(path: &Path, file: &SchemaFile, graph: &ImportGraph) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for import in file.imports() {
        let invalid_path_message = if import.path.trim().is_empty() {
            Some("Import path cannot be empty".to_string())
        } else if Path::new(&import.path).is_absolute() {
            Some("Import paths must be relative to the declaring file".to_string())
        } else if Path::new(&import.path).extension().and_then(|extension| extension.to_str()) != Some("dinoco") {
            Some("Imported schema paths must end in `.dinoco`".to_string())
        } else {
            None
        };
        if let Some(message) = invalid_path_message {
            diagnostics.push(Diagnostic {
                range: import_value_range(&file.source, &import.path, &import.path),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("dinoco.invalidImportPath".to_string())),
                source: Some("dinoco".to_string()),
                message,
                ..Diagnostic::default()
            });
            continue;
        }
        let Some(target_path) = crate::workspace::resolve_import_path(path, &import.path) else {
            diagnostics.push(Diagnostic {
                range: import_value_range(&file.source, &import.path, &import.path),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("dinoco.importFileNotFound".to_string())),
                source: Some("dinoco".to_string()),
                message: format!("Import file `{}` was not found.", import.path),
                ..Diagnostic::default()
            });
            continue;
        };
        let Some(target) = graph.files.get(&target_path) else {
            continue;
        };
        for symbol in &import.symbols {
            let exists = target.index.blocks.iter().any(|block| {
                matches!(block.kind, BlockKind::Model | BlockKind::Enum)
                    && block.name.as_ref().is_some_and(|name| name.name == *symbol)
            });
            if !exists {
                diagnostics.push(Diagnostic {
                    range: import_value_range(&file.source, &import.path, symbol),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("dinoco.importSymbolNotFound".to_string())),
                    source: Some("dinoco".to_string()),
                    message: format!("Imported symbol `{symbol}` was not found in `{}`.", import.path),
                    ..Diagnostic::default()
                });
            }
        }
    }
    diagnostics
}

fn import_value_range(source: &str, import_path: &str, value: &str) -> Range {
    let quoted_path = format!("\"{import_path}\"");
    let path_offset = source.find(&quoted_path).map(|offset| offset + 1);
    let offset = if value == import_path {
        path_offset
    } else {
        path_offset.and_then(|path_offset| {
            let import_start = source[..path_offset].rfind("import").unwrap_or_default();
            source[import_start..path_offset].find(value).map(|offset| import_start + offset)
        })
    };
    let Some(offset) = offset.or_else(|| source.find(value)) else {
        return Range::default();
    };
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = source[line_start..offset].encode_utf16().count() as u32;
    let start = Position::new(line, character);
    Range::new(start, Position::new(line, character + value.encode_utf16().count() as u32))
}

fn is_main_schema(uri: &Url) -> bool {
    let Some(mut segments) = uri.path_segments() else {
        return false;
    };
    segments.next_back() == Some("schema.dinoco") && segments.next_back() == Some("dinoco")
}

fn format_document_source(
    uri: &Url,
    source: &str,
    config: &dinoco_formatter::FormatterConfig,
) -> Result<String, dinoco_compiler::CompileError> {
    if is_main_schema(uri) {
        dinoco_formatter::format_from_raw_with_config(source, config)
    } else {
        dinoco_formatter::format_fragment_from_raw_with_config(source, config)
    }
}

fn compile_error_diagnostic(
    root: &Path,
    graph: &ImportGraph,
    error: dinoco_compiler::CompileError,
) -> Option<(Url, Diagnostic)> {
    let path = diagnostic_path(root, error.file.as_deref());
    let uri = Url::from_file_path(&path).ok()?;
    let source = graph
        .files
        .get(&path)
        .map(|file| file.source.clone())
        .or_else(|| std::fs::read_to_string(&path).ok())
        .unwrap_or_default();
    let range = compiler_error_range(&source, &error);
    let related_information = error
        .related
        .into_iter()
        .filter_map(|related| {
            let path = diagnostic_path(root, Some(&related.file));
            let uri = Url::from_file_path(&path).ok()?;
            let source = graph
                .files
                .get(&path)
                .map(|file| file.source.clone())
                .or_else(|| std::fs::read_to_string(&path).ok())
                .unwrap_or_default();
            let start = compiler_position(&source, related.line, related.column);
            Some(DiagnosticRelatedInformation {
                location: Location::new(uri, Range::new(start, Position::new(start.line, start.character + 1))),
                message: related.message,
            })
        })
        .collect::<Vec<_>>();
    Some((
        uri,
        Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("dinoco.project".to_string())),
            source: Some("dinoco".to_string()),
            message: error.message,
            related_information: (!related_information.is_empty()).then_some(related_information),
            ..Diagnostic::default()
        },
    ))
}

fn compiler_error_range(source: &str, error: &dinoco_compiler::CompileError) -> Range {
    let fallback = compiler_position(source, error.line, error.column);
    let target = error
        .message
        .strip_prefix("Imported symbol `")
        .and_then(|message| message.split('`').next())
        .or_else(|| error.message.strip_prefix("Imported schema `").and_then(|message| message.split('`').next()));
    let target =
        target.or_else(|| error.message.strip_prefix("Import file `").and_then(|message| message.split('`').next()));
    let Some(target) = target else {
        return Range::new(fallback, Position::new(fallback.line, fallback.character + 1));
    };
    let Some(line) = source.lines().nth(error.line.saturating_sub(1)) else {
        return Range::new(fallback, Position::new(fallback.line, fallback.character + 1));
    };
    let Some(byte_column) = line.find(target) else {
        return Range::new(fallback, Position::new(fallback.line, fallback.character + 1));
    };
    let character = line[..byte_column].encode_utf16().count() as u32;
    let start = Position::new(error.line.saturating_sub(1) as u32, character);
    Range::new(start, Position::new(start.line, start.character + target.encode_utf16().count() as u32))
}

fn diagnostic_path(root: &Path, file: Option<&str>) -> PathBuf {
    let Some(file) = file else {
        return root.to_path_buf();
    };
    let file = Path::new(file);
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.parent().unwrap_or_else(|| Path::new(".")).join(file)
    };
    canonical_path(&path).unwrap_or(path)
}

fn compiler_position(source: &str, line: usize, column: usize) -> Position {
    let line_index = line.saturating_sub(1);
    let character = source
        .lines()
        .nth(line_index)
        .unwrap_or_default()
        .chars()
        .take(column.saturating_sub(1))
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    Position::new(line_index as u32, character)
}

#[tower_lsp::async_trait]
impl LanguageServer for DinocoLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions { include_text: Some(true) })),
                    ..TextDocumentSyncOptions::default()
                })),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".into(),
                        " ".into(),
                        "(".into(),
                        ",".into(),
                        ":".into(),
                        "=".into(),
                        "[".into(),
                        "{".into(),
                        "\"".into(),
                        "/".into(),
                        ".".into(),
                    ]),
                    ..CompletionOptions::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into(), ":".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    ..SignatureHelpOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        legend: semantic_tokens_legend(),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        ..SemanticTokensOptions::default()
                    },
                )),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "Dinoco Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "Dinoco language intelligence is ready.").await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text, params.text_document.version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.update_document(params.text_document.uri, change.text, params.text_document.version).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            let version = self.document(&params.text_document.uri).map_or(0, |document| document.version);
            self.update_document(params.text_document.uri.clone(), text, version).await;
        }
        self.validate_project_on_save(&params.text_document.uri).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut roots = HashSet::new();
        if let Ok(mut workspace) = self.workspace.write() {
            for change in params.changes {
                if let Ok(path) = change.uri.to_file_path() {
                    roots.extend(workspace.known_roots_affected_by(&path));
                    for directory in path.parent().into_iter().flat_map(Path::ancestors) {
                        let candidate = directory.join("schema.dinoco");
                        if candidate.is_file()
                            && let Some(candidate) = canonical_path(&candidate)
                        {
                            roots.insert(candidate);
                            break;
                        }
                    }
                    workspace.invalidate(&path);
                }
            }
        }
        for root in roots {
            if let Ok(uri) = Url::from_file_path(root) {
                self.validate_project_on_save(&uri).await;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Ok(mut documents) = self.documents.write() {
            documents.remove(&params.text_document.uri);
        }
        self.client.publish_diagnostics(params.text_document.uri, Vec::new(), None).await;
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        if let Some(context) = completion::import_completion_context(state.text(), position) {
            return Ok(self.import_completion(&uri, &state, context));
        }
        let (index, _) = self.semantic_index(&uri, &state);
        Ok(Some(completion::complete(state.text(), &index, position)))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> LspResult<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        Ok(completion::signature(state.text(), position))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&uri, &state);
        Ok(self.hover_for(&state, &index, position))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, graph) = self.semantic_index(&uri, &state);
        let Some(symbol) = index.resolve_symbol(position) else {
            return Ok(None);
        };
        if let Some((path, graph)) = graph
            && let Some((definition_path, range)) = graph.definition(&path, &symbol)
            && let Ok(definition_uri) = Url::from_file_path(definition_path)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location::new(definition_uri, range))));
        }
        let Some(range) = state.index().definition(&symbol) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location::new(uri, range))))
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&uri, &state);
        let Some(symbol) = index.resolve_symbol(position) else {
            return Ok(None);
        };
        let locations = if let Some((current, graph)) = self.project_import_graph(&uri) {
            let definition = graph.definition(&current, &symbol).map(|(path, range)| (path.to_path_buf(), range));
            graph
                .files
                .iter()
                .filter_map(|(path, file)| Some((Url::from_file_path(path).ok()?, path, file)))
                .flat_map(|(item_uri, path, file)| {
                    let definition = definition.clone();
                    file.index
                        .occurrences(&symbol)
                        .into_iter()
                        .filter(move |range| {
                            params.context.include_declaration
                                || definition.as_ref().is_none_or(|(definition_path, definition_range)| {
                                    definition_path != path || definition_range != range
                                })
                        })
                        .map(move |range| Location::new(item_uri.clone(), range))
                })
                .collect()
        } else {
            let definition = state.index().definition(&symbol);
            state
                .index()
                .occurrences(&symbol)
                .into_iter()
                .filter(|range| params.context.include_declaration || Some(*range) != definition)
                .map(|range| Location::new(uri.clone(), range))
                .collect()
        };
        Ok(Some(locations))
    }

    async fn document_highlight(&self, params: DocumentHighlightParams) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&uri, &state);
        let Some(symbol) = index.resolve_symbol(position) else {
            return Ok(None);
        };
        Ok(Some(
            state
                .index()
                .occurrences(&symbol)
                .into_iter()
                .map(|range| DocumentHighlight { range, kind: Some(DocumentHighlightKind::TEXT) })
                .collect(),
        ))
    }

    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> LspResult<Option<PrepareRenameResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&params.text_document.uri, &state);
        if index.resolve_symbol(params.position).is_none() {
            return Ok(None);
        }
        Ok(state
            .index()
            .token_symbol_at(params.position)
            .map(|token| PrepareRenameResponse::RangeWithPlaceholder { range: token.range, placeholder: token.name }))
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        if !is_identifier(&params.new_name) {
            return Err(LspError::invalid_params("Dinoco names must be valid identifiers."));
        }
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&uri, &state);
        let Some(symbol) = index.resolve_symbol(position) else {
            return Ok(None);
        };
        let changes = if let Some((_, graph)) = self.project_import_graph(&uri) {
            graph
                .files
                .iter()
                .filter_map(|(path, file)| {
                    let edits = file
                        .index
                        .occurrences(&symbol)
                        .into_iter()
                        .map(|range| TextEdit { range, new_text: params.new_name.clone() })
                        .collect::<Vec<_>>();
                    if edits.is_empty() { None } else { Some((Url::from_file_path(path).ok()?, edits)) }
                })
                .collect()
        } else {
            let edits = state
                .index()
                .occurrences(&symbol)
                .into_iter()
                .map(|range| TextEdit { range, new_text: params.new_name.clone() })
                .collect();
            HashMap::from([(uri, edits)])
        };
        Ok(Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }))
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> LspResult<Option<DocumentSymbolResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let symbols = state.index().blocks.iter().map(document_symbol).collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(
            state
                .index()
                .blocks
                .iter()
                .filter(|block| block.range.start.line < block.range.end.line)
                .map(|block| FoldingRange {
                    start_line: block.range.start.line,
                    start_character: Some(block.range.start.character),
                    end_line: block.range.end.line,
                    end_character: Some(block.range.end.character),
                    kind: None,
                    collapsed_text: block
                        .name
                        .as_ref()
                        .map(|name| format!("{} {}", block_kind_name(block.kind), name.name)),
                })
                .collect(),
        ))
    }

    async fn selection_range(&self, params: SelectionRangeParams) -> LspResult<Option<Vec<SelectionRange>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let ranges = params
            .positions
            .into_iter()
            .map(|position| {
                let document = SelectionRange {
                    range: Range::new(Position::new(0, 0), state.index().end_position()),
                    parent: None,
                };
                let Some(block) = state.index().block_at(position) else {
                    return document;
                };
                let block_range = SelectionRange { range: block.range, parent: Some(Box::new(document)) };
                if let Some((_, field)) = state.index().field_at(position) {
                    SelectionRange { range: field.range, parent: Some(Box::new(block_range)) }
                } else {
                    block_range
                }
            })
            .collect();
        Ok(Some(ranges))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> LspResult<Option<Vec<TextEdit>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let config = self.resolve_formatter_config(&params).await;
        let formatted = format_document_source(&params.text_document.uri, state.text(), &config)
            .map_err(|error| LspError::invalid_params(format!("Cannot format an invalid Dinoco schema: {error}")))?;
        if formatted == state.text() {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: Range::new(Position::new(0, 0), state.index().end_position()),
            new_text: formatted,
        }]))
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let (index, _) = self.semantic_index(&uri, &state);
        Ok(Some(self.quick_fixes(&uri, &state, &index, &params)))
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> LspResult<Option<SemanticTokensResult>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let data = encode_semantic_tokens(semantic_token_spans(state.index()));
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data })))
    }
}

fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::TYPE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::DECORATOR,
        ],
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

const SEMANTIC_TOKEN_TYPE: u32 = 0;
const SEMANTIC_TOKEN_PROPERTY: u32 = 1;
const SEMANTIC_TOKEN_ENUM_MEMBER: u32 = 2;
const SEMANTIC_TOKEN_DECORATOR: u32 = 3;
const SEMANTIC_MODIFIER_DECLARATION: u32 = 1;

/// Builds `(range, token_type, modifiers)` spans straight from the same
/// `DocumentIndex` used for hover/completion/go-to-definition, so semantic
/// highlighting reflects each identifier's real role (type declaration vs.
/// reference, field vs. enum member, etc.) rather than a regex guess.
fn semantic_token_spans(index: &DocumentIndex) -> Vec<(Range, u32, u32)> {
    let mut spans = Vec::new();
    let scalars = scalar_types();

    for block in &index.blocks {
        if matches!(block.kind, BlockKind::Model | BlockKind::Enum)
            && let Some(name) = &block.name
        {
            spans.push((name.range, SEMANTIC_TOKEN_TYPE, SEMANTIC_MODIFIER_DECLARATION));
        }

        for field in &block.fields {
            spans.push((field.name.range, SEMANTIC_TOKEN_PROPERTY, 0));
            if !scalars.contains(&field.ty.name.as_str()) {
                spans.push((field.ty.range, SEMANTIC_TOKEN_TYPE, 0));
            }
            for attribute in &field.attributes {
                spans.push((attribute.name.range, SEMANTIC_TOKEN_DECORATOR, 0));
            }
        }

        for attribute in &block.attributes {
            spans.push((attribute.name.range, SEMANTIC_TOKEN_DECORATOR, 0));
        }

        if block.kind == BlockKind::Enum {
            for value in &block.values {
                spans.push((value.range, SEMANTIC_TOKEN_ENUM_MEMBER, 0));
            }
        }

        if block.kind == BlockKind::Config {
            for entry in &block.entries {
                spans.push((entry.range, SEMANTIC_TOKEN_PROPERTY, 0));
            }
        }
    }

    spans
}

fn encode_semantic_tokens(mut spans: Vec<(Range, u32, u32)>) -> Vec<SemanticToken> {
    spans.sort_by_key(|(range, _, _)| (range.start.line, range.start.character));

    let mut tokens = Vec::with_capacity(spans.len());
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;

    for (range, token_type, modifiers) in spans {
        if range.end.line != range.start.line || range.end.character <= range.start.character {
            continue;
        }
        let length = range.end.character - range.start.character;
        let delta_line = range.start.line - previous_line;
        let delta_start =
            if delta_line == 0 { range.start.character - previous_start } else { range.start.character };

        tokens.push(SemanticToken { delta_line, delta_start, length, token_type, token_modifiers_bitset: modifiers });

        previous_line = range.start.line;
        previous_start = range.start.character;
    }

    tokens
}

fn hover_resolved_symbol(index: &DocumentIndex, symbol: &ResolvedSymbol) -> Option<String> {
    match symbol {
        ResolvedSymbol::Type(name) => {
            if let Some(model) = index.model(name) {
                let fields = model.fields.len();
                let relations = model.fields.iter().filter(|field| index.model(&field.ty.name).is_some()).count();
                Some(format!(
                    "```dinoco\nmodel {name}\n```\n\nPersisted model with **{fields} fields** and **{relations} relations**."
                ))
            } else {
                let item = index.enum_(name)?;
                let values = item.values.iter().map(|value| format!("`{}`", value.name)).collect::<Vec<_>>().join(", ");
                Some(format!("```dinoco\nenum {name}\n```\n\nAllowed values: {values}"))
            }
        }
        ResolvedSymbol::Field { model, field } => {
            let field = index.model(model)?.field(field)?;
            let mut output = format!("```dinoco\n{model}.{}: {}\n```", field.name.name, field.display_type());
            if index.model(&field.ty.name).is_some() {
                output.push_str("\n\nModel relation");
                if let Some(relation) = field.attribute("relation")
                    && let (Some(local), Some(references)) =
                        (relation.argument("fields"), relation.argument("references"))
                {
                    let local = local.values.iter().map(|item| item.name.as_str()).collect::<Vec<_>>().join(", ");
                    let references =
                        references.values.iter().map(|item| item.name.as_str()).collect::<Vec<_>>().join(", ");
                    output.push_str(&format!(": `[{local}]` references `{}.[{references}]`.", field.ty.name));
                }
            }
            Some(output)
        }
        ResolvedSymbol::EnumValue { enum_name, value } => {
            Some(format!("```dinoco\n{enum_name}.{value}\n```\n\nEnum value."))
        }
    }
}

#[allow(deprecated)]
fn document_symbol(block: &crate::document::BlockInfo) -> DocumentSymbol {
    let (name, selection_range, kind, detail, children) = match block.kind {
        BlockKind::Config => (
            "config".to_string(),
            Range::new(block.range.start, block.body_range.start),
            SymbolKind::OBJECT,
            Some("Dinoco configuration".to_string()),
            block
                .entries
                .iter()
                .map(|entry| DocumentSymbol {
                    name: entry.name.clone(),
                    detail: None,
                    kind: SymbolKind::KEY,
                    tags: None,
                    deprecated: None,
                    range: entry.range,
                    selection_range: entry.range,
                    children: None,
                })
                .collect(),
        ),
        BlockKind::Enum => {
            let name = block.name.as_ref().expect("enum name");
            (
                name.name.clone(),
                name.range,
                SymbolKind::ENUM,
                Some(format!("{} values", block.values.len())),
                block
                    .values
                    .iter()
                    .map(|value| DocumentSymbol {
                        name: value.name.clone(),
                        detail: None,
                        kind: SymbolKind::ENUM_MEMBER,
                        tags: None,
                        deprecated: None,
                        range: value.range,
                        selection_range: value.range,
                        children: None,
                    })
                    .collect(),
            )
        }
        BlockKind::Model => {
            let name = block.name.as_ref().expect("model name");
            (
                name.name.clone(),
                name.range,
                SymbolKind::CLASS,
                Some(format!("{} fields", block.fields.len())),
                block
                    .fields
                    .iter()
                    .map(|field| DocumentSymbol {
                        name: field.name.name.clone(),
                        detail: Some(field.display_type()),
                        kind: SymbolKind::FIELD,
                        tags: None,
                        deprecated: None,
                        range: field.range,
                        selection_range: field.name.range,
                        children: None,
                    })
                    .collect(),
            )
        }
    };

    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: block.range,
        selection_range,
        children: Some(children),
    }
}

fn unknown_type_fix(index: &DocumentIndex, diagnostic: &Diagnostic) -> Option<TextEdit> {
    let unknown = diagnostic.message.strip_prefix("Unknown type `")?.strip_suffix("`.")?;
    let mut candidates = index.type_names();
    candidates.extend(scalar_types().iter().map(|item| (*item).to_string()));
    let replacement = candidates
        .into_iter()
        .map(|candidate| {
            let distance = edit_distance(&candidate.to_lowercase(), &unknown.to_lowercase());
            (distance, candidate)
        })
        .min_by_key(|(distance, _)| *distance)?;
    if replacement.0 > (unknown.len() / 3).max(2) {
        return None;
    }
    Some(TextEdit { range: diagnostic.range, new_text: replacement.1 })
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.chars().enumerate() {
            current.push(
                (previous[right_index + 1] + 1)
                    .min(current[right_index] + 1)
                    .min(previous[right_index] + usize::from(left_char != right_char)),
            );
        }
        previous = current;
    }
    previous.last().copied().unwrap_or_default()
}

fn block_kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Config => "config",
        BlockKind::Enum => "enum",
        BlockKind::Model => "model",
    }
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters.next().is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn scalar_description(name: &str) -> &'static str {
    match name {
        "String" => "UTF-8 text. Maps to `TEXT` on PostgreSQL/SQLite and `VARCHAR(255)` on MySQL.",
        "Boolean" => "Boolean value.",
        "Integer" => "Signed 64-bit integer.",
        "Float" => "Double-precision floating-point number.",
        "DateTime" => "Date and time represented as a UTC `DateTime` in generated Rust models.",
        "Date" => "Calendar date represented as `NaiveDate` in generated Rust models.",
        "Json" => "Structured JSON value.",
        _ => "Dinoco scalar type.",
    }
}

fn keyword_description(name: &str) -> Option<&'static str> {
    match name {
        "model" => Some("Defines a persisted database model and its relations."),
        "enum" => Some("Defines a database enum and generates a matching Rust enum."),
        "config" => Some("Configures the database adapter, primary URL, replicas, and ID generation."),
        _ => None,
    }
}

fn attribute_description(name: &str) -> Option<&'static str> {
    match name {
        "id" => Some("`@id` marks this field as the model's primary key."),
        "unique" => Some("`@unique` creates a unique constraint for this field."),
        "index" => Some("`@index` creates a standard database index for this field."),
        "fulltext" => Some("`@fulltext` enables native full-text search for this String field."),
        "ids" => Some("`@@ids([...])` defines the model's composite primary key."),
        "uniques" => Some("`@@uniques([...])` creates a composite unique constraint."),
        "indexes" => Some("`@@indexes([...])` creates a composite standard index."),
        "fulltexts" => Some("`@@fulltexts([...])` creates a composite full-text index for String fields."),
        "table_name" => Some("`@@table_name(\"...\")` maps the model to a database table name."),
        "default" => Some("`@default(...)` supplies a literal, enum, or generated default value."),
        "relation" => {
            Some("`@relation(...)` defines foreign keys, references, relation names, and referential actions.")
        }
        "uuid" => Some("`uuid()` generates a UUID value in the Dinoco library."),
        "snowflake" => Some("`snowflake()` generates a distributed Snowflake ID using `snowflake_node_id`."),
        "autoincrement" => Some("`autoincrement()` delegates integer ID generation to the database."),
        "now" => Some("`now()` uses the current date or UTC timestamp."),
        "Cascade" => Some("Propagates the referenced update or deletion to related rows."),
        "Restrict" => Some("Rejects the update or deletion while related rows exist."),
        "NoAction" => Some("Leaves enforcement timing and behavior to the database."),
        "SetNull" => Some("Sets optional local foreign-key fields to null."),
        "SetDefault" => Some("Sets local foreign-key fields to their declared defaults."),
        _ => None,
    }
}

fn config_description(name: &str) -> Option<&'static str> {
    match name {
        "database" => Some("Selects `postgresql`, `mysql`, or `sqlite`."),
        "connection" => Some("Selects PostgreSQL `direct` or `pgbouncer` connection behavior."),
        "database_url" => Some("Primary database URL. It must be loaded through `env(...)`."),
        "read_replicas" => Some("Optional environment-backed URLs used in round-robin reads."),
        "imports" => Some("Imports every declaration from schema files listed by the main `schema.dinoco`."),
        "snowflake_node_id" => Some("Environment-backed node ID required by `snowflake()`."),
        "with_logger" => Some("Enables SQL query logging when set to `true`. Defaults to `false`."),
        "min_connection" => Some("Minimum PostgreSQL Direct pool size. Defaults to `2`."),
        "max_connection" => Some("Maximum PostgreSQL Direct pool size. Defaults to `10`."),
        "env" => Some("Reads a configuration value from an environment variable."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn only_schema_dinoco_is_treated_as_the_database_entrypoint() {
        assert!(is_main_schema(&Url::parse("file:///project/dinoco/schema.dinoco").unwrap()));
        assert!(!is_main_schema(&Url::parse("file:///project/dinoco/models/account.dinoco").unwrap()));
        assert!(!is_main_schema(&Url::parse("file:///project/dinoco/enums.dinoco").unwrap()));
        assert!(!is_main_schema(&Url::parse("file:///project/dinoco/models/schema.dinoco").unwrap()));
    }

    #[test]
    fn formatting_imported_snowflake_models_does_not_require_project_config() {
        let uri = Url::parse("file:///project/dinoco/models/account.dinoco").unwrap();
        let source = "model Account{id Integer @id @default(snowflake())}";

        let formatted = format_document_source(&uri, source, &dinoco_formatter::FormatterConfig::default())
            .expect("imported document should format");

        assert!(formatted.contains("@default(snowflake())"));
    }

    #[test]
    fn formatting_main_snowflake_models_still_requires_project_config() {
        let uri = Url::parse("file:///project/dinoco/schema.dinoco").unwrap();
        let source = "model Account{id Integer @id @default(snowflake())}";

        let error = format_document_source(&uri, source, &dinoco_formatter::FormatterConfig::default())
            .expect_err("main schema must still validate project config");

        assert!(error.message.contains("snowflake_node_id"));
    }

    #[test]
    fn calculates_close_type_fixes() {
        let source = "model User { name Strng }";
        let index = DocumentIndex::new(source);
        let diagnostic = Diagnostic {
            range: Range::new(Position::new(0, 18), Position::new(0, 23)),
            message: "Unknown type `Strng`.".to_string(),
            ..Diagnostic::default()
        };
        assert_eq!(unknown_type_fix(&index, &diagnostic).expect("fix").new_text, "String");
    }

    #[test]
    fn edit_distance_handles_insertions() {
        assert_eq!(edit_distance("strng", "string"), 1);
    }

    #[test]
    fn project_compile_errors_are_published_at_the_imported_file() {
        let project = tempdir().expect("project");
        let root = project.path().join("schema.dinoco");
        let child = project.path().join("business.dinoco");
        fs::write(&root, "import { Business } from \"business.dinoco\"\n").expect("root");
        fs::write(
            &child,
            "model Business {\n    id String @id\n    account Account @relation(fields: [id], references: [id])\n}\n",
        )
        .expect("child");
        let root = canonical_path(&root).expect("canonical root");
        let mut cache = WorkspaceCache::default();
        let graph = cache.load_graph(&root, &HashMap::new());
        let error = dinoco_compiler::compile_file(&root).expect_err("project error");

        let (uri, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("diagnostic");

        assert_eq!(uri.to_file_path().expect("file uri"), canonical_path(&child).expect("canonical child"));
        assert_eq!(diagnostic.range.start.line, 2);
        assert_eq!(diagnostic.code, Some(NumberOrString::String("dinoco.project".to_string())));
    }

    #[test]
    fn import_diagnostics_select_the_missing_symbol_or_path() {
        let project = tempdir().expect("project");
        let root = project.path().join("schema.dinoco");
        let child = project.path().join("models.dinoco");
        fs::write(&child, "model Present { id String @id }\n").expect("child");
        fs::write(&root, "import { Missing } from \"./models.dinoco\"\n").expect("root");

        let root = canonical_path(&root).expect("canonical root");
        let mut cache = WorkspaceCache::default();
        let graph = cache.load_graph(&root, &HashMap::new());
        let error = dinoco_compiler::compile_file(&root).expect_err("missing symbol");
        let (_, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("symbol diagnostic");
        assert_eq!(diagnostic.range, Range::new(Position::new(0, 9), Position::new(0, 16)));
        let live = import_diagnostics(&root, &graph.files[&root], &graph);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].code, Some(NumberOrString::String("dinoco.importSymbolNotFound".to_string())));
        assert_eq!(live[0].range, diagnostic.range);

        fs::write(&root, "import { Present } from \"../missing.dinoco\"\n").expect("missing path");
        let mut cache = WorkspaceCache::default();
        let graph = cache.load_graph(&root, &HashMap::new());
        let error = dinoco_compiler::compile_file(&root).expect_err("missing file");
        let (_, diagnostic) = compile_error_diagnostic(&root, &graph, error).expect("path diagnostic");
        assert_eq!(diagnostic.range, Range::new(Position::new(0, 25), Position::new(0, 42)));
        let live = import_diagnostics(&root, &graph.files[&root], &graph);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].code, Some(NumberOrString::String("dinoco.importFileNotFound".to_string())));
        assert_eq!(live[0].range, diagnostic.range);
    }

    #[test]
    fn live_import_diagnostics_follow_transitive_content_changes() {
        let project = tempdir().expect("project");
        let root = project.path().join("schema.dinoco");
        let first = project.path().join("first.dinoco");
        let second = project.path().join("second.dinoco");
        fs::write(&root, "import { First } from \"./first.dinoco\"\n").expect("root");
        fs::write(&first, "import { Second } from \"./second.dinoco\"\nmodel First { id String @id second Second? }\n")
            .expect("first");
        fs::write(&second, "model Second { id String @id }\n").expect("second");
        let root = canonical_path(&root).expect("root path");
        let first = canonical_path(&first).expect("first path");

        let mut cache = WorkspaceCache::default();
        let graph = cache.load_graph(&root, &HashMap::new());
        assert!(graph.files.iter().all(|(path, file)| import_diagnostics(path, file, &graph).is_empty()));

        fs::write(&second, "model Renamed { id String @id }\n").expect("rename second");
        cache.invalidate(&second);
        let graph = cache.load_graph(&root, &HashMap::new());
        let diagnostics = import_diagnostics(&first, &graph.files[&first], &graph);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].range, Range::new(Position::new(0, 9), Position::new(0, 15)));
        assert!(diagnostics[0].message.contains("Imported symbol `Second`"));
    }

    #[test]
    fn semantic_tokens_distinguish_declarations_properties_types_and_enum_members() {
        let source = "enum Status {\n    active\n}\n\nmodel Account {\n    id     String @id\n    status Status\n}\n";
        let index = DocumentIndex::new(source);
        let spans = semantic_token_spans(&index);

        let kind_at = |line: u32, character: u32| {
            spans
                .iter()
                .find(|(range, _, _)| range.start.line == line && range.start.character == character)
                .map(|(_, token_type, modifiers)| (*token_type, *modifiers))
        };

        // `Status` the enum declaration.
        assert_eq!(kind_at(0, 5), Some((SEMANTIC_TOKEN_TYPE, SEMANTIC_MODIFIER_DECLARATION)));
        // `active` the enum member.
        assert_eq!(kind_at(1, 4), Some((SEMANTIC_TOKEN_ENUM_MEMBER, 0)));
        // `Account` the model declaration.
        assert_eq!(kind_at(4, 6), Some((SEMANTIC_TOKEN_TYPE, SEMANTIC_MODIFIER_DECLARATION)));
        // `id` the field name (property), not a type.
        assert_eq!(kind_at(5, 4), Some((SEMANTIC_TOKEN_PROPERTY, 0)));
        // `status` field referencing the `Status` enum: property name, then a type reference.
        assert_eq!(kind_at(6, 4), Some((SEMANTIC_TOKEN_PROPERTY, 0)));
        assert_eq!(kind_at(6, 11), Some((SEMANTIC_TOKEN_TYPE, 0)));

        let tokens = encode_semantic_tokens(spans);
        assert!(!tokens.is_empty());
        // Non-decreasing (line, start) order is required by the LSP encoding.
        let mut cursor_line = 0u32;
        for token in &tokens {
            cursor_line += token.delta_line;
            assert!(token.length > 0);
        }
        let _ = cursor_line;
    }
}
