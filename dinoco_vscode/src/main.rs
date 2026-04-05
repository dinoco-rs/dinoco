use dashmap::DashMap;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use dinoco_compiler::{compile, compile_only_ast};
use dinoco_formatter::{format_from_ast, FormatterConfig};

#[derive(Debug)]
struct LspServer {
    client: Client,

    documents: DashMap<Url, String>,
}

impl LspServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
        }
    }

    async fn validate_document(&self, uri: Url, text: String) {
        let mut diagnostics = vec![];

        if let Err(errors) = compile(&text) {
            for e in errors {
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: e.start_line.saturating_sub(1) as u32,
                            character: e.start_column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: e.end_line.saturating_sub(1) as u32,
                            character: e.end_column.saturating_sub(1) as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("E0001".to_string())),
                    source: Some("dinoco".to_string()),
                    message: e.message.clone(),
                    ..Default::default()
                };

                diagnostics.push(diagnostic);
            }
        }

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LspServer {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),

                    trigger_characters: Some(vec!["@".to_string(), " ".to_string()]),
                    ..Default::default()
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },

            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "Dinoco LSP Server is running!").await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        self.documents.insert(uri.clone(), text.clone());
        self.validate_document(uri, text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.content_changes.remove(0).text;

        self.documents.insert(uri.clone(), text.clone());
        self.validate_document(uri, text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> LspResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        let text = doc.value().clone();
        let formatted = match compile_only_ast(&text) {
            Ok(schema) => {
                let config = FormatterConfig::default();

                format_from_ast(&schema, &config)
            }
            Err(_) => {
                return Ok(None);
            }
        };

        let line_count = text.lines().count() as u32;
        let full_range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: line_count, character: 0 },
        };

        Ok(Some(vec![TextEdit {
            range: full_range,
            new_text: formatted,
        }]))
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let doc_text = self.documents.get(&uri).map(|v| v.clone()).unwrap_or_default();
        let lines: Vec<&str> = doc_text.lines().collect();

        let current_line = lines.get(position.line as usize).unwrap_or(&"");

        let prefix_up_to_cursor = &current_line[..position.character as usize];
        let last_word = prefix_up_to_cursor.split_whitespace().last().unwrap_or("");

        let mut completions = vec![];

        if prefix_up_to_cursor.contains("@default(") && !prefix_up_to_cursor.ends_with(')') {
            completions.extend(vec![
                create_completion("autoincrement()", CompletionItemKind::FUNCTION, "Sequencial ID", "autoincrement()"),
                create_completion("uuid()", CompletionItemKind::FUNCTION, "UUID v4", "uuid()"),
                create_completion("snowflake()", CompletionItemKind::FUNCTION, "Snowflake ID", "snowflake()"),
                create_completion("now()", CompletionItemKind::FUNCTION, "Current timestamp", "now()"),
            ]);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        if last_word.starts_with('@') || prefix_up_to_cursor.ends_with('@') {
            completions.extend(vec![
                create_completion("@id", CompletionItemKind::PROPERTY, "Primary Key", "id"),
                create_completion("@unique", CompletionItemKind::PROPERTY, "Unique constraint", "unique"),
                create_snippet_completion("@default", CompletionItemKind::FUNCTION, "Default value", "default($0)"),
                create_snippet_completion(
                    "@relation",
                    CompletionItemKind::FUNCTION,
                    "Relation definition",
                    "relation(name: \"${1}\", fields: [${2}], references: [${3}])",
                ),
            ]);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        if prefix_up_to_cursor.contains("onUpdate") || prefix_up_to_cursor.contains("onDelete") {
            completions.extend(vec![
                create_completion("Cascade", CompletionItemKind::ENUM_MEMBER, "Cascade action", "Cascade"),
                create_completion("Restrict", CompletionItemKind::ENUM_MEMBER, "Restrict action", "Restrict"),
                create_completion("SetNull", CompletionItemKind::ENUM_MEMBER, "Set to Null", "SetNull"),
                create_completion("NoAction", CompletionItemKind::ENUM_MEMBER, "No action taken", "NoAction"),
            ]);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        let is_root_level = !prefix_up_to_cursor.starts_with(' ') && !prefix_up_to_cursor.starts_with('\t');

        if is_root_level {
            completions.extend(vec![
                create_snippet_completion("model", CompletionItemKind::KEYWORD, "Define a new model", "model ${1:ModelName} {\n\t$0\n}"),
                create_snippet_completion("enum", CompletionItemKind::KEYWORD, "Define an enum", "enum ${1:EnumName} {\n\t$0\n}"),
                create_snippet_completion(
                    "config",
                    CompletionItemKind::KEYWORD,
                    "Database configuration",
                    "config {\n\tprovider = \"${1:postgresql}\"\n\turl = \"${2:env(\"DATABASE_URL\")}\"\n}",
                ),
            ]);
        } else {
            completions.extend(vec![
                create_completion("String", CompletionItemKind::TYPE_PARAMETER, "String text type", "String"),
                create_completion("Integer", CompletionItemKind::TYPE_PARAMETER, "Integer number type", "Integer"),
                create_completion("Float", CompletionItemKind::TYPE_PARAMETER, "Floating point number", "Float"),
                create_completion("Boolean", CompletionItemKind::TYPE_PARAMETER, "True or False type", "Boolean"),
                create_completion("DateTime", CompletionItemKind::TYPE_PARAMETER, "Date and Time in UTC", "DateTime"),
                create_completion("Date", CompletionItemKind::TYPE_PARAMETER, "Date without time", "Date"),
                create_completion("Json", CompletionItemKind::TYPE_PARAMETER, "JSON object type", "Json"),
            ]);
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }
}

fn create_completion(label: &str, kind: CompletionItemKind, detail: &str, insert_text: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        insert_text: Some(insert_text.to_string()),
        ..Default::default()
    }
}

fn create_snippet_completion(label: &str, kind: CompletionItemKind, detail: &str, snippet: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        insert_text: Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| LspServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
