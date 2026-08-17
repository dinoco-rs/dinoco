use std::collections::HashMap;
use std::sync::RwLock;

use tower_lsp::jsonrpc::{Error as LspError, Result as LspResult};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::completion;
use crate::diagnostics::{self, CODE_MISSING_CONFIG, CODE_MISSING_DATABASE_URL, CODE_UNKNOWN_TYPE};
use crate::document::{BlockKind, DocumentIndex, ResolvedSymbol, scalar_types};

#[derive(Debug, Clone)]
struct DocumentState {
    text: String,
    index: DocumentIndex,
    version: i32,
}

impl DocumentState {
    fn new(text: String, version: i32) -> Self {
        let index = DocumentIndex::new(&text);
        Self { text, index, version }
    }
}

pub struct DinocoLanguageServer {
    client: Client,
    documents: RwLock<HashMap<Url, DocumentState>>,
}

impl DinocoLanguageServer {
    pub fn new(client: Client) -> Self {
        Self { client, documents: RwLock::new(HashMap::new()) }
    }

    fn document(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.read().ok()?.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String, version: i32) {
        let state = DocumentState::new(text, version);
        let diagnostics = diagnostics::analyze(&state.text, &state.index);
        if let Ok(mut documents) = self.documents.write() {
            documents.insert(uri.clone(), state);
        }
        self.client.publish_diagnostics(uri, diagnostics, Some(version)).await;
    }

    fn hover_for(&self, state: &DocumentState, position: Position) -> Option<Hover> {
        let token = state.index.token_symbol_at(position)?;
        let contents = if let Some(symbol) = state.index.resolve_symbol(position) {
            hover_resolved_symbol(&state.index, &symbol)?
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

    fn quick_fixes(&self, uri: &Url, state: &DocumentState, params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
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
                CODE_MISSING_DATABASE_URL => state.index.config().map(|config| TextEdit {
                    range: Range::new(config.body_range.start, config.body_range.start),
                    new_text: "\n    database_url = env(\"DATABASE_URL\")".to_string(),
                }),
                CODE_UNKNOWN_TYPE => unknown_type_fix(&state.index, item),
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
                    save: Some(TextDocumentSyncSaveOptions::Supported(true)),
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
            self.update_document(params.text_document.uri, text, version).await;
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
        Ok(Some(completion::complete(&state.text, &state.index, position)))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> LspResult<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        Ok(completion::signature(&state.text, position))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self.document(&uri).and_then(|state| self.hover_for(&state, position)))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let Some(range) = state.index.resolve_symbol(position).and_then(|symbol| state.index.definition(&symbol))
        else {
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
        let Some(symbol) = state.index.resolve_symbol(position) else {
            return Ok(None);
        };
        let definition = state.index.definition(&symbol);
        let locations = state
            .index
            .occurrences(&symbol)
            .into_iter()
            .filter(|range| params.context.include_declaration || Some(*range) != definition)
            .map(|range| Location::new(uri.clone(), range))
            .collect();
        Ok(Some(locations))
    }

    async fn document_highlight(&self, params: DocumentHighlightParams) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        let Some(symbol) = state.index.resolve_symbol(position) else {
            return Ok(None);
        };
        Ok(Some(
            state
                .index
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
        if state.index.resolve_symbol(params.position).is_none() {
            return Ok(None);
        }
        Ok(state
            .index
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
        let Some(symbol) = state.index.resolve_symbol(position) else {
            return Ok(None);
        };
        let edits = state
            .index
            .occurrences(&symbol)
            .into_iter()
            .map(|range| TextEdit { range, new_text: params.new_name.clone() })
            .collect();
        Ok(Some(WorkspaceEdit { changes: Some(HashMap::from([(uri, edits)])), ..WorkspaceEdit::default() }))
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> LspResult<Option<DocumentSymbolResponse>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let symbols = state.index.blocks.iter().map(document_symbol).collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let Some(state) = self.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(
            state
                .index
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
                let document =
                    SelectionRange { range: Range::new(Position::new(0, 0), state.index.end_position()), parent: None };
                let Some(block) = state.index.block_at(position) else {
                    return document;
                };
                let block_range = SelectionRange { range: block.range, parent: Some(Box::new(document)) };
                if let Some((_, field)) = state.index.field_at(position) {
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
        let config = dinoco_formatter::FormatterConfig {
            indent_width: params.options.tab_size.max(1) as usize,
            final_newline: true,
        };
        let formatted = dinoco_formatter::format_from_raw_with_config(&state.text, &config)
            .map_err(|error| LspError::invalid_params(format!("Cannot format an invalid Dinoco schema: {error}")))?;
        if formatted == state.text {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: Range::new(Position::new(0, 0), state.index.end_position()),
            new_text: formatted,
        }]))
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let Some(state) = self.document(&uri) else {
            return Ok(None);
        };
        Ok(Some(self.quick_fixes(&uri, &state, &params)))
    }
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
    use super::*;

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
}
