import { notFound } from 'next/navigation';
import { cookies } from 'next/headers';

import '../globals.css';

import { DOCS_THEME_COOKIE, resolveDocsTheme } from '../../src/lib/docs-preferences';
import { SITE_URL } from '../../src/lib/site';
import { SUPPORTED_LOCALES } from '../../src/jsons/versions';

import type { Metadata } from 'next';
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
		description: meta.description,
		metadataBase: new URL(SITE_URL),
		title: {
			default: meta.title,
			template: '%s | Dinoco',
		},
	};
}

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
				<link rel="shortcut icon" href="/favicon.png" type="image/png" />
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
