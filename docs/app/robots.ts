import { SITE_URL } from '../src/lib/site';

import type { MetadataRoute } from 'next';

export default function robots(): MetadataRoute.Robots {
	return {
		rules: {
			allow: '/',
			userAgent: '*',
		},
		sitemap: `${SITE_URL}/sitemap.xml`,
	};
}
