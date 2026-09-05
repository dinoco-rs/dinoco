import { SITE_URL } from '../src/lib/site';
import { SUPPORTED_LOCALES, buildDocsPath, getAllDocsSlugs } from '../src/jsons/versions';

import type { MetadataRoute } from 'next';

export default function sitemap(): MetadataRoute.Sitemap {
	const homeEntries: MetadataRoute.Sitemap = SUPPORTED_LOCALES.map(locale => ({
		url: `${SITE_URL}/${locale}`,
		changeFrequency: 'monthly',
		priority: 1,
	}));

	const docsEntries: MetadataRoute.Sitemap = getAllDocsSlugs().map(({ locale, slug }) => ({
		url: `${SITE_URL}${buildDocsPath(locale, slug[0], slug[1], slug[2])}`,
		changeFrequency: 'weekly',
		priority: 0.8,
	}));

	return [...homeEntries, ...docsEntries];
}
