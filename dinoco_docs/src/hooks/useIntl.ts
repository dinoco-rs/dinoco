import type { DocsLocale } from '../jsons/versions';

const messages = {
	'pt-br': {
		description: 'Documentação oficial',
		donate: 'Apoiar',
		github: 'GitHub',
		localeLabel: 'Idioma',
		locales: {
			'pt-br': 'Português (Brasil)',
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
		versionOld: 'Você está consultando a documentação da versão antiga',
		versionNew: 'A versão mais recente é a',
		goto: 'Ir para',
		constructionBadge: 'Em construção',
		constructionTitle: 'Conteúdo em desenvolvimento',
		constructionDescription: 'Esta seção da documentação ainda está em desenvolvimento. Volte em breve para conferir as novidades!',
		inPageLabel: 'Nesta página',
	},
} as const;

export function getIntlMessages(locale: DocsLocale) {
	return messages[locale];
}
