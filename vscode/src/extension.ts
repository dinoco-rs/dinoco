import * as fs from 'node:fs';
import * as path from 'node:path';

import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    RevealOutputChannelOn,
    ServerOptions,
    State,
    Trace,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let outputChannel: vscode.LogOutputChannel | undefined;
let status: vscode.StatusBarItem | undefined;
let extensionContext: vscode.ExtensionContext | undefined;
let fileWatcher: vscode.FileSystemWatcher | undefined;
let stateSubscription: vscode.Disposable | undefined;
let normalizingFormatterSettings = false;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    extensionContext = context;
    outputChannel = vscode.window.createOutputChannel('Dinoco Language Server', { log: true });
    status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 90);
    status.command = 'dinoco.openSchema';

    context.subscriptions.push(
        outputChannel,
        status,
        new vscode.Disposable(() => fileWatcher?.dispose()),
        new vscode.Disposable(() => stateSubscription?.dispose()),
    );
    registerCommands(context);
    registerEditorEvents(context);
    updateStatus();
    await normalizeFormatterIndentationOnStartup();

    await startLanguageServer(context);
}

export async function deactivate(): Promise<void> {
    const runningClient = client;
    client = undefined;
    fileWatcher?.dispose();
    fileWatcher = undefined;
    stateSubscription?.dispose();
    stateSubscription = undefined;
    if (runningClient) {
        await runningClient.stop();
    }
}

async function startLanguageServer(context: vscode.ExtensionContext): Promise<void> {
    const serverPath = resolveServerPath(context);
    if (!serverPath) {
        return;
    }

    fileWatcher?.dispose();
    fileWatcher = vscode.workspace.createFileSystemWatcher('**/*.dinoco');

    const serverOptions: ServerOptions = {
        run: { command: serverPath, transport: TransportKind.stdio },
        debug: { command: serverPath, transport: TransportKind.stdio },
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'dinoco' },
            { scheme: 'untitled', language: 'dinoco' },
        ],
        synchronize: {
            configurationSection: 'dinoco',
            fileEvents: fileWatcher,
        },
        outputChannel,
        revealOutputChannelOn: RevealOutputChannelOn.Error,
    };

    client = new LanguageClient('dinocoLanguageServer', 'Dinoco Language Server', serverOptions, clientOptions);
    stateSubscription?.dispose();
    stateSubscription = client.onDidChangeState(event => {
        if (event.newState === State.Running) {
            updateStatus();
        } else if (event.newState === State.Stopped) {
            updateStatus('Language server stopped');
        }
    });

    try {
        await client.start();
        await applyServerTrace();
    } catch (error) {
        client = undefined;
        updateStatus('Language server failed');
        const detail = error instanceof Error ? error.message : String(error);
        void vscode.window.showErrorMessage(`Dinoco language server failed to start: ${detail}`, 'Show Output').then(choice => {
            if (choice === 'Show Output') {
                outputChannel?.show(true);
            }
        });
    }
}

async function restartLanguageServer(): Promise<void> {
    const context = extensionContext;
    if (!context) {
        return;
    }
    const runningClient = client;
    client = undefined;
    if (runningClient) {
        await runningClient.stop();
    }
    await startLanguageServer(context);
}

function registerCommands(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('dinoco.openSchema', openSchema),
        vscode.commands.registerCommand('dinoco.formatSchema', formatActiveSchema),
        vscode.commands.registerCommand('dinoco.restartLanguageServer', restartLanguageServer),
        vscode.commands.registerCommand('dinoco.showOutput', () => outputChannel?.show(true)),
        vscode.commands.registerCommand('dinoco.init', () => runCli(['init'], 'Initialize project')),
        vscode.commands.registerCommand('dinoco.models.generate', () => runCli(['models', 'generate'], 'Generate models')),
        vscode.commands.registerCommand('dinoco.migrate.generate', () => runCli(['migrate', 'generate'], 'Generate migration')),
        vscode.commands.registerCommand('dinoco.migrate.run', () => runCli(['migrate', 'run'], 'Run migrations')),
    );
}

async function formatActiveSchema(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'dinoco') {
        void vscode.window.showWarningMessage('Open a Dinoco schema before formatting.');
        return;
    }
    if (!client?.isRunning()) {
        void vscode.window.showErrorMessage('The Dinoco language server is not running.', 'Show Output').then(choice => {
            if (choice === 'Show Output') {
                outputChannel?.show(true);
            }
        });
        return;
    }

    const tabSize = typeof editor.options.tabSize === 'number' ? editor.options.tabSize : 4;
    const insertSpaces = typeof editor.options.insertSpaces === 'boolean' ? editor.options.insertSpaces : true;
    const edits = await vscode.commands.executeCommand<vscode.TextEdit[]>('vscode.executeFormatDocumentProvider',
        editor.document.uri,
        { tabSize, insertSpaces },
    );
    if (!edits) {
        void vscode.window.showErrorMessage('The Dinoco formatter did not return a result.', 'Show Output').then(choice => {
            if (choice === 'Show Output') {
                outputChannel?.show(true);
            }
        });
        return;
    }
    if (edits.length === 0) {
        return;
    }

    const workspaceEdit = new vscode.WorkspaceEdit();
    workspaceEdit.set(editor.document.uri, edits);
    if (!(await vscode.workspace.applyEdit(workspaceEdit))) {
        void vscode.window.showErrorMessage('VS Code could not apply the Dinoco formatting edits.');
    }
}

