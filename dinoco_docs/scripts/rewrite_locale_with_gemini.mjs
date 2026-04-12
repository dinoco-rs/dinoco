import fs from 'fs/promises';
import path from 'path';

const API_KEY = process.env.GEMINI_API_KEY;
const DEFAULT_MODEL = 'gemini-2.5-flash';
const DEFAULT_VERSION = 'v0.0.2';
const DEFAULT_RETRY_ATTEMPTS = 4;
const PROJECT_ROOT = 'dinoco_docs';
const MIN_START_INTERVAL_MS = 250;

const LOCALES = {
	'de-de': 'German (Germany)',
	'en-us': 'English (United States)',
	'fr-fr': 'French (France)',
	'it-it': 'Italian (Italy)',
	'ja-jp': 'Japanese (Japan)',
	'ko-kr': 'Korean (South Korea)',
	'pt-br': 'Portuguese (Brazil)',
	'ru-ru': 'Russian (Russia)',
	'zh-cn': 'Simplified Chinese (China)',
};

const FREE_TIER_LIMITS = {
	'gemini-2.0-flash': { concurrency: 2, rpm: 15 },
	'gemini-2.0-flash-lite': { concurrency: 4, rpm: 30 },
	'gemini-2.5-flash': { concurrency: 10, rpm: 40 },
	'gemini-2.5-flash-lite': { concurrency: 2, rpm: 15 },
	'gemini-2.5-flash-lite-preview': { concurrency: 2, rpm: 15 },
	'gemini-2.5-flash-preview': { concurrency: 2, rpm: 10 },
	'gemini-2.5-pro': { concurrency: 2, rpm: 30 },
};

if (!API_KEY) {
	throw new Error('GEMINI_API_KEY is required.');
}

function parseArgs(argv) {
	const options = {
		concurrency: undefined,
		include: 'all',
		model: DEFAULT_MODEL,
		retryAttempts: DEFAULT_RETRY_ATTEMPTS,
		rpm: undefined,
		sourceLocale: undefined,
		targetLocales: undefined,
		useAllVersionLocales: false,
		version: DEFAULT_VERSION,
	};

	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		const next = argv[index + 1];

		if (arg === '--version' && next) {
			options.version = next;
			index += 1;
			continue;
		}

		if (arg === '--model' && next) {
			options.model = next;
			index += 1;
			continue;
		}

		if (arg === '--concurrency' && next) {
			options.concurrency = Number.parseInt(next, 10);
			index += 1;
			continue;
		}

		if (arg === '--rpm' && next) {
			options.rpm = Number.parseInt(next, 10);
			index += 1;
			continue;
		}

		if (arg === '--retry-attempts' && next) {
			options.retryAttempts = Number.parseInt(next, 10);
			index += 1;
			continue;
		}

		if (arg === '--source-locale' && next) {
			options.sourceLocale = next.trim();
			index += 1;
			continue;
		}

		if (arg === '--target-locales' && next) {
			options.targetLocales = next
				.split(',')
				.map(locale => locale.trim())
				.filter(Boolean);
			index += 1;
			continue;
		}

		if (arg === '--include' && next) {
			options.include = next.trim();
			index += 1;
			continue;
		}

		if (arg === '--all-version-locales') {
			options.useAllVersionLocales = true;
		}
	}

	if (options.sourceLocale !== undefined && !(options.sourceLocale in LOCALES)) {
		throw new Error(`Unsupported source locale: ${options.sourceLocale}`);
	}

	if (options.targetLocales !== undefined) {
		for (const locale of options.targetLocales) {
			if (!(locale in LOCALES)) {
				throw new Error(`Unsupported target locale: ${locale}`);
			}
		}
	}

	if (!['all', 'json', 'content'].includes(options.include)) {
		throw new Error("--include must be one of: 'all', 'json', 'content'.");
	}

	if (options.concurrency !== undefined && (!Number.isInteger(options.concurrency) || options.concurrency < 1)) {
		throw new Error('--concurrency must be an integer greater than 0.');
	}

	if (options.rpm !== undefined && (!Number.isInteger(options.rpm) || options.rpm < 1)) {
		throw new Error('--rpm must be an integer greater than 0.');
	}

	if (!Number.isInteger(options.retryAttempts) || options.retryAttempts < 0) {
		throw new Error('--retry-attempts must be an integer greater than or equal to 0.');
	}

	return options;
}

