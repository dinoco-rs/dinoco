import { SITE_URL } from './site';
import { getLocalizedGroupName } from '../jsons/versions';

import type { DocsLocale, ResolvedDocsPath } from '../jsons/versions';

const BASE_KEYWORDS: Record<DocsLocale, string[]> = {
	'en-us': ['Dinoco', 'Rust ORM', 'Rust database', 'type-safe queries', 'schema migrations', 'PostgreSQL', 'MySQL', 'SQLite'],
	'pt-br': ['Dinoco', 'ORM Rust', 'banco de dados Rust', 'queries type-safe', 'migrations de schema', 'PostgreSQL', 'MySQL', 'SQLite'],
};

/** "Overview" alone is ambiguous across groups, so a page title carries its group unless they are the same. */
export function docsPageTitle(resolved: ResolvedDocsPath, locale: DocsLocale): string {
	const name = resolved.item.name;

	// Sub-pages such as "Overview" repeat across parents, so the parent names them.
	if (resolved.parentItem !== undefined) {
		return `${name} - ${resolved.parentItem.name}`;
	}

	const groupName = getLocalizedGroupName(resolved.group, locale);

	return name.toLowerCase() === groupName.toLowerCase() ? name : `${name} - ${groupName}`;
}

export function docsPageKeywords(resolved: ResolvedDocsPath, locale: DocsLocale): string[] {
	const headings = resolved.item.inPage.slice(0, 6).map(entry => (typeof entry === 'string' ? entry : entry.title));

	return [...new Set([resolved.item.name, ...headings, ...BASE_KEYWORDS[locale]])];
}

const ORGANIZATION = {
	'@type': 'Organization',
	logo: `${SITE_URL}/logo.png`,
	name: 'Dinoco',
	sameAs: ['https://github.com/dinoco-rs/dinoco'],
	url: SITE_URL,
};

export function websiteJsonLd(locale: DocsLocale, description: string) {
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{ '@id': `${SITE_URL}/#website`, '@type': 'WebSite', description, inLanguage: locale, name: 'Dinoco', publisher: ORGANIZATION, url: `${SITE_URL}/${locale}` },
			{
				'@type': 'SoftwareSourceCode',
				codeRepository: 'https://github.com/dinoco-rs/dinoco',
				description,
				license: 'https://www.apache.org/licenses/LICENSE-2.0',
				name: 'Dinoco',
				programmingLanguage: 'Rust',
			},
		],
	};
}

export function docsPageJsonLd(params: { description: string; keywords: string[]; locale: DocsLocale; modifiedAt?: Date; resolved: ResolvedDocsPath; title: string }) {
	const { description, keywords, locale, modifiedAt, resolved, title } = params;
	const url = `${SITE_URL}${resolved.path}`;
	const groupName = getLocalizedGroupName(resolved.group, locale);
	const homeName = locale === 'pt-br' ? 'Início' : 'Home';

	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'TechArticle',
				about: { '@type': 'SoftwareApplication', name: 'Dinoco' },
				dateModified: modifiedAt?.toISOString(),
				description,
				headline: title,
				inLanguage: locale,
				isPartOf: { '@id': `${SITE_URL}/#website` },
				keywords: keywords.join(', '),
				mainEntityOfPage: url,
				proficiencyLevel: 'Beginner',
				publisher: ORGANIZATION,
				url,
			},
			{
				'@type': 'BreadcrumbList',
				itemListElement: [
					{ '@type': 'ListItem', item: `${SITE_URL}/${locale}`, name: homeName, position: 1 },
					{ '@type': 'ListItem', name: groupName, position: 2 },
					{ '@type': 'ListItem', item: url, name: resolved.item.name, position: 3 },
				],
			},
		],
	};
}
