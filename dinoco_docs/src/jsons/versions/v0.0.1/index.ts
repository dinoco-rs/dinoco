import deDe from './de-de.json';
import enUs from './en-us.json';
import frFr from './fr-fr.json';
import itIt from './it-it.json';
import jaJp from './ja-jp.json';
import koKr from './ko-kr.json';
import ptBr from './pt-br.json';
import ruRu from './ru-ru.json';
import zhCn from './zh-cn.json';

type DocsLocale = 'pt-br' | 'en-us' | 'ru-ru' | 'ja-jp' | 'ko-kr' | 'de-de' | 'it-it' | 'zh-cn' | 'fr-fr';

type DocsInPageItemData =
	| string
	| {
			items?: DocsInPageItemData[];
			title: string;
	  };

type DocsItemData = {
	description?: string;
	inPage: DocsInPageItemData[];
	mdxPath: string;
	name: string;
	shortName: string;
	subItems?: DocsItemData[];
};

type DocsSectionData = {
	items: DocsItemData[];
	title: string;
};

type LocaleGroupData = {
	icon: string;
	name: string;
	sections: DocsSectionData[];
	shortName: string;
	status?: 'comingSoon';
};

type LocaleVersionData = {
	description?: string;
	groups: LocaleGroupData[];
	locale: DocsLocale;
	name: string;
};

type VersionGroupData = {
	icon: string;
	languages: Partial<Record<DocsLocale, DocsSectionData[]>>;
	name: string;
	shortName: string;
	status?: 'comingSoon';
};

export type VersionData = {
	description: Partial<Record<DocsLocale, string>>;
	groups: VersionGroupData[];
	name: string;
};

const localizedVersions: LocaleVersionData[] = [
	ptBr as LocaleVersionData,
	enUs as LocaleVersionData,
	ruRu as LocaleVersionData,
	jaJp as LocaleVersionData,
	koKr as LocaleVersionData,
	deDe as LocaleVersionData,
	itIt as LocaleVersionData,
	zhCn as LocaleVersionData,
	frFr as LocaleVersionData,
];

function assertGroupConsistency(baseGroups: LocaleGroupData[], localeVersion: LocaleVersionData) {
	if (localeVersion.groups.length !== baseGroups.length) {
		throw new Error(`Invalid versions data for ${localeVersion.locale}: group length mismatch.`);
	}

	for (const [index, group] of localeVersion.groups.entries()) {
		const baseGroup = baseGroups[index];

		if (baseGroup?.shortName !== group.shortName) {
			throw new Error(`Invalid versions data for ${localeVersion.locale}: group order mismatch at index ${index}.`);
		}
	}
}

const [baseVersion, ...otherVersions] = localizedVersions;

if (baseVersion === undefined) {
	throw new Error('No localized versions data found for v0.0.1.');
}

for (const localeVersion of otherVersions) {
	assertGroupConsistency(baseVersion.groups, localeVersion);
}

const versionData: VersionData = {
	description: Object.fromEntries(
		localizedVersions.flatMap(localeVersion =>
			localeVersion.description === undefined ? [] : [[localeVersion.locale, localeVersion.description] as const],
		),
	),
	groups: baseVersion.groups.map((group, index) => ({
		icon: group.icon,
		languages: Object.fromEntries(localizedVersions.map(localeVersion => [localeVersion.locale, localeVersion.groups[index]?.sections ?? []])),
		name: group.name,
		shortName: group.shortName,
		status: group.status,
	})),
	name: baseVersion.name,
};

export default versionData;
