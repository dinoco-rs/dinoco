import { cookies } from 'next/headers';
import { notFound, redirect } from 'next/navigation';

import DocsPage from '../../src/components/DocsPage';
import MarkdownContent from '../../src/components/MarkdownContent';
import { DOCS_LOCALE_COOKIE, DOCS_THEME_COOKIE, resolveDocsLocale, resolveDocsTheme } from '../../src/lib/docs-preferences';
import { getDefaultVersionName, parseDocsPath, resolveDocsPath } from '../../src/jsons/versions';

import type { Metadata } from 'next';

type DocsRoutePageProps = {
	params: Promise<{
		slug?: string[];
	}>;
};

function toPathname(slug?: string[]): string {
	if (slug === undefined || slug.length === 0) {
		return '/';
	}

	return `/${slug.slice(0, 4).join('/')}`;
}

async function resolvePageData(slug?: string[]) {
	const cookieStore = await cookies();
	const locale = resolveDocsLocale(cookieStore.get(DOCS_LOCALE_COOKIE)?.value);
	const theme = resolveDocsTheme(cookieStore.get(DOCS_THEME_COOKIE)?.value);
	const pathname = toPathname(slug);
	const routeParams = parseDocsPath(pathname);
	const resolved = resolveDocsPath({
		versionName: routeParams.versionName,
		groupShortName: routeParams.groupShortName,
		itemShortName: routeParams.itemShortName,
		subItemShortName: routeParams.subItemShortName,
		locale,
	});

	return {
		locale,
		pathname,
		resolved,
		theme,
	};
}

export async function generateMetadata({ params }: DocsRoutePageProps): Promise<Metadata> {
	const { slug } = await params;
	const { resolved } = await resolvePageData(slug);

	if (resolved === undefined) {
		return {
			title: 'Dinoco documentation',
		};
	}

	return {
		title: `${resolved.item.documentTitle} | Dinoco`,
	};
}

const DocsRoutePage = async ({ params }: DocsRoutePageProps): Promise<React.JSX.Element> => {
	const { slug } = await params;
	const { locale, pathname, resolved, theme } = await resolvePageData(slug);

	if (resolved === undefined) {
		notFound();
	}

	const resolvedPath = resolved.path;

	if (pathname !== resolvedPath) {
		redirect(resolvedPath);
	}

	if (pathname === '/') {
		redirect(`/${getDefaultVersionName()}`);
	}

	return (
		<DocsPage initialLocale={locale} initialTheme={theme} pathname={resolvedPath} resolved={resolved}>
			<MarkdownContent contentPath={resolved.item.contentPath} />
		</DocsPage>
	);
};

export default DocsRoutePage;
