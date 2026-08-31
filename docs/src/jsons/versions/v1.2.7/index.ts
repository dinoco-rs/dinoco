import enUs from './en-us.json';
import ptBr from './pt-br.json';

const localizedContent = {
	'en-us': enUs,
	'pt-br': ptBr,
} as const;

const version = {
	name: 'v1.2.7',
	description: {
		'en-us': 'Type-safe database access for Rust, from schema to production.',
		'pt-br': 'Acesso type-safe a bancos de dados em Rust, do schema a producao.',
	},
	groups: enUs.groups.map(group => ({
		icon: group.icon,
		name: group.name,
		shortName: group.shortName,
		localizedNames: Object.fromEntries(
			Object.entries(localizedContent).map(([locale, content]) => [
				locale,
				content.groups.find(candidate => candidate.shortName === group.shortName)?.name ?? group.name,
			]),
		),
		languages: Object.fromEntries(
			Object.entries(localizedContent).map(([locale, content]) => {
				const localizedGroup = content.groups.find(candidate => candidate.shortName === group.shortName);

				return [locale, localizedGroup?.sections ?? group.sections];
			}),
		),
	})),
};

export default version;
