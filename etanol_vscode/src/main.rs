// use tower_lsp::jsonrpc::Result as LspResult;
// use tower_lsp::lsp_types::*;
// use tower_lsp::{Client, LanguageServer, LspService, Server};

// use etanol_compiler::compile;

// #[derive(Debug)]
// struct LspServer {
//     client: Client,
// }

// #[tower_lsp::async_trait]
// impl LanguageServer for LspServer {
//     async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
//         Ok(InitializeResult {
//             capabilities: ServerCapabilities {
//                 text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),

//                 completion_provider: Some(CompletionOptions {
//                     resolve_provider: Some(false),
//                     trigger_characters: Some(vec!["@".to_string()]),
//                     ..Default::default()
//                 }),
//                 ..Default::default()
//             },
//             ..Default::default()
//         })
//     }

//     async fn initialized(&self, _: InitializedParams) {
//         self.client.log_message(MessageType::INFO, "Servidor Etanol LSP iniciado com sucesso!").await;
//     }

//     async fn shutdown(&self) -> LspResult<()> {
//         Ok(())
//     }

//     async fn did_open(&self, params: DidOpenTextDocumentParams) {
//         self.validate_document(params.text_document.uri, params.text_document.text).await;
//     }

//     async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
//         let uri = params.text_document.uri;
//         let text = params.content_changes.remove(0).text;
//         self.validate_document(uri, text).await;
//     }

//     async fn completion(&self, _params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
//         let completions = vec![
//             CompletionItem {
//                 label: "@id".into(),
//                 kind: Some(CompletionItemKind::PROPERTY),
//                 detail: Some("Primary Key".into()),
//                 insert_text: Some("@id".into()),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "@default".into(),
//                 kind: Some(CompletionItemKind::FUNCTION),
//                 detail: Some("Default value".into()),
//                 insert_text: Some("default(${1:value})".into()),
//                 insert_text_format: Some(InsertTextFormat::SNIPPET),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "@unique".into(),
//                 kind: Some(CompletionItemKind::PROPERTY),
//                 detail: Some("Unique constraint".into()),
//                 insert_text: Some("unique".into()),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "@relation".into(),
//                 kind: Some(CompletionItemKind::FUNCTION),
//                 detail: Some("Relation definition".into()),
//                 insert_text: Some("relation(name: \"${1}\", fields: [${2}], references: [${3}])".into()),
//                 insert_text_format: Some(InsertTextFormat::SNIPPET),
//                 ..Default::default()
//             },
//             // Types
//             CompletionItem {
//                 label: "String".into(),
//                 kind: Some(CompletionItemKind::TYPE_PARAMETER),
//                 detail: Some("String type".into()),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "Integer".into(),
//                 kind: Some(CompletionItemKind::TYPE_PARAMETER),
//                 detail: Some("Integer type".into()),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "Boolean".into(),
//                 kind: Some(CompletionItemKind::TYPE_PARAMETER),
//                 detail: Some("Boolean type".into()),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "Float".into(),
//                 kind: Some(CompletionItemKind::TYPE_PARAMETER),
//                 detail: Some("Float type".into()),
//                 ..Default::default()
//             },
//             // Blocks
//             CompletionItem {
//                 label: "model".into(),
//                 kind: Some(CompletionItemKind::KEYWORD),
//                 detail: Some("Define a model".into()),
//                 insert_text: Some("model ${1:Name} {\n\t$0\n}".into()),
//                 insert_text_format: Some(InsertTextFormat::SNIPPET),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "enum".into(),
//                 kind: Some(CompletionItemKind::KEYWORD),
//                 detail: Some("Define an enum".into()),
//                 insert_text: Some("enum ${1:Name} {\n\t$0\n}".into()),
//                 insert_text_format: Some(InsertTextFormat::SNIPPET),
//                 ..Default::default()
//             },
//             CompletionItem {
//                 label: "config".into(),
//                 kind: Some(CompletionItemKind::KEYWORD),
//                 detail: Some("Database configuration".into()),
//                 insert_text: Some("config {\n\tdatabase = \"${1:postgres}\"\n\tdatabase_url = \"${2}\"\n}".into()),
//                 insert_text_format: Some(InsertTextFormat::SNIPPET),
//                 ..Default::default()
//             },
//         ];

//         Ok(Some(CompletionResponse::Array(completions)))
//     }
// }

// impl LspServer {
//     async fn validate_document(&self, uri: Url, text: String) {
//         let mut diagnostics = vec![];

//         if let Err(errors) = compile(&text) {
//             for e in errors {
//                 let vscode_start_line = (e.start_line.saturating_sub(1)) as u32;
//                 let vscode_start_col = (e.start_column.saturating_sub(1)) as u32;

//                 let vscode_end_line = (e.end_line.saturating_sub(1)) as u32;
//                 let vscode_end_col = (e.end_column.saturating_sub(1)) as u32;

