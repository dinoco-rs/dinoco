import * as path from 'path';
import * as fs from 'fs';

import { ExtensionContext, window } from 'vscode';
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind } from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
	const command = getServerBinaryName();
	const serverPath = context.asAbsolutePath(path.join('bin', command));

	if (!fs.existsSync(serverPath)) {
		void window.showErrorMessage(`Dinoco language server binary not found for ${process.platform}/${process.arch}. Expected file: ${command}`);
		return;
	}

	const serverOptions: ServerOptions = {
		run: { command: serverPath, transport: TransportKind.stdio },
		debug: { command: serverPath, transport: TransportKind.stdio },
	};

	const clientOptions: LanguageClientOptions = { documentSelector: [{ scheme: 'file', language: 'dinoco' }] };

	client = new LanguageClient('DinocoLanguageServer', 'Dinoco Language Server', serverOptions, clientOptions);
	client.start();
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) return;

	return client.stop();
}

function getServerBinaryName(): string {
	if (process.platform === 'linux') {
		if (process.arch === 'x64') {
			return 'dinoco_vscode-linux-x64';
		}

		if (process.arch === 'arm64') {
			return 'dinoco_vscode-linux-arm64';
		}
	}

	if (process.platform === 'darwin') {
		if (process.arch === 'x64') {
			return 'dinoco_vscode-darwin-x64';
		}

		if (process.arch === 'arm64') {
			return 'dinoco_vscode-darwin-arm64';
		}
	}

	if (process.platform === 'win32') {
		if (process.arch === 'x64') {
			return 'dinoco_vscode-win32-x64.exe';
		}

		if (process.arch === 'arm64') {
			return 'dinoco_vscode-win32-arm64.exe';
		}
	}

	throw new Error(`Unsupported platform/architecture: ${process.platform}/${process.arch}`);
}
