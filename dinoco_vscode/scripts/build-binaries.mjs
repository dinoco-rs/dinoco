import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const extensionRoot = path.resolve(import.meta.dirname, '..');
const binDir = path.join(extensionRoot, 'bin');
const crateName = 'dinoco_vscode';

const targetConfigs = [
	{ target: 'aarch64-apple-darwin', outputName: 'dinoco_vscode-darwin-arm64', platform: 'macos' },
	// { target: 'x86_64-apple-darwin', outputName: 'dinoco_vscode-darwin-x64', platform: 'macos' },

	{ target: 'x86_64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-x64', platform: 'linux' },
	{ target: 'aarch64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-arm64', platform: 'linux' },

	{ target: 'x86_64-pc-windows-gnu', outputName: 'dinoco_vscode-win32-x64.exe', platform: 'windows' },
	{ target: 'aarch64-pc-windows-gnullvm', outputName: 'dinoco_vscode-win32-arm64.exe', platform: 'windows' },
];

function runCommand(cmd, args) {
	const result = spawnSync(cmd, args, { cwd: extensionRoot, stdio: 'inherit' });
	if (result.error) {
		console.error(`\n❌ Falha ao tentar executar '${cmd}':`, result.error.message);
		process.exit(1);
	}
	if (result.status !== 0) {
		console.error(`\n❌ O comando '${cmd} ${args.join(' ')}' falhou com código ${result.status}.`);
		process.exit(result.status ?? 1);
	}
}

fs.mkdirSync(binDir, { recursive: true });

console.log('🔄 Verificando e instalando targets do Rust...');
const targets = targetConfigs.map(c => c.target);
runCommand('rustup', ['target', 'add', ...targets]);

const hasCross = spawnSync('cross', ['--version']).status === 0;
if (!hasCross) {
	console.warn('\n⚠️  ATENÇÃO: A ferramenta "cross" não foi encontrada.');
	console.warn('   Ela é necessária para compilar para Windows e Linux a partir do Mac sem erros de Linker.');
	console.warn('   Por favor, instale usando: cargo install cross');
	console.warn('   E certifique-se de que o Docker (Colima / Docker Desktop) está rodando.\n');
}

for (const config of targetConfigs) {
	console.log(`\n======================================================`);
	console.log(`🔨 Compilando para: ${config.target} (${config.platform})`);
	console.log(`======================================================`);

	const isMac = config.target.includes('apple');
	const buildTool = isMac || !hasCross ? 'cargo' : 'cross';

	runCommand(buildTool, ['build', '--release', '--target', config.target]);

	const isWindows = config.platform === 'windows';
	const sourceBinaryName = isWindows ? `${crateName}.exe` : crateName;
	const sourceBinaryPath = path.join(extensionRoot, 'target', config.target, 'release', sourceBinaryName);
	const destBinaryPath = path.join(binDir, config.outputName);

	if (!fs.existsSync(sourceBinaryPath)) {
		console.error(`\n❌ Binário não encontrado em: ${sourceBinaryPath}`);
		process.exit(1);
	}

	fs.copyFileSync(sourceBinaryPath, destBinaryPath);

	if (!isWindows) {
		fs.chmodSync(destBinaryPath, 0o755);
	}

	console.log(`✅ Sucesso! Copiado para: bin/${config.outputName}`);
}

console.log('\n🎉 Todos os binários foram gerados com sucesso na pasta "bin"!');
