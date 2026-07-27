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
const podmanRustBaseImage = process.env.DINOCO_PODMAN_RUST_IMAGE ?? 'docker.io/library/rust:1.95-slim-bullseye';
const podmanBuilderImage = process.env.DINOCO_PODMAN_BUILDER_IMAGE ?? 'localhost/dinoco-rust-linux-builder:1.95-bullseye-v1';
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
		return 'podman';
	}
	if (config.platform === 'windows') {
		return process.platform === 'win32' ? 'cargo' : 'xwin';
	}
	fail(`no build strategy is available for ${config.target}.`);
}

function podmanHostPlatform() {
	if (process.arch === 'x64') {
		return 'linux/amd64';
	}
	if (process.arch === 'arm64') {
		return 'linux/arm64';
	}
	fail(`no Podman builder platform is configured for ${process.arch}.`);
}

function runPodmanStep(args, options = {}) {
	const result = spawnSync('podman', args, {
		cwd: options.cwd ?? extensionRoot,
		env,
		stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
		encoding: options.capture ? 'utf8' : undefined,
	});

	if (result.error) {
		throw new Error(`could not run \`podman ${args.join(' ')}\`: ${result.error.message}`);
	}
	if (result.status !== 0) {
		const detail = options.capture && result.stderr ? `\n${result.stderr.trim()}` : '';
		throw new Error(`\`podman ${args.join(' ')}\` exited with status ${result.status ?? 'unknown'}.${detail}`);
	}

	return result;
}

function cleanupPodmanResource(args) {
	spawnSync('podman', args, {
		cwd: extensionRoot,
		env,
		stdio: 'ignore',
	});
}

function ensurePodmanBuilderImage() {
	const exists = spawnSync('podman', ['image', 'exists', podmanBuilderImage], {
		cwd: extensionRoot,
		env,
		stdio: 'ignore',
	});
	if (exists.status === 0) {
		return;
	}

	const contextDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'dinoco-podman-builder-'));
	const containerfile = `FROM ${podmanRustBaseImage}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \\
    && apt-get install --yes --no-install-recommends \\
        build-essential \\
        pkg-config \\
        cmake \\
        gcc-x86-64-linux-gnu \\
        g++-x86-64-linux-gnu \\
        libc6-dev-amd64-cross \\
        gcc-aarch64-linux-gnu \\
        g++-aarch64-linux-gnu \\
        libc6-dev-arm64-cross \\
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
`;

	let buildError;
	try {
		fs.writeFileSync(path.join(contextDirectory, 'Containerfile'), containerfile);
		console.log(`\nCreating Podman Linux builder image ${podmanBuilderImage}...`);
		runPodmanStep([
			'build',
			'--pull=missing',
			'--platform',
			podmanHostPlatform(),
			'--security-opt',
			'label=disable',
			'--tag',
			podmanBuilderImage,
			'--file',
			path.join(contextDirectory, 'Containerfile'),
			contextDirectory,
		]);
	} catch (error) {
		buildError = error instanceof Error ? error.message : String(error);
	} finally {
		fs.rmSync(contextDirectory, { recursive: true, force: true });
	}
	if (buildError) {
		fail(buildError);
	}
}

function workspaceRelativePath(workspaceRoot, source) {
	const relativePath = path.relative(workspaceRoot, source);
	if (relativePath === '' || relativePath.startsWith('..') || path.isAbsolute(relativePath)) {
		fail(`path is outside the Cargo workspace: ${source}`);
	}
	return relativePath;
}

function copyRustBuildSource(workspaceRoot, destination) {
	const ignoredPaths = new Set(['.git', 'target', 'vscode/bin', 'vscode/node_modules', 'vscode/out', 'vscode/.xwin-cache']);

	fs.cpSync(workspaceRoot, destination, {
		recursive: true,
		filter: source => {
			const relativePath = path.relative(workspaceRoot, source);
			if (relativePath === '') {
				return true;
			}
			const normalizedPath = relativePath.split(path.sep).join('/');
			return ![...ignoredPaths].some(ignoredPath => normalizedPath === ignoredPath || normalizedPath.startsWith(`${ignoredPath}/`));
		},
	});
}

