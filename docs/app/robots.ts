import { SITE_URL } from '../src/lib/site';

import type { MetadataRoute } from 'next';

export default function robots(): MetadataRoute.Robots {
	return {
		host: SITE_URL,
		rules: [
			// Documentation is meant to be found and read by search engines and AI assistants alike;
			// only the JSON-RPC endpoint is kept out of results (it has nothing to index).
			{ allow: ['/', '/llms.txt', '/llms-full.txt'], disallow: ['/mcp'], userAgent: '*' },
		],
		sitemap: `${SITE_URL}/sitemap.xml`,
	};
}