async function discoverVersionLocales(version) {
	const discovered = new Set();
	const jsonDir = path.join(PROJECT_ROOT, 'src/jsons/versions', version);
	const contentDir = path.join(PROJECT_ROOT, 'src/content', version);

	try {
		const entries = await fs.readdir(jsonDir, { withFileTypes: true });

		for (const entry of entries) {
			if (!entry.isFile() || !entry.name.endsWith('.json')) {
				continue;
			}

			const locale = entry.name.replace(/\.json$/, '');

			if (locale in LOCALES) {
				discovered.add(locale);
			}
		}
	} catch {}

	try {
		const entries = await fs.readdir(contentDir, { withFileTypes: true });

		for (const entry of entries) {
			if (entry.isDirectory() && entry.name in LOCALES) {
				discovered.add(entry.name);
			}
		}
	} catch {}

	return [...discovered].sort();
}

function buildLocalePairs({ sourceLocale, targetLocales, useAllVersionLocales, versionLocales }) {
	if (useAllVersionLocales || (sourceLocale === undefined && targetLocales === undefined)) {
		return versionLocales.map(locale => ({
			sourceLocale: locale,
			targetLocale: locale,
		}));
	}

	if (sourceLocale !== undefined && (targetLocales === undefined || targetLocales.length === 0)) {
		return [{ sourceLocale, targetLocale: sourceLocale }];
	}

	if (sourceLocale === undefined) {
		throw new Error('When --target-locales is provided, --source-locale must also be provided.');
	}

	return targetLocales.map(targetLocale => ({
		sourceLocale,
		targetLocale,
	}));
}

function normalizeModelName(model) {
	return model.trim().toLowerCase();
}

function resolveRateLimitConfig(options) {
	const normalizedModel = normalizeModelName(options.model);
	const defaultConfig = FREE_TIER_LIMITS[normalizedModel] ?? {
		concurrency: 5,
		rpm: 20,
	};

	return {
		concurrency: options.concurrency ?? defaultConfig.concurrency,
		rpm: options.rpm ?? defaultConfig.rpm,
	};
}

function delay(ms) {
	return new Promise(resolve => {
		setTimeout(resolve, ms);
	});
}

function parseRetryAfterMs(response) {
	const retryAfter = response.headers.get('retry-after');

	if (!retryAfter) {
		return undefined;
	}

	const seconds = Number.parseFloat(retryAfter);

	if (!Number.isNaN(seconds)) {
		return Math.max(0, Math.ceil(seconds * 1000));
	}

	const timestamp = Date.parse(retryAfter);

	if (Number.isNaN(timestamp)) {
		return undefined;
	}

	return Math.max(0, timestamp - Date.now());
}

function buildRateLimiter({ rpm }) {
	const intervalMs = Math.max(MIN_START_INTERVAL_MS, Math.ceil(60_000 / rpm));
	let nextStartAt = 0;

	return {
		async waitTurn() {
			const now = Date.now();
			const scheduledAt = Math.max(now, nextStartAt);

			nextStartAt = scheduledAt + intervalMs;

			const waitMs = scheduledAt - now;

			if (waitMs > 0) {
				await delay(waitMs);
			}
		},
		intervalMs,
	};
}

