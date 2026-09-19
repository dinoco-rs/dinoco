import { loadDocsIndex } from '../src/lib/docs-index';
import { SITE_URL } from '../src/lib/site';
import { SUPPORTED_LOCALES } from '../src/jsons/versions';

import type { MetadataRoute } from 'next';

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
	const index = await loadDocsIndex();
	const latest = new Date(Math.max(...index.map(entry => entry.modifiedAt.getTime())));

	const homeEntries: MetadataRoute.Sitemap = SUPPORTED_LOCALES.map(locale => ({
		alternates: { languages: Object.fromEntries(SUPPORTED_LOCALES.map(other => [other, `${SITE_URL}/${other}`])) },
		changeFrequency: 'monthly',
		lastModified: latest,
		priority: 1,
		url: `${SITE_URL}/${locale}`,
	}));

	// Every page exists in each locale under the same `group/item`, so the alternates are the same id in the other locales.
	const docsEntries: MetadataRoute.Sitemap = index.map(entry => ({
		alternates: {
			languages: Object.fromEntries(index.filter(other => other.id === entry.id).map(other => [other.locale, other.url])),
		},
		changeFrequency: 'weekly',
		lastModified: entry.modifiedAt,
		priority: entry.group === 'guide' ? 0.9 : 0.8,
		url: entry.url,
	}));

	return [...homeEntries, ...docsEntries];
}
