import type { Metadata } from 'next';

import './globals.css';

import { DOCS_LOCALE_COOKIE, DOCS_THEME_COOKIE, resolveDocsLocale, resolveDocsTheme } from '../src/lib/docs-preferences';

import { cookies } from 'next/headers';

export const metadata: Metadata = {
	description: 'Official Dinoco documentation',
	title: 'Dinoco documentation',
};

type RootLayoutProps = {
	children: React.ReactNode;
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

const RootLayout = async ({ children }: RootLayoutProps): Promise<React.JSX.Element> => {
	const cookieStore = await cookies();
	const locale = resolveDocsLocale(cookieStore.get(DOCS_LOCALE_COOKIE)?.value);
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

export default RootLayout;
