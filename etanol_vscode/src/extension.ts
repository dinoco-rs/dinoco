import * as path from 'path';

import { ExtensionContext } from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind } from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
	const command = process.platform === 'win32' ? 'etanol_vscode.exe' : 'etanol_vscode';
	const serverPath = context.asAbsolutePath(path.join('target', 'debug', command));

	const serverOptions: ServerOptions = {
		run: { command: serverPath, transport: TransportKind.stdio },
		debug: { command: serverPath, transport: TransportKind.stdio },
	};

	const clientOptions: LanguageClientOptions = { documentSelector: [{ scheme: 'file', language: 'etanol' }] };

	client = new LanguageClient('etanolLanguageServer', 'Etanol Language Server', serverOptions, clientOptions);
	client.start();
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) return;

	return client.stop();
}
