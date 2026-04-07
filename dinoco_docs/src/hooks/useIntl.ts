import { useMemo } from 'react';

import { type DocsLocale, useDocs } from './useDocs';

const messages = {
	pt: {
		consumerLabel: 'Consumindo via',
		consumerOptions: {
			api: 'API',
			cli: 'CLI',
			sdk: 'SDK',
		},
		description: 'Documentação oficial',
		donate: 'Doar',
		github: 'GitHub',
		localeLabel: 'Idioma',
		locales: {
			pt: 'pt-BR',
		},
		nav: {
			docs: 'Docs',
			examples: 'Exemplos',
			guides: 'Guias',
			reference: 'Referencia',
		},
		themeDark: 'Modo escuro',
		themeLight: 'Modo claro',
		versionLabel: 'Versao',
		constructionBadge: 'Em construcao',
		constructionDescription:
			'Esta area ainda esta sendo estruturada. O conteudo aqui serve como base inicial de navegacao e sera expandido nas proximas iteracoes.',
		inPageLabel: 'Nesta pagina',
	},
} as const;

export function getIntlMessages(locale: DocsLocale) {
	return messages[locale];
}

export function useIntl() {
	const locale = useDocs(state => state.locale);

	return useMemo(() => getIntlMessages(locale), [locale]);
}
