import type { DocsLocale } from '../jsons/versions';

export type DocsTheme = 'light' | 'dark';

export const DOCS_LOCALE_COOKIE = 'docs-locale';
export const DOCS_THEME_COOKIE = 'docs-theme';

const SUPPORTED_LOCALES: DocsLocale[] = ['pt-br', 'en-us', 'ru-ru', 'ja-jp', 'ko-kr', 'de-de', 'it-it', 'zh-cn', 'fr-fr'];

export function resolveDocsLocale(value?: string): DocsLocale {
	return SUPPORTED_LOCALES.includes(value as DocsLocale) ? (value as DocsLocale) : 'en-us';
}

export function resolveDocsTheme(value?: string): DocsTheme {
	return value === 'light' ? 'light' : 'dark';
}

export function persistDocsLocale(locale: DocsLocale): void {
	if (typeof document === 'undefined') {
		return;
	}

	document.documentElement.lang = locale;
	localStorage.setItem('locale', locale);
	document.cookie = `${DOCS_LOCALE_COOKIE}=${locale}; path=/; max-age=31536000; samesite=lax`;
}

export function persistDocsTheme(theme: DocsTheme): void {
	if (typeof document === 'undefined') {
		return;
	}

	localStorage.setItem('theme', theme);
	document.documentElement.classList.toggle('dark', theme === 'dark');
	document.cookie = `${DOCS_THEME_COOKIE}=${theme}; path=/; max-age=31536000; samesite=lax`;
}
