import { ImageResponse } from 'next/og';

import { SUPPORTED_LOCALES, getLocalizedGroupName, parseDocsPath, resolveDocsPath } from '../../../../src/jsons/versions';

import type { DocsLocale } from '../../../../src/jsons/versions';

const size = { width: 1200, height: 630 };

// A route handler rather than a file-convention `opengraph-image`, which cannot live below a catch-all segment.
export async function GET(_request: Request, { params }: { params: Promise<{ locale: string; slug?: string[] }> }): Promise<Response> {
	const { locale, slug } = await params;
	const resolvedLocale: DocsLocale = SUPPORTED_LOCALES.includes(locale as DocsLocale) ? (locale as DocsLocale) : 'en-us';
	const route = parseDocsPath(slug);
	const resolved = resolveDocsPath({ groupShortName: route.groupShortName, itemShortName: route.itemShortName, locale: resolvedLocale, subItemShortName: route.subItemShortName });
	const title = resolved === undefined ? 'Dinoco' : resolved.parentItem === undefined ? resolved.item.name : `${resolved.parentItem.name}: ${resolved.item.name}`;
	const group = resolved === undefined ? '' : getLocalizedGroupName(resolved.group, resolvedLocale);
	const description = resolved?.item.description ?? '';

	return new ImageResponse(
		(
			<div style={{ background: '#050505', color: '#ffffff', display: 'flex', flexDirection: 'column', height: '100%', justifyContent: 'space-between', padding: 72, width: '100%' }}>
				<div style={{ color: '#00ffff', display: 'flex', fontSize: 34, fontWeight: 700 }}>{`Dinoco  ·  ${group}`}</div>
				<div style={{ display: 'flex', flexDirection: 'column' }}>
					<div style={{ display: 'flex', fontSize: title.length > 28 ? 72 : 92, fontWeight: 800, lineHeight: 1.05 }}>{title}</div>
					<div style={{ color: '#94a3b8', display: 'flex', fontSize: 34, lineHeight: 1.3, marginTop: 28, maxWidth: 1000 }}>{description}</div>
				</div>
				<div style={{ color: '#64748b', display: 'flex', fontSize: 28 }}>dinoco.io</div>
			</div>
		),
		{ ...size },
	);
}
