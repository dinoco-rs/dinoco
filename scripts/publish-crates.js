#!/usr/bin/env node

const { spawn, spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const cratesRoot = path.join(repositoryRoot, "crates");
const inputArguments = process.argv.slice(2);
const planOnly = inputArguments.includes("--plan");
const cargoArguments = inputArguments.filter((argument) => argument !== "--plan");
const dryRun = cargoArguments.includes("--dry-run");
const indexTimeoutMs = Number.parseInt(process.env.DINOCO_INDEX_TIMEOUT_MS ?? "1200000", 10);
const indexPollMs = 15000;
const userAgent = "dinoco-publish-crates (https://github.com/dinoco-rs/dinoco)";

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

// A API do crates.io responde assim que a versão é publicada: ela diz se uma
// execução anterior já publicou a crate.
async function isPublished(package_) {
    const url = `https://crates.io/api/v1/crates/${package_.name}/${package_.version}`;
    const response = await fetch(url, { headers: { "User-Agent": userAgent } });
    if (response.status === 404) {
        return false;
    }
    if (!response.ok) {
        fail(`O crates.io respondeu ${response.status} para ${package_.name} v${package_.version}`);
    }
    return true;
}

// O Cargo resolve as dependências pelo índice esparso, servido por uma CDN que
// guarda cada arquivo por até 10 minutos: uma versão recém-publicada pode
// demorar esse tempo para aparecer, mais do que o `cargo publish` espera.
async function isInIndex(package_) {
    const name = package_.name.toLowerCase();
    const prefix = name.length <= 2 ? name.length : name.length === 3 ? `3/${name[0]}` : `${name.slice(0, 2)}/${name.slice(2, 4)}`;
    const response = await fetch(`https://index.crates.io/${prefix}/${name}`, { headers: { "User-Agent": userAgent } });
    if (!response.ok) {
        return false;
    }
    const lines = (await response.text()).split("\n").filter(Boolean);
    return lines.some((line) => JSON.parse(line).vers === package_.version);
}

// Roda o `cargo publish` mostrando a saída e guardando o stderr. Uma versão que
// já está no crates.io conta como publicada: uma execução anterior pode ter
// falhado depois do upload, ou a API ainda não refletir um upload recente.
function publish(package_) {
    return new Promise((resolve) => {
        const child = spawn("cargo", ["publish", "--manifest-path", package_.manifest_path, ...cargoArguments], {
            cwd: repositoryRoot,
            stdio: ["inherit", "inherit", "pipe"],
        });
        let stderr = "";
        child.stderr.on("data", (chunk) => {
            process.stderr.write(chunk);
            stderr += chunk;
        });
        child.on("error", (error) => fail(`Não foi possível executar \`cargo\`: ${error.message}`));
        child.on("close", (status) => {
            if (status === 0) {
                resolve("published");
            } else if (/is already uploaded|already exists on crates\.io index/.test(stderr)) {
                resolve("already");
            } else {
                process.exit(status ?? 1);
            }
        });
    });
}

async function waitForIndex(packages) {
    const deadline = Date.now() + indexTimeoutMs;
    for (const package_ of packages) {
        while (!(await isInIndex(package_))) {
            if (Date.now() > deadline) {
                fail(`${package_.name} v${package_.version} não apareceu no índice do crates.io a tempo; rode o script de novo`);
            }
            console.log(`Aguardando ${package_.name} v${package_.version} aparecer no índice do crates.io...`);
            await wait(indexPollMs);
        }
    }
}

async function main() {
    if (!Number.isFinite(indexTimeoutMs) || indexTimeoutMs < 0) {
        fail("DINOCO_INDEX_TIMEOUT_MS deve ser um número inteiro maior ou igual a zero");
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

    const packageByName = new Map(crates.map((package_) => [package_.name, package_]));
    for (const [index, package_] of crates.entries()) {
        if (!dryRun && (await isPublished(package_))) {
            console.log(`\n[${index + 1}/${crates.length}] ${package_.name} v${package_.version} já está publicada.`);
            continue;
        }

        if (!dryRun) {
            const dependencies = package_.dependencies
                .filter((dependency) => dependency.path && packageByName.has(dependency.name))
                .map((dependency) => packageByName.get(dependency.name));
            await waitForIndex(dependencies);
        }

        console.log(`\n[${index + 1}/${crates.length}] Publicando ${package_.name} v${package_.version}...`);
        if ((await publish(package_)) === "already") {
            console.log(`${package_.name} v${package_.version} já estava publicada; seguindo.`);
        }
    }

    console.log(`\n${crates.length} crates publicadas com sucesso.`);
}

main().catch((error) => fail(error.stack ?? error.message));