//                 let diagnostic = Diagnostic {
//                     range: Range {
//                         start: Position {
//                             line: vscode_start_line,
//                             character: vscode_start_col,
//                         },
//                         end: Position {
//                             line: vscode_end_line,
//                             character: vscode_end_col,
//                         },
//                     },
//                     severity: Some(DiagnosticSeverity::ERROR),
//                     code: Some(NumberOrString::String("E0001".to_string())),
//                     source: Some("etanol".to_string()),
//                     message: e.message.clone(),
//                     ..Default::default()
//                 };

//                 diagnostics.push(diagnostic);
//             }
//         }

//         self.client.publish_diagnostics(uri, diagnostics, None).await;
//     }
// }

// #[tokio::main]
// async fn main() {
//     let stdin = tokio::io::stdin();
//     let stdout = tokio::io::stdout();

//     let (service, socket) = LspService::new(|client| LspServer { client });
//     Server::new(stdin, stdout, socket).serve(service).await;
// }

use dashmap::DashMap;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use etanol_compiler::compile;

#[derive(Debug)]
struct LspServer {
    client: Client,
    // Armazena o conteúdo atual dos documentos abertos no editor
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
                    source: Some("etanol".to_string()),
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
                    // O LSP vai triggar automaticamente quando o usuário digitar '@' ou ' ' (espaço)
                    trigger_characters: Some(vec!["@".to_string(), " ".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "Etanol LSP Server is running!").await;
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

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Recupera o texto atual do documento
        let doc_text = self.documents.get(&uri).map(|v| v.clone()).unwrap_or_default();
        let lines: Vec<&str> = doc_text.lines().collect();

        // Pega exatamente a linha onde o cursor está
        let current_line = lines.get(position.line as usize).unwrap_or(&"");
        // Pega o texto da linha até o ponto exato do cursor
        let prefix_up_to_cursor = &current_line[..position.character as usize];
        let last_word = prefix_up_to_cursor.split_whitespace().last().unwrap_or("");

        let mut completions = vec![];

        // CONTEXTO 1: O usuário está digitando um decorator (começa com '@')
        if last_word.starts_with('@') || prefix_up_to_cursor.ends_with('@') {
            completions.extend(vec![
                create_completion("@id", CompletionItemKind::PROPERTY, "Primary Key", "id"),
                create_completion("@unique", CompletionItemKind::PROPERTY, "Unique constraint", "unique"),
                create_completion("@updatedAt", CompletionItemKind::PROPERTY, "Auto-update timestamp", "updatedAt"),
                create_snippet_completion("@default", CompletionItemKind::FUNCTION, "Default value", "default(${1:value})"),
                create_snippet_completion(
                    "@relation",
                    CompletionItemKind::FUNCTION,
                    "Relation definition",
                    "relation(name: \"${1}\", fields: [${2}], references: [${3}])",
                ),
            ]);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        // CONTEXTO 2: Ações Referenciais (se estiver digitando onUpdate: ou onDelete:)
        if prefix_up_to_cursor.contains("onUpdate") || prefix_up_to_cursor.contains("onDelete") {
            completions.extend(vec![
                create_completion("Cascade", CompletionItemKind::ENUM_MEMBER, "Cascade action", "Cascade"),
                create_completion("Restrict", CompletionItemKind::ENUM_MEMBER, "Restrict action", "Restrict"),
                create_completion("SetNull", CompletionItemKind::ENUM_MEMBER, "Set to Null", "SetNull"),
                create_completion("NoAction", CompletionItemKind::ENUM_MEMBER, "No action taken", "NoAction"),
            ]);
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        // CONTEXTO 3: Keywords Globais (blocos e tipos)
        let is_root_level = !prefix_up_to_cursor.starts_with(' ') && !prefix_up_to_cursor.starts_with('\t');

        if is_root_level {
            // Sugere blocos apenas se estiver na raiz do arquivo
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
            // Se estiver indentado, sugere Tipos primitivos
            completions.extend(vec![
                create_completion("String", CompletionItemKind::TYPE_PARAMETER, "String text type", "String"),
                create_completion("Integer", CompletionItemKind::TYPE_PARAMETER, "Integer number type", "Integer"),
                create_completion("Float", CompletionItemKind::TYPE_PARAMETER, "Floating point number", "Float"),
                create_completion("Boolean", CompletionItemKind::TYPE_PARAMETER, "True or False type", "Boolean"),
                create_completion("DateTime", CompletionItemKind::TYPE_PARAMETER, "Date and Time type", "DateTime"),
                create_completion("Json", CompletionItemKind::TYPE_PARAMETER, "JSON object type", "Json"),
            ]);
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }
}

// -- Funções Auxiliares para deixar o código limpo --

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