async function collectJsonFiles(version, localePairs) {
	const versionsDir = path.join(PROJECT_ROOT, 'src/jsons/versions', version);

	return localePairs
		.map(({ sourceLocale, targetLocale }) => ({
			filePath: path.join(versionsDir, `${targetLocale}.json`),
			sourcePath: path.join(versionsDir, `${sourceLocale}.json`),
			sourceLocale,
			targetLocale,
		}))
		.sort((left, right) => left.filePath.localeCompare(right.filePath));
}

async function collectContentFiles(version, localePairs) {
	const files = [];

	for (const { sourceLocale, targetLocale } of localePairs) {
		const baseDir = path.join(PROJECT_ROOT, 'src/content', version, sourceLocale);
		const entries = await fs.readdir(baseDir, { recursive: true, withFileTypes: true });

		for (const entry of entries) {
			if (!entry.isFile() || !entry.name.endsWith('.md')) {
				continue;
			}

			const sourcePath = path.join(entry.parentPath, entry.name);
			const relativePath = path.relative(baseDir, sourcePath);

			files.push({
				filePath: path.join(PROJECT_ROOT, 'src/content', version, targetLocale, relativePath),
				sourcePath,
				sourceLocale,
				targetLocale,
			});
		}
	}

	return files.sort((left, right) => left.filePath.localeCompare(right.filePath));
}

function buildPrompt({ filePath, sourcePath, sourceLocale, targetLocale, content }) {
	const sourceLanguage = LOCALES[sourceLocale];
	const targetLanguage = LOCALES[targetLocale];
	const isJson = filePath.endsWith('.json');
	const isInPlaceRewrite = sourceLocale === targetLocale;

	return [
		`Rewrite the following ${isJson ? 'JSON' : 'MDX'} file so that all human-readable natural language is in ${targetLanguage}.`,
		`The current file should be treated as the source of truth and is currently labeled as locale '${sourceLocale}'.`,
		isInPlaceRewrite
			? `This is an in-place locale normalization pass for ${targetLanguage}. The file may already contain ${targetLanguage}, mixed languages, or accidental translations from other locales.`
			: `Translate the file from ${sourceLanguage} to ${targetLanguage} using the current source file as the only source of truth.`,
		'Return only the full rewritten file contents, with no markdown fences and no commentary.',
		'Requirements:',
		'- Preserve the exact file structure and syntax.',
		'- Preserve keys, property names, paths, identifiers, imports, exports, code fences, code syntax, URLs, and code symbols.',
		'- Rewrite only human-readable natural language text to the target language.',
		'- Keep locale codes, shortName values, contentPath values, route segments, type names, function names, Rust identifiers, Dinoco API identifiers, and command names unchanged.',
		'- In JSON files, rewrite only string values meant for UI or documentation text.',
		'- In MDX files, also rewrite human-readable comments inside code blocks.',
		'- In MDX files, also rewrite human-readable string literals used as example content, labels, titles, messages, text, names, descriptions, comments, and sample values.',
		'- Do not translate code identifiers, type names, field names, enum names, API names, locale codes, paths, URLs, commands, or syntax tokens.',
		'- If a string literal is acting as an identifier, slug, database key, file path, locale, route segment, or command, keep it unchanged.',
		'- If the source contains mixed languages, normalize all human-readable text to the target language consistently.',
		'- Preserve line breaks as naturally as possible.',
		'Target file path:',
		filePath,
		'Source file path:',
		sourcePath,
		'Source file contents:',
		content,
	].join('\n');
}

function stripMarkdownFence(content) {
	const fencedBlock = content.match(/^```[^\n]*\n([\s\S]*?)\n```$/);

	if (!fencedBlock) {
		return content;
	}

	return fencedBlock[1].trim();
}

