import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const versions = ['v1.3.0'];
const locales = ['en-us', 'pt-br'];
const contentRoot = path.join(process.cwd(), 'src', 'content');

function flattenInPage(items) {
	return items.flatMap(item => (typeof item === 'string' ? [item] : [item.title, ...flattenInPage(item.items ?? [])]));
}

function flattenItems(items) {
	return items.flatMap(item => [item, ...flattenItems(item.subItems ?? [])]);
}

function markdownHeadings(source) {
	return source
		.split('\n')
		.filter(line => /^#{2,6}\s+/.test(line))
		.map(line => line.replace(/^#{2,6}\s+/, '').trim());
}

const referencedPaths = new Set();
const errors = [];

for (const version of versions) {
	const navigationRoot = path.join(process.cwd(), 'src', 'jsons', 'versions', version);

	for (const locale of locales) {
		const navigationPath = path.join(navigationRoot, `${locale}.json`);
		const navigation = JSON.parse(await readFile(navigationPath, 'utf8'));
		const itemKeys = new Set();

		for (const group of navigation.groups) {
			for (const section of group.sections) {
				for (const item of flattenItems(section.items)) {
					const itemKey = `${group.shortName}/${item.shortName}`;

					if (itemKeys.has(itemKey)) {
						errors.push(`${version}/${locale}: duplicate route ${itemKey}`);
					}
					itemKeys.add(itemKey);

					const expectedPrefix = `${version}/${locale}/`;
					if (!item.contentPath.startsWith(expectedPrefix)) {
						errors.push(`${version}/${locale}: ${item.contentPath} must start with ${expectedPrefix}`);
					}

					const contentPath = path.join(contentRoot, item.contentPath);
					referencedPaths.add(contentPath);

					let source;
					try {
						source = await readFile(contentPath, 'utf8');
					} catch {
						errors.push(`${version}/${locale}: missing content file ${item.contentPath}`);
						continue;
					}

					const availableHeadings = new Set(markdownHeadings(source));
					for (const heading of flattenInPage(item.inPage)) {
						if (!availableHeadings.has(heading)) {
							errors.push(`${version}/${locale}: ${item.contentPath} is missing heading "${heading}"`);
						}
					}
				}
			}
		}
	}

	for (const locale of locales) {
		const localeDirectory = path.join(contentRoot, version, locale);
		for (const name of await readdir(localeDirectory)) {
			const filePath = path.join(localeDirectory, name);
			if (name.endsWith('.md') && !referencedPaths.has(filePath)) {
				errors.push(`${version}/${locale}: unreferenced content file ${name}`);
			}
		}
	}
}

if (errors.length > 0) {
	console.error(errors.map(error => `- ${error}`).join('\n'));
	process.exitCode = 1;
} else {
	console.log(`Validated ${referencedPaths.size} documentation pages for ${versions.join(', ')}.`);
}
