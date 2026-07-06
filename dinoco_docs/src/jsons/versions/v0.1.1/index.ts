import ptBr from './pt-br.json';

type DocsInPageItemData =
	| string
	| {
			items?: DocsInPageItemData[];
			title: string;
	  };

type DocsItemData = {
	description?: string;
	inPage: DocsInPageItemData[];
	contentPath: string;
	name: string;
	shortName: string;
	subItems?: DocsItemData[];
};

type DocsSectionData = {
	items: DocsItemData[];
	title: string;
};

type LocaleVersionData = {
	description?: string;
	groups: Array<{
		icon: string;
		name: string;
		sections: DocsSectionData[];
		shortName: string;
		status?: 'comingSoon';
	}>;
	locale: 'pt-br';
	name: string;
};

const version = ptBr as LocaleVersionData;

export default {
	description: version.description === undefined ? {} : { 'pt-br': version.description },
	groups: version.groups.map(group => ({
		icon: group.icon,
		languages: { 'pt-br': group.sections },
		name: group.name,
		shortName: group.shortName,
		status: group.status,
	})),
	name: version.name,
};
