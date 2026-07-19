import type { DocsLocale } from '../jsons/versions';

const messages = {
	'en-us': {
		description: 'Official documentation',
		donate: 'Support',
		github: 'GitHub',
		inPageLabel: 'On this page',
		localeLabel: 'Language',
		menuLabel: 'Open documentation menu',
		closeMenuLabel: 'Close documentation menu',
		themeDark: 'Use dark theme',
		themeLight: 'Use light theme',
		locales: {
			'en-us': 'English',
			'pt-br': 'Portuguese',
		},
		navigation: {
			closeSubItems: 'Collapse subitems',
			openSubItems: 'Expand subitems',
			previous: 'Previous',
			next: 'Next',
			title: 'Documentation navigation',
		},
		code: {
			copy: 'Copy',
			copied: 'Copied',
			copyLabel: 'Copy code',
			copiedLabel: 'Code copied',
		},
	},
	'pt-br': {
		description: 'Documentação oficial',
		donate: 'Apoiar',
		github: 'GitHub',
		inPageLabel: 'Nesta página',
		localeLabel: 'Idioma',
		menuLabel: 'Abrir menu da documentação',
		closeMenuLabel: 'Fechar menu da documentação',
		themeDark: 'Usar tema escuro',
		themeLight: 'Usar tema claro',
		locales: {
			'en-us': 'Inglês',
			'pt-br': 'Português',
		},
		navigation: {
			closeSubItems: 'Fechar subitens',
			openSubItems: 'Abrir subitens',
			previous: 'Anterior',
			next: 'Próximo',
			title: 'Navegação da documentação',
		},
		code: {
			copy: 'Copiar',
			copied: 'Copiado',
			copyLabel: 'Copiar código',
			copiedLabel: 'Código copiado',
		},
	},
} as const;

export function getIntlMessages(locale: DocsLocale) {
	return messages[locale] ?? messages['en-us'];
}
