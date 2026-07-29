use dinoco_vscode::server::DinocoLanguageServer;
use dinoco_vscode::tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(DinocoLanguageServer::new);

    Server::new(stdin, stdout, socket).serve(service).await;
}