function copyCargoFetchManifests(workspaceRoot, destination, workspacePackages) {
	fs.mkdirSync(destination, { recursive: true });
	fs.copyFileSync(path.join(workspaceRoot, 'Cargo.toml'), path.join(destination, 'Cargo.toml'));
	fs.copyFileSync(path.join(workspaceRoot, 'Cargo.lock'), path.join(destination, 'Cargo.lock'));

	for (const workspacePackage of workspacePackages) {
		const manifestRelativePath = workspaceRelativePath(workspaceRoot, workspacePackage.manifest_path);
		const manifestDestination = path.join(destination, manifestRelativePath);
		fs.mkdirSync(path.dirname(manifestDestination), { recursive: true });
		fs.copyFileSync(workspacePackage.manifest_path, manifestDestination);

		for (const target of workspacePackage.targets) {
			const targetRelativePath = workspaceRelativePath(workspaceRoot, target.src_path);
			const targetDestination = path.join(destination, targetRelativePath);
			fs.mkdirSync(path.dirname(targetDestination), { recursive: true });
			fs.writeFileSync(targetDestination, '');
		}
	}
}

function buildLinuxWithPodman(config, workspaceRoot, targetDirectory, workspacePackages) {
	ensurePodmanBuilderImage();
	const uniqueId = `${config.target.replaceAll('_', '-')}-${process.pid}-${Date.now()}`;
	const fetchContainer = `dinoco-fetch-${uniqueId}`;
	const buildContainer = `dinoco-build-${uniqueId}`;
	const cargoVolume = `dinoco-cargo-${uniqueId}`;
	const hostTargetDirectory = path.join(targetDirectory, 'podman', config.target);
	const sourcePath = path.join(hostTargetDirectory, 'release', crateName);
	const stagingRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dinoco-vscode-linux-'));
	const sourceDirectory = path.join(stagingRoot, 'source');
	const manifestDirectory = path.join(stagingRoot, 'manifests');

	fs.rmSync(hostTargetDirectory, { recursive: true, force: true });
	fs.mkdirSync(path.dirname(sourcePath), { recursive: true });
	copyRustBuildSource(workspaceRoot, sourceDirectory);
	copyCargoFetchManifests(workspaceRoot, manifestDirectory, workspacePackages);

	let buildError;
	try {
		runPodmanStep(['volume', 'create', cargoVolume], { capture: true });
		runPodmanStep([
			'create',
			'--name',
			fetchContainer,
			'--pull=missing',
			'--platform',
			podmanHostPlatform(),
			'--security-opt',
			'label=disable',
			'--volume',
			`${cargoVolume}:/cargo-home`,
			'--workdir',
			'/',
			'--env',
			'CARGO_HOME=/cargo-home',
			podmanBuilderImage,
			'sleep',
			'infinity',
		]);
		runPodmanStep(['start', fetchContainer], { capture: true });
		runPodmanStep(['exec', fetchContainer, 'mkdir', '-p', '/workspace']);
		runPodmanStep(['cp', `${manifestDirectory}/.`, `${fetchContainer}:/workspace`]);
		runPodmanStep(['exec', '--workdir', '/workspace', fetchContainer, 'cargo', 'fetch', '--target', config.target]);
		runPodmanStep(['rm', '--force', fetchContainer], { capture: true });

		runPodmanStep([
			'create',
			'--name',
			buildContainer,
			'--network',
			'none',
			'--platform',
			podmanHostPlatform(),
			'--security-opt',
			'label=disable',
			'--volume',
			`${cargoVolume}:/cargo-home`,
			'--workdir',
			'/',
			'--env',
			'CARGO_HOME=/cargo-home',
			'--env',
			'CARGO_TARGET_DIR=/target',
			'--env',
			'CARGO_NET_OFFLINE=true',
			'--env',
			'CARGO_TERM_COLOR=always',
			'--env',
			'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc',
			'--env',
			'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc',
			podmanBuilderImage,
			'sleep',
			'infinity',
		]);
		runPodmanStep(['start', buildContainer], { capture: true });
		runPodmanStep(['exec', buildContainer, 'mkdir', '-p', '/workspace']);
		runPodmanStep(['cp', `${sourceDirectory}/.`, `${buildContainer}:/workspace`]);
		runPodmanStep([
			'exec',
			'--workdir',
			'/workspace',
			buildContainer,
			'cargo',
			'build',
			'--locked',
			'--offline',
			'--release',
			'--package',
			crateName,
			'--bin',
			crateName,
			'--target',
			config.target,
		]);
		runPodmanStep(['cp', `${buildContainer}:/target/${config.target}/release/${crateName}`, sourcePath]);
	} catch (error) {
		buildError = error instanceof Error ? error.message : String(error);
	} finally {
		cleanupPodmanResource(['rm', '--force', fetchContainer]);
		cleanupPodmanResource(['rm', '--force', buildContainer]);
		cleanupPodmanResource(['volume', 'rm', '--force', cargoVolume]);
		fs.rmSync(stagingRoot, { recursive: true, force: true });
	}
	if (buildError) {
		fail(buildError);
	}

	return sourcePath;
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
const workspacePackages = metadata.packages.filter(workspacePackage => metadata.workspace_members.includes(workspacePackage.id));

const selectedTargets = selectTargets();
const currentHostTarget = hostTarget();
const strategies = new Map(selectedTargets.map(config => [config.target, buildStrategy(config, currentHostTarget)]));
const needsPodman = [...strategies.values()].includes('podman');
const needsXwin = [...strategies.values()].includes('xwin');

if (needsPodman && !hasCommand('podman')) {
	fail('Podman is required for building Linux binaries. Install Podman and ensure it is available on PATH.');
}
if (needsPodman) {
	runCommand('podman', ['info'], { capture: true });
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
if (needsPodman) {
	console.log(`Linux builder: Podman (${podmanBuilderImage}, based on ${podmanRustBaseImage})`);
}

const localTargets = selectedTargets.filter(config => strategies.get(config.target) !== 'podman').map(config => config.target);
if (localTargets.length > 0) {
	if (!hasCommand('rustup')) {
		fail('`rustup` is required to install targets compiled directly on this host.');
	}

	const installedTargets = new Set(runCommand('rustup', ['target', 'list', '--installed'], { cwd: workspaceRoot, capture: true }).stdout.split(/\r?\n/).filter(Boolean));
	const missingTargets = localTargets.filter(target => !installedTargets.has(target));
	if (missingTargets.length > 0) {
		console.log(`\nInstalling missing Rust targets: ${missingTargets.join(', ')}`);
		runCommand('rustup', ['target', 'add', ...missingTargets], { cwd: workspaceRoot });
	} else {
		console.log('\nAll locally compiled Rust targets are already installed.');
	}
}

for (const config of selectedTargets) {
	const strategy = strategies.get(config.target);
	const buildArgs = ['build', '--locked', '--release', '--package', crateName, '--bin', crateName, '--target', config.target];
	const sourceName = config.platform === 'windows' ? `${crateName}.exe` : crateName;
	let sourcePath;

	console.log(`\nBuilding ${config.target} with ${strategy}...`);
	if (strategy === 'cargo') {
		runCommand('cargo', buildArgs, { cwd: workspaceRoot });
		sourcePath = path.join(targetDirectory, config.target, 'release', sourceName);
	} else if (strategy === 'podman') {
		sourcePath = buildLinuxWithPodman(config, workspaceRoot, targetDirectory, workspacePackages);
	} else {
		runCommand('cargo', ['xwin', ...buildArgs], { cwd: workspaceRoot, env: xwinEnv });
		sourcePath = path.join(targetDirectory, config.target, 'release', sourceName);
	}

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
