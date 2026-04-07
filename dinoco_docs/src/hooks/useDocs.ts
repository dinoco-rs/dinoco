import { create } from 'zustand';

import { getDefaultVersionName } from '../jsons/versions';

export type DocsLocale = 'pt';
export type DocsTheme = 'light' | 'dark';
export type DocsConsumer = string;

type DocsState = {
	consumer: DocsConsumer;
	locale: DocsLocale;
	theme: DocsTheme;
	version: string;
	setConsumer: (consumer: DocsConsumer) => void;
	setLocale: (locale: DocsLocale) => void;
	setTheme: (theme: DocsTheme) => void;
	setVersion: (version: string) => void;
};

export function getSystemTheme(): DocsTheme {
	if (typeof window == 'undefined') return 'dark';

	const persistedTheme = localStorage.getItem('theme') as DocsTheme | undefined;
	if (persistedTheme) return persistedTheme;

	return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export const useDocs = create<DocsState>()(set => ({
	consumer: 'cli',
	locale: 'pt',
	theme: getSystemTheme(),
	version: getDefaultVersionName(),
	setConsumer: consumer => set({ consumer }),
	setLocale: locale => set({ locale }),
	setTheme: theme => {
		localStorage.setItem('theme', theme);

		document.documentElement.classList.toggle('dark', theme === 'dark');
		document.documentElement.lang = 'pt-BR';

		set({ theme });
	},
	setVersion: version => set({ version }),
}));
