import { notFound } from 'next/navigation';
import { cookies } from 'next/headers';

import '../globals.css';

import { DOCS_THEME_COOKIE, resolveDocsTheme } from '../../src/lib/docs-preferences';
import { SITE_URL } from '../../src/lib/site';
import { SUPPORTED_LOCALES } from '../../src/jsons/versions';

import type { Metadata, Viewport } from 'next';
import type { DocsLocale } from '../../src/jsons/versions';

type LocaleLayoutProps = {
	children: React.ReactNode;
	params: Promise<{ locale: string }>;
};

const localeMetadata: Record<DocsLocale, { description: string; title: string }> = {
	'en-us': {
		description: 'Dinoco is an open-source ecosystem of Rust tools for schema modeling, migrations, and typed database access.',
		title: 'Dinoco',
	},
	'pt-br': {
		description: 'Dinoco é um ecossistema open-source de ferramentas em Rust para modelagem de schema, migrations e acesso tipado a bancos de dados.',
		title: 'Dinoco',
	},
};

export async function generateStaticParams(): Promise<{ locale: string }[]> {
	return SUPPORTED_LOCALES.map(locale => ({ locale }));
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> {
	const { locale } = await params;
	const resolvedLocale: DocsLocale = SUPPORTED_LOCALES.includes(locale as DocsLocale) ? (locale as DocsLocale) : 'en-us';
	const meta = localeMetadata[resolvedLocale];

	return {
		applicationName: 'Dinoco',
		authors: [{ name: 'Dinoco', url: 'https://github.com/dinoco-rs' }],
		category: 'technology',
		description: meta.description,
		icons: { icon: '/favicon.png' },
		keywords: resolvedLocale === 'pt-br' ? ['Dinoco', 'ORM Rust', 'banco de dados', 'migrations', 'queries type-safe'] : ['Dinoco', 'Rust ORM', 'database', 'migrations', 'type-safe queries'],
		metadataBase: new URL(SITE_URL),
		openGraph: { siteName: 'Dinoco', type: 'website' },
		other: { 'mcp-server': `${SITE_URL}/mcp`, 'llms-txt': `${SITE_URL}/llms.txt` },
		title: {
			default: meta.title,
			template: '%s | Dinoco',
		},
	};
}

export const viewport: Viewport = {
	themeColor: [
		{ color: '#ffffff', media: '(prefers-color-scheme: light)' },
		{ color: '#050505', media: '(prefers-color-scheme: dark)' },
	],
};

const themeScript = `
(() => {
	try {
		const persistedTheme = localStorage.getItem('theme');
		const theme = persistedTheme === 'light' ? 'light' : 'dark';
		document.documentElement.classList.toggle('dark', theme === 'dark');
		document.cookie = '${DOCS_THEME_COOKIE}=' + theme + '; path=/; max-age=31536000; samesite=lax';
	} catch {}
})();
`;

const LocaleLayout = async ({ children, params }: LocaleLayoutProps): Promise<React.JSX.Element> => {
	const { locale } = await params;

	if (!SUPPORTED_LOCALES.includes(locale as DocsLocale)) {
		notFound();
	}

	const cookieStore = await cookies();
	const theme = resolveDocsTheme(cookieStore.get(DOCS_THEME_COOKIE)?.value);

	return (
		<html lang={locale} className={theme === 'dark' ? 'dark' : undefined} suppressHydrationWarning>
			<head>
				<meta name="viewport" content="width=device-width, initial-scale=1" />
				<link rel="icon" href="/favicon.png" type="image/png" />
				<link rel="alternate" type="text/plain" href="/llms.txt" title="llms.txt" />
				<link rel="preconnect" href="https://fonts.googleapis.com" />
				<link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
				<link href="https://fonts.googleapis.com/css2?family=Bungee&family=Montserrat:ital,wght@0,100..900;1,100..900&display=swap" rel="stylesheet" />
				<script dangerouslySetInnerHTML={{ __html: themeScript }} />
			</head>
			<body>
				<main>{children}</main>
			</body>
		</html>
	);
};

export default LocaleLayout;
