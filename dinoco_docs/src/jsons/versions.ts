import React from 'react';

import versionsData from './versions.json';

export type DocsLocale = 'pt';

type MdxModule = {
	default: React.ComponentType<{ components?: Record<string, React.ElementType> }>;
};

type DocsItemData = {
	description: string;
	inPage: DocsInPageItemData[];
	mdxPath: string;
	name: string;
	shortName: string;
	subItems?: DocsItemData[];
};

export type DocsInPageItemData =
	| string
	| {
			items?: DocsInPageItemData[];
			title: string;
	  };

type DocsSectionData = {
	items: DocsItemData[];
	title: string;
};

type DocsGroupData = {
	icon: string;
	languages: Partial<Record<DocsLocale, DocsSectionData[]>>;
	name: string;
	shortName: string;
	status?: 'comingSoon';
};

type DocsVersionData = {
	description: Record<DocsLocale, string>;
	groups: DocsGroupData[];
	name: string;
};

export type DocsItem = Omit<DocsItemData, 'subItems'> & {
	component: React.ComponentType<{ components?: Record<string, React.ElementType> }>;
	subItems?: DocsItem[];
};

export type DocsSection = {
	items: DocsItem[];
	title: string;
};

export type DocsNavigationItem = {
	item: DocsItem;
	parentItem?: DocsItem;
	path: string;
};

export type DocsGroup = Omit<DocsGroupData, 'languages'> & {
	languages: Partial<Record<DocsLocale, DocsSection[]>>;
};

export type DocsVersion = Omit<DocsVersionData, 'groups'> & {
	groups: DocsGroup[];
};

const mdxModules = import.meta.glob('../content/**/*.mdx', {
	eager: true,
}) as Record<string, MdxModule>;

function getMdxComponent(path: string) {
	return mdxModules[`../content/${path}`]?.default ?? (() => null);
}

function mapItem(item: DocsItemData): DocsItem {
	return {
		...item,
		component: getMdxComponent(item.mdxPath),
		subItems: item.subItems?.map(mapItem),
	};
}

export const versions: DocsVersion[] = (versionsData as DocsVersionData[]).map(version => ({
	...version,
	groups: version.groups.map(group => ({
		...group,
		languages: Object.fromEntries(
			Object.entries(group.languages).map(([locale, sections]) => [
				locale,
				(sections ?? []).map(section => ({
					...section,
					items: section.items.map(mapItem),
				})),
			]),
		) as Partial<Record<DocsLocale, DocsSection[]>>,
	})),
}));

export type ResolvedDocsPath = {
	group: DocsGroup;
	item: DocsItem;
	parentItem?: DocsItem;
	path: string;
	sections: DocsSection[];
	version: DocsVersion;
};

function resolveEntryItem(entry: DocsItem, subItemShortName?: string): { item: DocsItem; parentItem?: DocsItem } {
	if (entry.subItems === undefined || entry.subItems.length === 0) {
		return {
			item: entry,
		};
	}

	const matchedSubItem = subItemShortName === undefined ? undefined : entry.subItems.find(subItem => subItem.shortName === subItemShortName);
	const fallbackSubItem = matchedSubItem ?? entry.subItems[0];

	return {
		item: fallbackSubItem,
		parentItem: entry,
	};
}

function fallbackLocale(locale: DocsLocale, version: DocsVersion): DocsLocale {
	const localeSet = getAvailableLocales(version.name);

	if (localeSet.includes(locale)) {
		return locale;
	}

	return localeSet[0] ?? 'pt';
}

export function getLatestVersionName(): string {
	return versions[0]?.name ?? 'v0.0.1';
}

export function isLatestVersion(versionName: string): boolean {
	return versionName === getLatestVersionName();
}

export function getDefaultVersionName(): string {
	return getLatestVersionName();
}

export function getVersionNames(): string[] {
	return versions.map(version => version.name);
}

export function getVersionByName(versionName: string): DocsVersion | undefined {
	return versions.find(version => version.name === versionName);
}

export function getAvailableLocales(versionName: string): DocsLocale[] {
	return getVersionByName(versionName) === undefined ? ['pt'] : ['pt'];
}

export function getGroupsForVersion(versionName: string, locale: DocsLocale): DocsGroup[] {
	const version = getVersionByName(versionName);

	if (version === undefined) {
		return [];
	}

	const resolvedLocale = fallbackLocale(locale, version);

	return version.groups.filter(group => {
		const localizedSections = group.languages[resolvedLocale] ?? group.languages.pt;

		return localizedSections !== undefined && localizedSections.length > 0;
	});
}

export function getLocalizedSections(group: DocsGroup, locale: DocsLocale): DocsSection[] {
	return group.languages[locale] ?? group.languages.pt ?? [];
}

export function getGroupByShortName(
	versionName: string,
	locale: DocsLocale,
	groupShortName?: string,
): DocsGroup | undefined {
	const groups = getGroupsForVersion(versionName, locale);

	if (groupShortName === undefined) {
		return groups[0];
	}

	return groups.find(group => group.shortName === groupShortName) ?? groups[0];
}

