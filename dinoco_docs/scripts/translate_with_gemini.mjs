import fs from 'fs/promises';
import path from 'path';

const API_KEY = process.env.GEMINI_API_KEY;
const DEFAULT_MODEL = 'gemini-2.5-flash';
const DEFAULT_VERSION = 'v0.0.1';
const PROJECT_ROOT = 'dinoco_docs';

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
		return Object.keys(TARGETS);
	}

	for (const locale of locales) {
		if (!(locale in TARGETS)) {
			throw new Error(`Unsupported locale: ${locale}`);
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
	const files = await fs.readdir(versionsDir);

	return files
		.filter(file => file.endsWith('.json'))
		.map(file => path.join(versionsDir, file))
		.filter(file => {
			const locale = localeFromFile(file);

			return locale !== undefined && locales.includes(locale);
		})
		.sort();
}

async function collectContentFiles(version, locales) {
	const baseDir = path.join(PROJECT_ROOT, 'src/content', version);
	const files = [];

	for (const locale of locales) {
		const localeDir = path.join(baseDir, locale);
		const entries = await fs.readdir(localeDir, { recursive: true, withFileTypes: true });

		for (const entry of entries) {
			if (!entry.isFile() || !entry.name.endsWith('.mdx')) {
				continue;
			}

			files.push(path.join(entry.parentPath, entry.name));
		}
	}

	return files.sort();
}

function buildPrompt(filePath, locale, content) {
	const targetLanguage = TARGETS[locale];
	const isJson = filePath.endsWith('.json');

	return [
		`Translate the following ${isJson ? 'JSON' : 'MDX'} file to ${targetLanguage}.`,
		'Return only the full translated file contents, with no markdown fences and no commentary.',
		'Requirements:',
		'- Preserve the exact file structure and syntax.',
		'- Preserve keys, property names, paths, identifiers, imports, exports, code fences, code syntax, URLs, and code symbols.',
		'- Translate only human-readable natural language text to the target language.',
		'- Keep locale codes, shortName values, mdxPath values, type names, function names, Rust identifiers, Dinoco API identifiers, and command names unchanged.',
		'- In JSON files, translate only string values meant for UI or documentation text.',
		'- In MDX files, keep code blocks unchanged unless they contain prose comments that should obviously be translated.',
		'- Preserve line breaks as naturally as possible.',
		'File path:',
		filePath,
		'File contents:',
		content,
	].join('\n');
}

async function translateFile({ filePath, locale, model }) {
	const original = await fs.readFile(filePath, 'utf8');
	const prompt = buildPrompt(filePath, locale, original);

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

	await fs.writeFile(filePath, `${translated}\n`);
}

const options = parseArgs(process.argv.slice(2));
const locales = resolveLocales(options.locales);
const jsonFiles = await collectJsonFiles(options.version, locales);
const contentFiles = await collectContentFiles(options.version, locales);
const files = [...jsonFiles, ...contentFiles];

for (const filePath of files) {
	const locale = localeFromFile(filePath);

	if (!locale) {
		throw new Error(`Unable to infer locale for ${filePath}`);
	}

	console.log(`Translating ${filePath}`);
	await translateFile({
		filePath,
		locale,
		model: options.model,
	});
}

console.log(`Done. Version: ${options.version}. Locales: ${locales.join(', ')}`);
