import { useMemo } from 'react';

import { type DocsLocale, useDocs } from './useDocs';

const messages = {
	pt: {
		description: 'Documentação oficial',
		donate: 'Apoiar',
		github: 'GitHub',
		localeLabel: 'Idioma',
		locales: {
			pt: 'pt-BR',
		},
		nav: {
			docs: 'Docs',
			examples: 'Exemplos',
			guides: 'Guias',
			reference: 'Referência',
		},
		themeDark: 'Modo escuro',
		themeLight: 'Modo claro',
		versionLabel: 'Versão',
		constructionBadge: 'Em construção',
		constructionTitle: 'Conteúdo em desenvolvimento',
		constructionDescription: 'Esta seção da documentação ainda está em desenvolvimento. Volte em breve para conferir as novidades!',
		inPageLabel: 'Nesta página',
	},
} as const;

export function getIntlMessages(locale: DocsLocale) {
	return messages[locale];
}

export function useIntl() {
	const locale = useDocs(state => state.locale);

	return useMemo(() => getIntlMessages(locale), [locale]);
}
