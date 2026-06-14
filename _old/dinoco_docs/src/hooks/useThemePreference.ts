'use client';

import { useEffect, useState } from 'react';

import { persistDocsTheme } from '../lib/docs-preferences';

import type { DocsTheme } from '../lib/docs-preferences';

export function useThemePreference(initialTheme: DocsTheme) {
	const [theme, setThemeState] = useState<DocsTheme>(initialTheme);

	useEffect(() => {
		setThemeState(initialTheme);
		persistDocsTheme(initialTheme);
	}, [initialTheme]);

	const setTheme = (nextTheme: DocsTheme) => {
		setThemeState(nextTheme);
		persistDocsTheme(nextTheme);
	};

	return {
		theme,
		setTheme,
	};
}
