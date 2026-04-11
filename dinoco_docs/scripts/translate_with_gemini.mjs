import fs from 'fs/promises';
import path from 'path';

const API_KEY = process.env.GEMINI_API_KEY;
const DEFAULT_MODEL = 'gemini-2.5-flash';
const DEFAULT_VERSION = 'v0.0.1';
const DEFAULT_RETRY_ATTEMPTS = 4;
const PROJECT_ROOT = 'dinoco_docs';
const SOURCE_LOCALE = 'pt-br';
const MIN_START_INTERVAL_MS = 250;

const TARGETS = {
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
	'gemini-2.5-flash': { concurrency: 15, rpm: 60 },
	'gemini-2.5-flash-lite': { concurrency: 2, rpm: 15 },
	'gemini-2.5-flash-lite-preview': { concurrency: 2, rpm: 15 },
	'gemini-2.5-flash-preview': { concurrency: 2, rpm: 10 },
	'gemini-2.5-pro': { concurrency: 2, rpm: 10 },
};

if (!API_KEY) {
	throw new Error('GEMINI_API_KEY is required.');
}

function parseArgs(argv) {
	const options = {
		concurrency: undefined,
		locales: undefined,
		model: DEFAULT_MODEL,
		retryAttempts: DEFAULT_RETRY_ATTEMPTS,
		rpm: undefined,
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

		if (arg === '--locales' && next) {
			options.locales = next
				.split(',')
				.map(locale => locale.trim())
				.filter(Boolean);
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
		}
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

	if (!options.locales) {
		options.locales = Object.keys(TARGETS).filter(c => c != 'pt-br');
	}

	return options;
}

function resolveLocales(locales) {
	if (locales === undefined || locales.length === 0) {
		return Object.keys(TARGETS).filter(locale => locale !== SOURCE_LOCALE);
	}

	for (const locale of locales) {
		if (!(locale in TARGETS)) {
			throw new Error(`Unsupported locale: ${locale}`);
		}

		if (locale === SOURCE_LOCALE) {
			throw new Error(`The source locale '${SOURCE_LOCALE}' cannot be used as a translation target.`);
		}
	}

	return locales;
}

function localeFromFile(file) {
	const matchedLocale = file.match(/(de-de|en-us|fr-fr|it-it|ja-jp|ko-kr|pt-br|ru-ru|zh-cn)/);

	return matchedLocale?.[1];
}

async function collectJsonFiles(version, locales) {
	const versionsDir = path.join(PROJECT_ROOT, 'src/jsons/versions', version);

	return locales
		.map(locale => ({
			filePath: path.join(versionsDir, `${locale}.json`),
			sourcePath: path.join(versionsDir, `${SOURCE_LOCALE}.json`),
		}))
		.sort((left, right) => left.filePath.localeCompare(right.filePath));
}

async function collectContentFiles(version, locales) {
	const baseDir = path.join(PROJECT_ROOT, 'src/content', version, SOURCE_LOCALE);
	const files = [];
	const entries = await fs.readdir(baseDir, { recursive: true, withFileTypes: true });

	for (const entry of entries) {
		if (!entry.isFile() || !entry.name.endsWith('.mdx')) {
			continue;
		}

		const sourcePath = path.join(entry.parentPath, entry.name);
		const relativePath = path.relative(baseDir, sourcePath);

		for (const locale of locales) {
			files.push({
				filePath: path.join(PROJECT_ROOT, 'src/content', version, locale, relativePath),
				sourcePath,
			});
		}
	}

	return files.sort((left, right) => left.filePath.localeCompare(right.filePath));
}

function buildPrompt(filePath, sourcePath, locale, content) {
	const targetLanguage = TARGETS[locale];
	const isJson = filePath.endsWith('.json');

	return [
		`Translate the following ${isJson ? 'JSON' : 'MDX'} file from Portuguese (Brazil) to ${targetLanguage}.`,
		'Return only the full translated file contents, with no markdown fences and no commentary.',
		'Requirements:',
		'- Preserve the exact file structure and syntax.',
		'- Preserve keys, property names, paths, identifiers, imports, exports, code fences, code syntax, URLs, and code symbols.',
		'- Translate only human-readable natural language text to the target language.',
		'- Keep locale codes, shortName values, mdxPath values, type names, function names, Rust identifiers, Dinoco API identifiers, and command names unchanged.',
		'- In JSON files, translate only string values meant for UI or documentation text.',
		'- In MDX files, also translate human-readable comments inside code blocks.',
		'- In MDX files, also translate human-readable string literals used as example content, labels, titles, messages, text, names, descriptions, comments, and sample values.',
		'- Do not translate code identifiers, type names, field names, enum names, API names, locale codes, paths, URLs, commands, or syntax tokens.',
		'- If a string literal is acting as an identifier, slug, database key, file path, locale, route segment, or command, keep it unchanged.',
		'- Preserve line breaks as naturally as possible.',
		'- Use the Portuguese (Brazil) source file as the single source of truth.',
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

async function translateFile({ filePath, sourcePath, locale, model, retryAttempts }) {
	const original = await fs.readFile(sourcePath, 'utf8');
	const prompt = buildPrompt(filePath, sourcePath, locale, original);
	const translated = await requestTranslation({
		filePath,
		model,
		prompt,
		retryAttempts,
	});
	const sanitized = stripMarkdownFence(translated);

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
const locales = resolveLocales(options.locales);
const jsonFiles = await collectJsonFiles(options.version, locales);
const contentFiles = await collectContentFiles(options.version, locales);
const files = [...jsonFiles, ...contentFiles];
const rateLimitConfig = resolveRateLimitConfig(options);
const rateLimiter = buildRateLimiter(rateLimitConfig);

console.log(`Using Gemini rate limits for ${options.model}: ${rateLimitConfig.rpm} RPM, concurrency ${rateLimitConfig.concurrency}, start interval ${rateLimiter.intervalMs}ms.`);

await runWithConcurrency(files, rateLimitConfig.concurrency, async file => {
	const locale = localeFromFile(file.filePath);

	if (!locale) {
		throw new Error(`Unable to infer locale for ${file.filePath}`);
	}

	console.log(`Translating ${file.filePath} from ${file.sourcePath}`);

	await rateLimiter.waitTurn();

	await translateFile({
		filePath: file.filePath,
		locale,
		model: options.model,
		retryAttempts: options.retryAttempts,
		sourcePath: file.sourcePath,
	});
});

console.log(`Done. Version: ${options.version}. Locales: ${locales.join(', ')}`);
