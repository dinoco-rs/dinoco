import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

const contentRoot = path.join(process.cwd(), 'src', 'content');
const versionedContentRoot = path.join(contentRoot, 'v2.0.0');
const navRoot = path.join(process.cwd(), 'src', 'jsons', 'versions', 'v2.0.0');

function toAnchorId(value) {
	return value
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.trim()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-|-$/g, '');
}

function headingsOf(source) {
	return source
		.split('\n')
		.filter(line => /^#{1,6}\s+/.test(line))
		.map(line => toAnchorId(line.replace(/^#{1,6}\s+/, '').trim()));
}

function flattenItems(items) {
	return items.flatMap(item => [item, ...flattenItems(item.subItems ?? [])]);
}

// Build a map of "locale/group/item[/subItem]" -> contentPath, for every locale.
const routeToContentPath = new Map();

for (const locale of ['en-us', 'pt-br']) {
	const navigation = JSON.parse(await readFile(path.join(navRoot, `${locale}.json`), 'utf8'));
	for (const group of navigation.groups) {
		for (const section of group.sections) {
			for (const item of section.items) {
				if (item.subItems === undefined || item.subItems.length === 0) {
					routeToContentPath.set(`${locale}/${group.shortName}/${item.shortName}`, item.contentPath);
					continue;
				}
				for (const subItem of item.subItems) {
					routeToContentPath.set(`${locale}/${group.shortName}/${item.shortName}/${subItem.shortName}`, subItem.contentPath);
				}
			}
		}
	}
}

const headingsCache = new Map();
async function headingsFor(contentPath) {
	if (!headingsCache.has(contentPath)) {
		const source = await readFile(path.join(contentRoot, contentPath), 'utf8');
		headingsCache.set(contentPath, new Set(headingsOf(source)));
	}
	return headingsCache.get(contentPath);
}

const errors = [];
const ROUTE_PATTERN = /^\/(en-us|pt-br)\/docs\/orm\/([a-z-]+)\/([a-z-]+)(?:\/([a-z-]+))?$/;

for (const locale of ['en-us', 'pt-br']) {
	const dir = path.join(versionedContentRoot, locale);
	for (const name of await readdir(dir)) {
		if (!name.endsWith('.md')) continue;
		const filePath = path.join(dir, name);
		const source = await readFile(filePath, 'utf8');
		const ownHeadings = new Set(headingsOf(source));

		for (const match of source.matchAll(/\]\(([^)]+)\)/g)) {
			const target = match[1];
			const hashIndex = target.indexOf('#');
			if (hashIndex === -1) continue;
			const linkPath = target.slice(0, hashIndex);
			const anchor = target.slice(hashIndex + 1);

			if (linkPath === '') {
				if (!ownHeadings.has(anchor)) {
					errors.push(`${locale}/${name}: same-page anchor #${anchor} has no matching heading`);
				}
				continue;
			}

			const routeMatch = linkPath.match(ROUTE_PATTERN);
			if (!routeMatch) continue; // not a recognized internal doc route; skip
			const [, linkLocale, group, item, subItem] = routeMatch;
			const routeKey = subItem ? `${linkLocale}/${group}/${item}/${subItem}` : `${linkLocale}/${group}/${item}`;
			const targetContentPath = routeToContentPath.get(routeKey);

			if (targetContentPath === undefined) {
				errors.push(`${locale}/${name}: link to ${linkPath} does not match any known doc route`);
				continue;
			}

			const targetHeadings = await headingsFor(targetContentPath);
			if (!targetHeadings.has(anchor)) {
				errors.push(`${locale}/${name}: anchor #${anchor} not found in ${targetContentPath} (linked as ${target})`);
			}
		}
	}
}

if (errors.length > 0) {
	console.error(errors.map(e => `- ${e}`).join('\n'));
	process.exitCode = 1;
} else {
	console.log('All internal anchors (same-page and cross-page) resolve to a real heading.');
}
