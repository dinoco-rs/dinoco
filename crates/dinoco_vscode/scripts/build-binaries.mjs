import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const extensionRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const binDir = path.join(extensionRoot, 'bin');
const crateName = 'dinoco_vscode';
const cargoBinDir = path.join(os.homedir(), '.cargo', 'bin');
const xwinCacheDir = path.join(extensionRoot, '.xwin-cache');
const env = {
	...process.env,
	PATH: fs.existsSync(cargoBinDir) ? `${cargoBinDir}${path.delimiter}${process.env.PATH ?? ''}` : process.env.PATH,
};
const xwinEnv = { ...env, XWIN_CACHE_DIR: xwinCacheDir };

const targetConfigs = [
	{ target: 'aarch64-apple-darwin', outputName: 'dinoco_vscode-darwin-arm64', platform: 'macos' },
	{ target: 'x86_64-apple-darwin', outputName: 'dinoco_vscode-darwin-x64', platform: 'macos' },
	{ target: 'x86_64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-x64', platform: 'linux' },
	{ target: 'aarch64-unknown-linux-gnu', outputName: 'dinoco_vscode-linux-arm64', platform: 'linux' },
	{ target: 'x86_64-pc-windows-msvc', outputName: 'dinoco_vscode-win32-x64.exe', platform: 'windows' },
	{ target: 'aarch64-pc-windows-msvc', outputName: 'dinoco_vscode-win32-arm64.exe', platform: 'windows' },
];

function fail(message) {
	console.error(`\nError: ${message}`);
	process.exit(1);
}

function runCommand(command, args, options = {}) {
	const result = spawnSync(command, args, {
		cwd: options.cwd ?? extensionRoot,
		env: options.env ?? env,
		stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
		encoding: options.capture ? 'utf8' : undefined,
	});

	if (result.error) {
		fail(`could not run \`${command}\`: ${result.error.message}`);
	}
	if (result.status !== 0) {
		if (options.capture && result.stderr) {
			console.error(result.stderr.trim());
		}
		fail(`\`${command} ${args.join(' ')}\` exited with status ${result.status ?? 'unknown'}.`);
	}

	return result;
}

function hasCommand(command, args = ['--version']) {
	return spawnSync(command, args, { cwd: extensionRoot, env, stdio: 'ignore' }).status === 0;
}

function hostTarget() {
	const key = `${process.platform}-${process.arch}`;
	const targets = {
		'darwin-arm64': 'aarch64-apple-darwin',
		'darwin-x64': 'x86_64-apple-darwin',
		'linux-arm64': 'aarch64-unknown-linux-gnu',
		'linux-x64': 'x86_64-unknown-linux-gnu',
		'win32-arm64': 'aarch64-pc-windows-msvc',
		'win32-x64': 'x86_64-pc-windows-msvc',
	};

	return targets[key];
}

function selectTargets() {
	const filters = process.argv.slice(2).map(value => value.toLowerCase());
	if (filters.length === 0 || filters.includes('host')) {
		const target = hostTarget();
		if (!target) {
			fail(`the current platform is not supported: ${process.platform}/${process.arch}. Pass a Rust target explicitly.`);
		}
		return targetConfigs.filter(config => config.target === target);
	}
	if (filters.includes('all')) {
		return targetConfigs;
	}

	const selected = targetConfigs.filter(config => filters.includes(config.platform) || filters.includes(config.target));
	if (selected.length === 0) {
		fail(`unknown target filter: ${filters.join(', ')}. Use host, all, macos, linux, windows, or a Rust target.`);
	}

	return selected;
}

function buildStrategy(config, currentHostTarget) {
	if (config.target === currentHostTarget) {
		return 'cargo';
	}
	if (config.platform === 'macos') {
		if (process.platform !== 'darwin') {
			fail(`building ${config.target} requires a macOS host with Xcode command-line tools.`);
		}
		return 'cargo';
	}
	if (config.platform === 'linux') {
		return 'cross';
	}
	if (config.platform === 'windows') {
		return process.platform === 'win32' ? 'cargo' : 'xwin';
	}
	fail(`no build strategy is available for ${config.target}.`);
}

const metadataResult = runCommand('cargo', ['metadata', '--no-deps', '--format-version', '1'], { capture: true });
let metadata;
try {
	metadata = JSON.parse(metadataResult.stdout);
} catch (error) {
	fail(`Cargo returned invalid workspace metadata: ${error.message}`);
}

const workspaceRoot = metadata.workspace_root;
const targetDirectory = metadata.target_directory;
if (!workspaceRoot || !targetDirectory) {
	fail('Cargo metadata did not include workspace_root and target_directory.');
}

const selectedTargets = selectTargets();
const currentHostTarget = hostTarget();
const strategies = new Map(selectedTargets.map(config => [config.target, buildStrategy(config, currentHostTarget)]));
const needsCross = [...strategies.values()].includes('cross');
const needsXwin = [...strategies.values()].includes('xwin');

if (needsCross && !hasCommand('cross')) {
	fail('`cross` is required for the selected Linux targets. Install it and ensure Docker or Colima is running.');
}
if (needsXwin && !hasCommand('cargo', ['xwin', '--version'])) {
	fail('`cargo-xwin` is required for the selected Windows targets. Install it with `cargo install cargo-xwin`.');
}

fs.mkdirSync(binDir, { recursive: true });
if (needsXwin) {
	fs.mkdirSync(xwinCacheDir, { recursive: true });
}

console.log(`Workspace: ${workspaceRoot}`);
console.log(`Cargo target directory: ${targetDirectory}`);
console.log(`Selected targets: ${selectedTargets.map(config => config.target).join(', ')}`);
console.log('\nEnsuring Rust targets are installed...');
runCommand('rustup', ['target', 'add', ...selectedTargets.map(config => config.target)], { cwd: workspaceRoot });

for (const config of selectedTargets) {
	const strategy = strategies.get(config.target);
	const buildArgs = [
		'build',
		'--release',
		'--package',
		crateName,
		'--bin',
		crateName,
		'--target',
		config.target,
	];

	console.log(`\nBuilding ${config.target} with ${strategy}...`);
	if (strategy === 'cargo') {
		runCommand('cargo', buildArgs, { cwd: workspaceRoot });
	} else if (strategy === 'cross') {
		runCommand('cross', buildArgs, { cwd: workspaceRoot });
	} else {
		runCommand('cargo', ['xwin', ...buildArgs], { cwd: workspaceRoot, env: xwinEnv });
	}

	const sourceName = config.platform === 'windows' ? `${crateName}.exe` : crateName;
	const sourcePath = path.join(targetDirectory, config.target, 'release', sourceName);
	const destinationPath = path.join(binDir, config.outputName);
	if (!fs.existsSync(sourcePath)) {
		fail(`Cargo completed but the binary was not found at ${sourcePath}.`);
	}

	fs.copyFileSync(sourcePath, destinationPath);
	if (config.platform !== 'windows') {
		fs.chmodSync(destinationPath, 0o755);
	}
	console.log(`Created bin/${config.outputName}`);
}

console.log(`\nBuilt ${selectedTargets.length} Dinoco language server binary${selectedTargets.length === 1 ? '' : 'ies'}.`);
