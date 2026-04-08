import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const extensionRoot = path.resolve(import.meta.dirname, '..');
const binDir = path.join(extensionRoot, 'bin');
const crateName = 'dinoco_vscode';
const cargoBinDir = path.join(os.homedir(), '.cargo', 'bin');
const xwinCacheDir = path.join(extensionRoot, '.xwin-cache');
const env = {
	...process.env,
	PATH: fs.existsSync(cargoBinDir) ? `${cargoBinDir}${path.delimiter}${process.env.PATH ?? ''}` : process.env.PATH,
};
const xwinEnv = {
	...env,
	XWIN_CACHE_DIR: xwinCacheDir,
};

const targetConfigs = [
	{ target: 'aarch64-apple-darwin', outputName: 'dinoco_vscode-darwin-arm64', platform: 'macos' },
	{ target: 'x86_64-apple-darwin', outputName: 'dinoco_vscode-darwin-x64', platform: 'macos' },

	{ target: 'x86_64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-x64', platform: 'linux' },
	{ target: 'aarch64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-arm64', platform: 'linux' },

	{ target: 'x86_64-pc-windows-msvc', outputName: 'dinoco_vscode-win32-x64.exe', platform: 'windows' },
	{ target: 'aarch64-pc-windows-msvc', outputName: 'dinoco_vscode-win32-arm64.exe', platform: 'windows' },
];

function runCommand(cmd, args, customEnv = env) {
	const result = spawnSync(cmd, args, { cwd: extensionRoot, env: customEnv, stdio: 'inherit' });
	if (result.error) {
		console.error(`\n❌ Falha ao tentar executar '${cmd}':`, result.error.message);
		process.exit(1);
	}
	if (result.status !== 0) {
		if (cmd === 'cross') {
			console.error('\n💡 O "cross" precisa de Docker/Colima ativo e acessível pelo usuário atual.');
		}

		if (cmd === 'cargo' && args[0] === 'xwin') {
			console.error('\n💡 O "cargo-xwin" baixa o CRT/SDK do Windows na primeira execução.');
		}

		console.error(`\n❌ O comando '${cmd} ${args.join(' ')}' falhou com código ${result.status}.`);
		process.exit(result.status ?? 1);
	}
}

function hasCommand(cmd, args = ['--version']) {
	const result = spawnSync(cmd, args, { cwd: extensionRoot, env, stdio: 'ignore' });

	return result.status === 0;
}

function filterTargets() {
	const filters = process.argv.slice(2);

	if (filters.length === 0 || filters.includes('all')) {
		return targetConfigs;
	}

	const normalizedFilters = new Set(filters.map(filter => filter.toLowerCase()));
	const selectedTargets = targetConfigs.filter(config => normalizedFilters.has(config.platform) || normalizedFilters.has(config.target));

	if (selectedTargets.length === 0) {
		console.error(`\n❌ Filtro inválido: ${filters.join(', ')}`);
		console.error('   Use: all, macos, linux, windows ou um target Rust específico.');
		process.exit(1);
	}

	return selectedTargets;
}

fs.mkdirSync(binDir, { recursive: true });
fs.mkdirSync(xwinCacheDir, { recursive: true });

const selectedTargets = filterTargets();

console.log('🔄 Verificando e instalando targets do Rust...');
const targets = selectedTargets.map(c => c.target);
runCommand('rustup', ['target', 'add', ...targets]);

const hasCross = hasCommand('cross');
const hasXwin = hasCommand('cargo', ['xwin', '--version']);

if (!hasCross) {
	console.warn('\n⚠️  ATENÇÃO: A ferramenta "cross" não foi encontrada.');
}

if (!hasXwin) {
	console.warn('\n⚠️  ATENÇÃO: A ferramenta "cargo-xwin" não foi encontrada.');
}

for (const config of selectedTargets) {
	console.log(`\n======================================================`);
	console.log(`🔨 Compilando para: ${config.target} (${config.platform})`);
	console.log(`======================================================`);

	const isMac = config.platform === 'macos';
	const isWindows = config.platform === 'windows';
	const needsCross = config.platform === 'linux';

	if (needsCross && !hasCross) {
		console.error(`\n❌ Não foi possível compilar ${config.target} sem a ferramenta "cross".`);

		process.exit(1);
	}

	if (isWindows && !hasXwin) {
		console.error(`\n❌ Não foi possível compilar ${config.target} sem a ferramenta "cargo-xwin".`);
		console.error('   Instale com: cargo install cargo-xwin\n');

		process.exit(1);
	}

	if (isMac) {
		runCommand('cargo', ['build', '--release', '--target', config.target]);
	} else if (needsCross) {
		runCommand('cross', ['build', '--release', '--target', config.target]);
	} else if (isWindows) {
		runCommand('cargo', ['xwin', 'build', '--release', '--target', config.target], xwinEnv);
	}

	const sourceBinaryName = isWindows ? `${crateName}.exe` : crateName;
	const sourceBinaryPath = path.join(extensionRoot, '..', 'target', config.target, 'release', sourceBinaryName);
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
