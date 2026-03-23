"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const path = require("path");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const command = process.platform === 'win32' ? 'etanol_vscode.exe' : 'etanol_vscode';
    const serverPath = context.asAbsolutePath(path.join('target', 'debug', command));
    const serverOptions = {
        run: { command: serverPath, transport: node_1.TransportKind.stdio },
        debug: { command: serverPath, transport: node_1.TransportKind.stdio },
    };
    const clientOptions = { documentSelector: [{ scheme: 'file', language: 'etanol' }] };
    client = new node_1.LanguageClient('etanolLanguageServer', 'Etanol Language Server', serverOptions, clientOptions);
    client.start();
}
function deactivate() {
    if (!client)
        return;
    return client.stop();
}
//# sourceMappingURL=extension.js.map