import { cookies } from 'next/headers';
import { notFound, redirect } from 'next/navigation';

import DocsPage from '../../../../../src/components/DocsPage';
import MarkdownContent from '../../../../../src/components/MarkdownContent';
import { DOCS_THEME_COOKIE, resolveDocsTheme } from '../../../../../src/lib/docs-preferences';
import { docsPageJsonLd, docsPageKeywords, docsPageTitle } from '../../../../../src/lib/seo';
import { loadDocsIndex } from '../../../../../src/lib/docs-index';
import { SITE_URL } from '../../../../../src/lib/site';
import { SUPPORTED_LOCALES, getAllDocsSlugs, parseDocsPath, resolveDocsPath } from '../../../../../src/jsons/versions';

import type { Metadata } from 'next';
import type { DocsLocale } from '../../../../../src/jsons/versions';

type OrmDocsPageProps = {
	params: Promise<{
		locale: string;
		slug?: string[];
	}>;
};

function resolveLocale(locale: string): DocsLocale {
	return SUPPORTED_LOCALES.includes(locale as DocsLocale) ? (locale as DocsLocale) : 'en-us';
}

export async function generateStaticParams(): Promise<{ locale: string; slug: string[] }[]> {
	return getAllDocsSlugs();
}

async function resolvePageData(locale: string, slug?: string[]) {
	const resolvedLocale = resolveLocale(locale);
	const cookieStore = await cookies();
	const theme = resolveDocsTheme(cookieStore.get(DOCS_THEME_COOKIE)?.value);
	const routeParams = parseDocsPath(slug);
	const resolved = resolveDocsPath({
		groupShortName: routeParams.groupShortName,
		itemShortName: routeParams.itemShortName,
		subItemShortName: routeParams.subItemShortName,
		locale: resolvedLocale,
	});

	return { locale: resolvedLocale, resolved, theme };
}

export async function generateMetadata({ params }: OrmDocsPageProps): Promise<Metadata> {
	const { locale, slug } = await params;
	const { locale: resolvedLocale, resolved } = await resolvePageData(locale, slug);

	if (resolved === undefined) {
		return { title: 'Dinoco ORM documentation' };
	}

	const otherLocale: DocsLocale = resolvedLocale === 'en-us' ? 'pt-br' : 'en-us';
	const alternateResolved = resolveDocsPath({
		groupShortName: resolved.group.shortName,
		itemShortName: resolved.parentItem?.shortName ?? resolved.item.shortName,
		subItemShortName: resolved.parentItem?.shortName === undefined ? undefined : resolved.item.shortName,
		locale: otherLocale,
	});

	const title = docsPageTitle(resolved, resolvedLocale);
	const url = `${SITE_URL}${resolved.path}`;
	const languages: Record<string, string> = {
		[resolvedLocale]: url,
		...(alternateResolved ? { [otherLocale]: `${SITE_URL}${alternateResolved.path}` } : {}),
	};
	const imageUrl = `${SITE_URL}/og${resolved.path.replace('/docs/orm', '')}`;
	const englishUrl = resolvedLocale === 'en-us' ? url : alternateResolved ? `${SITE_URL}${alternateResolved.path}` : url;

	return {
		alternates: { canonical: url, languages: { ...languages, 'x-default': englishUrl } },
		description: resolved.item.description,
		keywords: docsPageKeywords(resolved, resolvedLocale),
		openGraph: {
			alternateLocale: alternateResolved ? [otherLocale] : undefined,
			description: resolved.item.description,
			images: [{ alt: title, height: 630, url: imageUrl, width: 1200 }],
			locale: resolvedLocale,
			siteName: 'Dinoco',
			title,
			type: 'article',
			url,
		},
		robots: { follow: true, index: true },
		title,
		twitter: { card: 'summary_large_image', description: resolved.item.description, images: [imageUrl], title },
	};
}

const OrmDocsPage = async ({ params }: OrmDocsPageProps): Promise<React.JSX.Element> => {
	const { locale, slug } = await params;

	if (!SUPPORTED_LOCALES.includes(locale as DocsLocale)) {
		notFound();
	}

	const { locale: resolvedLocale, resolved, theme } = await resolvePageData(locale, slug);

	if (resolved === undefined) {
		notFound();
	}

	const requestedPath = `/${locale}/docs/orm${slug && slug.length > 0 ? `/${slug.join('/')}` : ''}`;

	if (resolved.path !== requestedPath) {
		redirect(resolved.path);
	}

	const entry = (await loadDocsIndex()).find(page => page.locale === resolvedLocale && page.path === resolved.path);
	const jsonLd = docsPageJsonLd({
		description: resolved.item.description,
		keywords: docsPageKeywords(resolved, resolvedLocale),
		locale: resolvedLocale,
		modifiedAt: entry?.modifiedAt,
		resolved,
		title: docsPageTitle(resolved, resolvedLocale),
	});

	return (
		<>
			<script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, '\\u003c') }} />
			<DocsPage initialLocale={resolvedLocale} initialTheme={theme} resolved={resolved}>
				<MarkdownContent contentPath={resolved.item.contentPath} />
			</DocsPage>
		</>
	);
};

export default OrmDocsPage;
