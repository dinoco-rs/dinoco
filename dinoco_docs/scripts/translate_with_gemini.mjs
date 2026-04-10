import fs from 'fs/promises';
import path from 'path';

const API_KEY = process.env.GEMINI_API_KEY;
const DEFAULT_MODEL = 'gemini-2.5-flash';
const DEFAULT_VERSION = 'v0.0.2';
const PROJECT_ROOT = 'dinoco_docs';
const SOURCE_LOCALE = 'pt-br';

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

if (!API_KEY) {
	throw new Error('GEMINI_API_KEY is required.');
}

function parseArgs(argv) {
	const options = {
		locales: undefined,
		model: DEFAULT_MODEL,
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

		if (arg === '--locales' && next) {
			options.locales = next
				.split(',')
				.map(locale => locale.trim())
				.filter(Boolean);
			index += 1;
		}
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

async function translateFile({ filePath, sourcePath, locale, model }) {
	const original = await fs.readFile(sourcePath, 'utf8');
	const prompt = buildPrompt(filePath, sourcePath, locale, original);

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

	if (!response.ok) {
		throw new Error(`Gemini request failed for ${filePath}: ${response.status} ${await response.text()}`);
	}

	const data = await response.json();
	const translated = data.candidates?.[0]?.content?.parts?.map(part => part.text ?? '').join('')?.trim();

	if (!translated) {
		throw new Error(`Gemini returned empty content for ${filePath}`);
	}

	await fs.mkdir(path.dirname(filePath), { recursive: true });
	await fs.writeFile(filePath, `${translated}\n`);
}

const options = parseArgs(process.argv.slice(2));
const locales = resolveLocales(options.locales);
const jsonFiles = await collectJsonFiles(options.version, locales);
const contentFiles = await collectContentFiles(options.version, locales);
const files = [...jsonFiles, ...contentFiles];

for (const file of files) {
	const locale = localeFromFile(file.filePath);

	if (!locale) {
		throw new Error(`Unable to infer locale for ${file.filePath}`);
	}

	console.log(`Translating ${file.filePath} from ${file.sourcePath}`);
	await translateFile({
		filePath: file.filePath,
		locale,
		model: options.model,
		sourcePath: file.sourcePath,
	});
}

console.log(`Done. Version: ${options.version}. Locales: ${locales.join(', ')}`);