function registerEditorEvents(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => updateStatus()),
        vscode.workspace.onDidChangeTextDocument(event => {
            if (event.document === vscode.window.activeTextEditor?.document) {
                updateStatus();
            }
        }),
        vscode.languages.onDidChangeDiagnostics(event => {
            const activeUri = vscode.window.activeTextEditor?.document.uri;
            if (activeUri && event.uris.some(uri => uri.toString() === activeUri.toString())) {
                updateStatus();
            }
        }),
        vscode.workspace.onDidSaveTextDocument(async document => {
            if (document.languageId !== 'dinoco') {
                return;
            }
            const enabled = vscode.workspace.getConfiguration('dinoco', document.uri).get<boolean>('models.generateOnSave', false);
            if (enabled) {
                await runCli(['models', 'generate'], 'Generate models');
            }
        }),
        vscode.workspace.onDidChangeConfiguration(async event => {
            if (event.affectsConfiguration('dinoco.server.path')) {
                await restartLanguageServer();
            }
            if (event.affectsConfiguration('dinoco.trace.server')) {
                await applyServerTrace();
            }
            const useTabsChanged = event.affectsConfiguration('dinoco.formatter.useTabs');
            const useSpacesChanged = event.affectsConfiguration('dinoco.formatter.useSpaces');
            if (useTabsChanged && !useSpacesChanged) {
                await syncFormatterIndentation('useTabs');
            } else if (useSpacesChanged && !useTabsChanged) {
                await syncFormatterIndentation('useSpaces');
            } else if (useTabsChanged && useSpacesChanged) {
                // Both settings changed in the same edit (e.g. settings.json hand-edited
                // directly). There's no reliable way to tell which the user "meant", so
                // fall back to a clear, documented precedence: tabs win.
                await syncFormatterIndentation('useTabs');
            }
        }),
    );
}

/**
 * "dinoco.formatter.useTabs" and "dinoco.formatter.useSpaces" are mutually
 * exclusive. Whichever setting just changed is authoritative; its counterpart
 * is written to match (at the same configuration target) so the Settings UI
 * never shows both enabled or both disabled at once.
 */
async function syncFormatterIndentation(source: 'useTabs' | 'useSpaces'): Promise<void> {
    if (normalizingFormatterSettings) {
        return;
    }

    const config = vscode.workspace.getConfiguration('dinoco.formatter');
    const counterpart = source === 'useTabs' ? 'useSpaces' : 'useTabs';
    const sourceValue = config.get<boolean>(source, source === 'useSpaces');
    const counterpartValue = config.get<boolean>(counterpart, counterpart === 'useSpaces');
    if (counterpartValue === !sourceValue) {
        return;
    }

    const target = configurationTargetOf(config.inspect<boolean>(source)) ?? vscode.ConfigurationTarget.Global;
    normalizingFormatterSettings = true;
    try {
        await config.update(counterpart, !sourceValue, target);
    } finally {
        normalizingFormatterSettings = false;
    }
}

/** Fixes a pre-existing inconsistent state (both settings true, or both false) on activation. */
async function normalizeFormatterIndentationOnStartup(): Promise<void> {
    const config = vscode.workspace.getConfiguration('dinoco.formatter');
    const useTabs = config.get<boolean>('useTabs', false);
    const useSpaces = config.get<boolean>('useSpaces', true);
    if (useTabs === useSpaces) {
        await syncFormatterIndentation('useTabs');
    }
}

function configurationTargetOf(
    inspected: ReturnType<vscode.WorkspaceConfiguration['inspect']> | undefined,
): vscode.ConfigurationTarget | undefined {
    if (!inspected) {
        return undefined;
    }
    if (inspected.workspaceFolderValue !== undefined) {
        return vscode.ConfigurationTarget.WorkspaceFolder;
    }
    if (inspected.workspaceValue !== undefined) {
        return vscode.ConfigurationTarget.Workspace;
    }
    if (inspected.globalValue !== undefined) {
        return vscode.ConfigurationTarget.Global;
    }
    return undefined;
}

async function applyServerTrace(): Promise<void> {
    if (!client || !client.isRunning()) {
        return;
    }
    const configured = vscode.workspace.getConfiguration('dinoco').get<string>('trace.server', 'off');
    const trace = configured === 'verbose' ? Trace.Verbose : configured === 'messages' ? Trace.Messages : Trace.Off;
    await client.setTrace(trace);
}