async function requestTranslation({ filePath, model, prompt, retryAttempts }) {
	for (let attempt = 0; attempt <= retryAttempts; attempt += 1) {
		const response = await fetch(`https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${API_KEY}`, {
			body: JSON.stringify({
				contents: [
					{
						parts: [{ text: prompt }],
						role: 'user',
					},
				],
				generationConfig: {
					temperature: 0.2,
				},
			}),
			headers: {
				'Content-Type': 'application/json',
			},
			method: 'POST',
		});

		if (response.ok) {
			const data = await response.json();
			const translated = data.candidates?.[0]?.content?.parts
				?.map(part => part.text ?? '')
				.join('')
				?.trim();

			if (!translated) {
				throw new Error(`Gemini returned empty content for ${filePath}`);
			}

			return translated;
		}

		if (response.status === 429 && attempt < retryAttempts) {
			const retryAfterMs = parseRetryAfterMs(response);
			const backoffMs = 5_000 * (attempt + 1);
			const waitMs = Math.max(retryAfterMs ?? 0, backoffMs);

			console.warn(`Rate limit hit for ${filePath}. Retrying in ${waitMs}ms (${attempt + 1}/${retryAttempts}).`);
			await delay(waitMs);
			continue;
		}

		throw new Error(`Gemini request failed for ${filePath}: ${response.status} ${await response.text()}`);
	}

	throw new Error(`Gemini request failed for ${filePath}: retry attempts exhausted.`);
}

async function rewriteFile({ filePath, sourceLocale, sourcePath, targetLocale, model, retryAttempts }) {
	const original = await fs.readFile(sourcePath, 'utf8');
	const prompt = buildPrompt({
		filePath,
		sourceLocale,
		sourcePath,
		targetLocale,
		content: original,
	});
	const rewritten = await requestTranslation({
		filePath,
		model,
		prompt,
		retryAttempts,
	});
	const sanitized = stripMarkdownFence(rewritten);

	await fs.mkdir(path.dirname(filePath), { recursive: true });
	await fs.writeFile(filePath, `${sanitized}\n`);
}

async function runWithConcurrency(items, concurrency, worker) {
	const queue = [...items];
	const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
		while (queue.length > 0) {
			const item = queue.shift();

			if (item === undefined) {
				return;
			}

			await worker(item);
		}
	});

	await Promise.all(workers);
}

const options = parseArgs(process.argv.slice(2));
const rateLimitConfig = resolveRateLimitConfig(options);
const rateLimiter = buildRateLimiter(rateLimitConfig);
const versionLocales = await discoverVersionLocales(options.version);

if (versionLocales.length === 0) {
	throw new Error(`No locales were found for version ${options.version}.`);
}

const localePairs = buildLocalePairs({
	sourceLocale: options.sourceLocale,
	targetLocales: options.targetLocales,
	useAllVersionLocales: options.useAllVersionLocales,
	versionLocales,
});

const jsonFiles = options.include === 'all' || options.include === 'json' ? await collectJsonFiles(options.version, localePairs) : [];
const contentFiles = options.include === 'all' || options.include === 'content' ? await collectContentFiles(options.version, localePairs) : [];
const files = [...jsonFiles, ...contentFiles];

console.log(`Using Gemini rate limits for ${options.model}: ${rateLimitConfig.rpm} RPM, concurrency ${rateLimitConfig.concurrency}, start interval ${rateLimiter.intervalMs}ms.`);
console.log(`Rewriting locale content for version ${options.version} (${options.include}). Pairs: ${localePairs.map(pair => `${pair.sourceLocale}->${pair.targetLocale}`).join(', ')}.`);

await runWithConcurrency(files, rateLimitConfig.concurrency, async file => {
	console.log(`Rewriting ${file.filePath} from ${file.sourcePath} (${file.sourceLocale} -> ${file.targetLocale})`);

	await rateLimiter.waitTurn();

	await rewriteFile({
		filePath: file.filePath,
		model: options.model,
		retryAttempts: options.retryAttempts,
		sourceLocale: file.sourceLocale,
		sourcePath: file.sourcePath,
		targetLocale: file.targetLocale,
	});
});

console.log(`Done. Version: ${options.version}. Locale pairs: ${localePairs.map(pair => `${pair.sourceLocale}->${pair.targetLocale}`).join(', ')}`);
