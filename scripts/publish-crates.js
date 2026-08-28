#!/usr/bin/env node

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const cratesRoot = path.join(repositoryRoot, "crates");
const inputArguments = process.argv.slice(2);
const planOnly = inputArguments.includes("--plan");
const cargoArguments = inputArguments.filter((argument) => argument !== "--plan");
const dryRun = cargoArguments.includes("--dry-run");
const publishDelayMs = Number.parseInt(process.env.DINOCO_PUBLISH_DELAY_MS ?? "15000", 10);

function fail(message) {
    console.error(`\n[publish-crates] ${message}`);
    process.exit(1);
}

function run(command, arguments_, options = {}) {
    const result = spawnSync(command, arguments_, {
        cwd: repositoryRoot,
        encoding: "utf8",
        stdio: options.capture ? "pipe" : "inherit",
    });

    if (result.error) {
        fail(`Não foi possível executar \`${command}\`: ${result.error.message}`);
    }
    if (result.status !== 0) {
        if (options.capture && result.stderr) {
            process.stderr.write(result.stderr);
        }
        process.exit(result.status ?? 1);
    }

    return result.stdout ?? "";
}

function loadCrates() {
    if (!fs.existsSync(cratesRoot)) {
        fail(`Diretório não encontrado: ${cratesRoot}`);
    }

    const metadata = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--no-deps"], { capture: true }));
    const packages = metadata.packages.filter((package_) => {
        const packageDirectory = path.dirname(package_.manifest_path);
        return path.dirname(packageDirectory) === cratesRoot;
    });

    const manifestDirectories = fs
        .readdirSync(cratesRoot, { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && fs.existsSync(path.join(cratesRoot, entry.name, "Cargo.toml")))
        .map((entry) => path.join(cratesRoot, entry.name));

    if (packages.length !== manifestDirectories.length) {
        const discovered = new Set(packages.map((package_) => path.dirname(package_.manifest_path)));
        const missing = manifestDirectories.filter((directory) => !discovered.has(directory));
        fail(`As seguintes crates não pertencem ao workspace: ${missing.join(", ")}`);
    }

    return packages;
}

function publicationOrder(packages) {
    const packageByName = new Map(packages.map((package_) => [package_.name, package_]));
    const dependencies = new Map(
        packages.map((package_) => [
            package_.name,
            new Set(
                package_.dependencies
                    .filter((dependency) => dependency.path && packageByName.has(dependency.name))
                    .map((dependency) => dependency.name),
            ),
        ]),
    );
    const ordered = [];

    while (ordered.length < packages.length) {
        const published = new Set(ordered.map((package_) => package_.name));
        const ready = packages
            .filter((package_) => !published.has(package_.name))
            .filter((package_) => [...dependencies.get(package_.name)].every((dependency) => published.has(dependency)))
            .sort((left, right) => left.name.localeCompare(right.name));

        if (ready.length === 0) {
            const remaining = packages.filter((package_) => !published.has(package_.name)).map((package_) => package_.name);
            fail(`Dependência circular entre crates publicáveis: ${remaining.join(", ")}`);
        }

        ordered.push(...ready);
    }

    return ordered;
}

function wait(milliseconds) {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function main() {
    if (!Number.isFinite(publishDelayMs) || publishDelayMs < 0) {
        fail("DINOCO_PUBLISH_DELAY_MS deve ser um número inteiro maior ou igual a zero");
    }

    const crates = publicationOrder(loadCrates());
    if (crates.length === 0) {
        fail("Nenhuma crate foi encontrada em crates/*");
    }

    console.log("Ordem de publicação:");
    crates.forEach((package_, index) => console.log(`  ${index + 1}. ${package_.name} v${package_.version}`));

    if (planOnly) {
        return;
    }

    for (const [index, package_] of crates.entries()) {
        console.log(`\n[${index + 1}/${crates.length}] Publicando ${package_.name} v${package_.version}...`);
        run("cargo", ["publish", "--manifest-path", package_.manifest_path, ...cargoArguments]);

        const hasNextCrate = index + 1 < crates.length;
        if (!dryRun && hasNextCrate && publishDelayMs > 0) {
            console.log(`Aguardando ${publishDelayMs}ms para propagação do registry...`);
            await wait(publishDelayMs);
        }
    }

    console.log(`\n${crates.length} crates publicadas com sucesso.`);
}

main().catch((error) => fail(error.stack ?? error.message));