async function openSchema(): Promise<void> {
    const files = await vscode.workspace.findFiles('**/dinoco/schema.dinoco', '**/{target,node_modules}/**', 20);
    if (files.length === 0) {
        const choice = await vscode.window.showInformationMessage(
            'No dinoco/schema.dinoco file was found in this workspace.',
            'Initialize Dinoco',
        );
        if (choice === 'Initialize Dinoco') {
            await runCli(['init'], 'Initialize project');
        }
        return;
    }

    let selected = files[0];
    if (files.length > 1) {
        const picked = await vscode.window.showQuickPick(
            files.map(uri => ({
                label: vscode.workspace.asRelativePath(uri, false),
                uri,
            })),
            { placeHolder: 'Select a Dinoco schema' },
        );
        if (!picked) {
            return;
        }
        selected = picked.uri;
    }

    const document = await vscode.workspace.openTextDocument(selected);
    await vscode.window.showTextDocument(document);
}

async function runCli(args: string[], label: string): Promise<void> {
    const folder = activeWorkspaceFolder();
    if (!folder) {
        void vscode.window.showErrorMessage('Open a workspace folder before running Dinoco commands.');
        return;
    }

    const executable = vscode.workspace.getConfiguration('dinoco', folder.uri).get<string>('cli.path', 'dinoco').trim();
    if (!executable) {
        void vscode.window.showErrorMessage('Configure dinoco.cli.path before running Dinoco commands.');
        return;
    }

    const definition: vscode.TaskDefinition = { type: 'dinoco', command: args.join(' ') };
    const execution = new vscode.ShellExecution(executable, args, { cwd: folder.uri.fsPath });
    const task = new vscode.Task(definition, folder, `Dinoco: ${label}`, 'dinoco', execution, []);
    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Dedicated,
        clear: true,
        focus: true,
    };
    await vscode.tasks.executeTask(task);
}

function resolveServerPath(context: vscode.ExtensionContext): string | undefined {
    const configured = vscode.workspace.getConfiguration('dinoco').get<string>('server.path', '').trim();
    if (configured) {
        const folder = activeWorkspaceFolder();
        const resolved = path.isAbsolute(configured)
            ? configured
            : path.resolve(folder?.uri.fsPath ?? context.extensionPath, configured);
        if (fs.existsSync(resolved)) {
            return resolved;
        }
        void vscode.window.showErrorMessage(`Configured Dinoco language server was not found: ${resolved}`);
        return undefined;
    }

    let binary: string;
    try {
        binary = bundledBinaryName();
    } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        void vscode.window.showErrorMessage(detail);
        return undefined;
    }
    const resolved = context.asAbsolutePath(path.join('bin', binary));
    if (!fs.existsSync(resolved)) {
        void vscode.window.showErrorMessage(
            `Dinoco language server is not bundled for ${process.platform}/${process.arch}. Configure dinoco.server.path to use a custom binary.`,
        );
        return undefined;
    }
    return resolved;
}

function bundledBinaryName(): string {
    const names: Record<string, string> = {
        'darwin-arm64': 'dinoco_vscode-darwin-arm64',
        'darwin-x64': 'dinoco_vscode-darwin-x64',
        'linux-arm64': 'dinoco_vscode-linux-arm64',
        'linux-x64': 'dinoco_vscode-linux-x64',
        'win32-arm64': 'dinoco_vscode-win32-arm64.exe',
        'win32-x64': 'dinoco_vscode-win32-x64.exe',
    };
    const key = `${process.platform}-${process.arch}`;
    const name = names[key];
    if (!name) {
        throw new Error(`Dinoco does not currently support VS Code on ${process.platform}/${process.arch}.`);
    }
    return name;
}

function activeWorkspaceFolder(): vscode.WorkspaceFolder | undefined {
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    return (activeUri && vscode.workspace.getWorkspaceFolder(activeUri)) ?? vscode.workspace.workspaceFolders?.[0];
}

function updateStatus(message?: string): void {
    if (!status) {
        return;
    }
    const document = vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== 'dinoco') {
        status.hide();
        return;
    }

    const diagnostics = vscode.languages.getDiagnostics(document.uri);
    const errors = diagnostics.filter(item => item.severity === vscode.DiagnosticSeverity.Error).length;
    const warnings = diagnostics.filter(item => item.severity === vscode.DiagnosticSeverity.Warning).length;
    const database = document.getText().match(/\bdatabase\s*=\s*["']?([A-Za-z]+)/)?.[1];
    const adapter = database ? titleCase(database === 'postgres' ? 'postgresql' : database) : 'Schema';

    status.text = message
        ? `$(database) Dinoco: ${message}`
        : errors > 0
          ? `$(error) Dinoco: ${errors} error${errors === 1 ? '' : 's'}`
          : warnings > 0
            ? `$(warning) Dinoco: ${warnings} warning${warnings === 1 ? '' : 's'}`
            : `$(database) Dinoco: ${adapter}`;
    status.tooltip = new vscode.MarkdownString(
        message
            ? message
            : errors > 0 || warnings > 0
              ? `${errors} error(s), ${warnings} warning(s). Click to open the schema.`
              : `${adapter} schema is valid. Click to open the workspace schema.`,
    );
    status.show();
}

function titleCase(value: string): string {
    return value.charAt(0).toUpperCase() + value.slice(1).toLowerCase();
}
