import { NextResponse } from 'next/server';

import { DOCS_LOCALE_COOKIE } from './src/lib/docs-preferences';
import { SUPPORTED_LOCALES } from './src/jsons/versions';

import type { NextRequest } from 'next/server';
import type { DocsLocale } from './src/jsons/versions';

const DEFAULT_LOCALE: DocsLocale = 'en-us';
const LEGACY_VERSION_SEGMENT = /^\/v\d[\w.-]*(\/.*)?$/;

function negotiateLocale(request: NextRequest): DocsLocale {
	const cookieLocale = request.cookies.get(DOCS_LOCALE_COOKIE)?.value;

	if (SUPPORTED_LOCALES.includes(cookieLocale as DocsLocale)) {
		return cookieLocale as DocsLocale;
	}

	const acceptLanguage = request.headers.get('accept-language') ?? '';

	for (const tag of acceptLanguage.split(',')) {
		const languageCode = tag.trim().split(';')[0]?.toLowerCase();

		if (languageCode?.startsWith('pt')) {
			return 'pt-br';
		}

		if (languageCode?.startsWith('en')) {
			return 'en-us';
		}
	}

	return DEFAULT_LOCALE;
}

export function middleware(request: NextRequest) {
	const { pathname } = request.nextUrl;

	// The pre-reformulation site served every doc page unprefixed at
	// `/{version}/{group}/{item}/{subItem}` (e.g. `/v1.3.3/guide/introduction`).
	// Preserve those links by redirecting into the new locale-prefixed
	// `/docs/orm` product path. The original URLs carried no locale, so this
	// falls back to the negotiated/default locale.
	const legacyMatch = pathname.match(LEGACY_VERSION_SEGMENT);

	if (legacyMatch) {
		const rest = legacyMatch[1] ?? '';
		const locale = negotiateLocale(request);
		const url = request.nextUrl.clone();
		url.pathname = `/${locale}/docs/orm${rest}`;

		return NextResponse.redirect(url);
	}

	if (pathname === '/') {
		const locale = negotiateLocale(request);
		const url = request.nextUrl.clone();
		url.pathname = `/${locale}`;

		return NextResponse.redirect(url);
	}

	return NextResponse.next();
}

export const config = {
	// Runs on every request except static assets and Next.js internals; the
	// function above only actually redirects `/` and legacy `/v*` paths and
	// otherwise passes every other request through untouched.
	matcher: ['/((?!_next/|favicon\\.png|logo\\.png).*)'],
};