export function getItemByShortName(
	versionName: string,
	locale: DocsLocale,
	groupShortName?: string,
	itemShortName?: string,
	subItemShortName?: string,
): {
	group: DocsGroup;
	item: DocsItem;
	parentItem?: DocsItem;
	sections: DocsSection[];
} | undefined {
	const group = getGroupByShortName(versionName, locale, groupShortName);

	if (group === undefined) {
		return undefined;
	}

	const sections = getLocalizedSections(group, locale);
	const items = sections.flatMap(section => section.items);
	const firstItem = items[0];

	if (firstItem === undefined) {
		return undefined;
	}

	if (itemShortName === undefined) {
		const resolvedEntry = resolveEntryItem(firstItem);

		return {
			group,
			item: resolvedEntry.item,
			parentItem: resolvedEntry.parentItem,
			sections,
		};
	}

	for (const entry of items) {
		if (entry.shortName === itemShortName) {
			const resolvedEntry = resolveEntryItem(entry, subItemShortName);

			return {
				group,
				item: resolvedEntry.item,
				parentItem: resolvedEntry.parentItem,
				sections,
			};
		}

		const subItem = entry.subItems?.find(child => child.shortName === itemShortName || child.shortName === subItemShortName);

		if (subItem !== undefined) {
			return {
				group,
				item: subItem,
				parentItem: entry,
				sections,
			};
		}
	}

	const resolvedEntry = resolveEntryItem(firstItem);

	return {
		group,
		item: resolvedEntry.item,
		parentItem: resolvedEntry.parentItem,
		sections,
	};
}

export function buildDocsPath(versionName: string, groupShortName: string, itemShortName: string, subItemShortName?: string): string {
	return subItemShortName === undefined
		? `/${versionName}/${groupShortName}/${itemShortName}`
		: `/${versionName}/${groupShortName}/${itemShortName}/${subItemShortName}`;
}

export function getFirstDocsPath(versionName: string, locale: DocsLocale): string {
	const resolved = getItemByShortName(versionName, locale);

	if (resolved === undefined) {
		return `/${getDefaultVersionName()}`;
	}

	return buildDocsPath(versionName, resolved.group.shortName, resolved.item.shortName);
}

function flattenSectionItems(versionName: string, groupShortName: string, sections: DocsSection[]): DocsNavigationItem[] {
	return sections.flatMap(section =>
		section.items.flatMap(item => {
			if (item.subItems === undefined || item.subItems.length === 0) {
				return [
					{
						item,
						path: buildDocsPath(versionName, groupShortName, item.shortName),
					},
				];
			}

			return item.subItems.map(subItem => ({
				item: subItem,
				parentItem: item,
				path: buildDocsPath(versionName, groupShortName, item.shortName, subItem.shortName),
			}));
		}),
	);
}

export function getAdjacentDocsItems(params: {
	currentItemShortName: string;
	groupShortName: string;
	sections: DocsSection[];
	versionName: string;
}): {
	next?: DocsNavigationItem;
	previous?: DocsNavigationItem;
} {
	const flattenedItems = flattenSectionItems(params.versionName, params.groupShortName, params.sections);
	const currentIndex = flattenedItems.findIndex(entry => entry.item.shortName === params.currentItemShortName);

	if (currentIndex === -1) {
		return {};
	}

	return {
		previous: flattenedItems[currentIndex - 1],
		next: flattenedItems[currentIndex + 1],
	};
}

export function getLatestVersionPath(params: {
	groupShortName?: string;
	itemShortName?: string;
	locale: DocsLocale;
	subItemShortName?: string;
}): string {
	const latestVersionName = getLatestVersionName();
	const resolved = resolveDocsPath({
		versionName: latestVersionName,
		groupShortName: params.groupShortName,
		itemShortName: params.itemShortName,
		subItemShortName: params.subItemShortName,
		locale: params.locale,
	});

	return resolved?.path ?? getFirstDocsPath(latestVersionName, params.locale);
}

export function resolveDocsPath(params: {
	groupShortName?: string;
	itemShortName?: string;
	locale: DocsLocale;
	subItemShortName?: string;
	versionName?: string;
}): ResolvedDocsPath | undefined {
	const version = getVersionByName(params.versionName ?? getDefaultVersionName()) ?? getVersionByName(getDefaultVersionName());

	if (version === undefined) {
		return undefined;
	}

	const resolvedLocale = fallbackLocale(params.locale, version);
	const resolved = getItemByShortName(version.name, resolvedLocale, params.groupShortName, params.itemShortName, params.subItemShortName);

	if (resolved === undefined) {
		return undefined;
	}

	return {
		group: resolved.group,
		item: resolved.item,
		parentItem: resolved.parentItem,
		path: buildDocsPath(version.name, resolved.group.shortName, resolved.parentItem?.shortName ?? resolved.item.shortName, resolved.parentItem?.shortName === undefined ? undefined : resolved.item.shortName),
		sections: resolved.sections,
		version,
	};
}

export function parseDocsPath(pathname: string): {
	groupShortName?: string;
	itemShortName?: string;
	subItemShortName?: string;
	versionName?: string;
} {
	const segments = pathname.split('/').filter(Boolean);

	return {
		versionName: segments[0],
		groupShortName: segments[1],
		itemShortName: segments[2],
		subItemShortName: segments[3],
	};
}
