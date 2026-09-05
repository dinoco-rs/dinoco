import { cookies } from 'next/headers';
import { notFound } from 'next/navigation';

import HomePage from '../../src/components/HomePage';
import { DOCS_THEME_COOKIE, resolveDocsTheme } from '../../src/lib/docs-preferences';
import { SITE_URL } from '../../src/lib/site';
import { SUPPORTED_LOCALES } from '../../src/jsons/versions';

import type { Metadata } from 'next';
import type { DocsLocale } from '../../src/jsons/versions';

type HomeRouteProps = {
	params: Promise<{ locale: string }>;
};

const heroCopy: Record<DocsLocale, string> = {
	'en-us': 'Open-source tools for building fast, reliable software.',
	'pt-br': 'Ferramentas open-source para construir software rápido e confiável.',
};

export async function generateMetadata({ params }: HomeRouteProps): Promise<Metadata> {
	const { locale } = await params;
	const resolvedLocale: DocsLocale = SUPPORTED_LOCALES.includes(locale as DocsLocale) ? (locale as DocsLocale) : 'en-us';

	return {
		alternates: {
			canonical: `${SITE_URL}/${resolvedLocale}`,
			languages: Object.fromEntries(SUPPORTED_LOCALES.map(supported => [supported, `${SITE_URL}/${supported}`])),
		},
		description: heroCopy[resolvedLocale],
		openGraph: {
			description: heroCopy[resolvedLocale],
			locale: resolvedLocale,
			title: 'Dinoco',
			type: 'website',
			url: `${SITE_URL}/${resolvedLocale}`,
		},
		title: 'Dinoco',
	};
}

const HomeRoute = async ({ params }: HomeRouteProps): Promise<React.JSX.Element> => {
	const { locale } = await params;

	if (!SUPPORTED_LOCALES.includes(locale as DocsLocale)) {
		notFound();
	}

	const cookieStore = await cookies();
	const theme = resolveDocsTheme(cookieStore.get(DOCS_THEME_COOKIE)?.value);

	return <HomePage locale={locale as DocsLocale} theme={theme} />;
};

export default HomeRoute;
