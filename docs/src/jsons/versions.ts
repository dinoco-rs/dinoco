import v2_0_1 from './versions/v2.0.1';

export type DocsLocale = 'en-us' | 'pt-br';
export const SUPPORTED_LOCALES: DocsLocale[] = ['en-us', 'pt-br'];
type RawDocsLocale = DocsLocale | 'pt';

type DocsItemData = {
	description: string;
	inPage: DocsInPageItemData[];
	contentPath: string;
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
	languages: Partial<Record<RawDocsLocale, DocsSectionData[]>>;
	localizedNames?: Partial<Record<RawDocsLocale, string>>;
	name: string;
	shortName: string;
	status?: 'comingSoon';
};

type DocsVersionData = {
	description: Partial<Record<RawDocsLocale, string>>;
	groups: DocsGroupData[];
	name: string;
};

export type DocsItem = Omit<DocsItemData, 'subItems'> & {
	documentTitle: string;
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

export type DocsGroup = Omit<DocsGroupData, 'languages' | 'localizedNames'> & {
	languages: Partial<Record<DocsLocale, DocsSection[]>>;
	localizedNames: Partial<Record<DocsLocale, string>>;
};

export type DocsVersion = Omit<DocsVersionData, 'groups'> & {
	groups: DocsGroup[];
};

function mapItem(item: DocsItemData): DocsItem {
	return {
		...item,
		documentTitle: item.name,
		subItems: item.subItems?.map(mapItem),
	};
}

function normalizeLocaleKey(locale: string): DocsLocale | undefined {
	if (locale === 'pt') {
		return 'pt-br';
	}

	return SUPPORTED_LOCALES.includes(locale as DocsLocale) ? (locale as DocsLocale) : undefined;
}

function normalizeLocalizedRecord<T>(record: Partial<Record<RawDocsLocale, T>>): Partial<Record<DocsLocale, T>> {
	return Object.fromEntries(
		Object.entries(record).flatMap(([locale, value]) => {
			const normalized = normalizeLocaleKey(locale);

			if (normalized === undefined || value === undefined) {
				return [];
			}

			return [[normalized, value] as const];
		}),
	) as Partial<Record<DocsLocale, T>>;
}

const versionsData: DocsVersionData[] = [v2_0_1 as DocsVersionData];

export const versions: DocsVersion[] = versionsData.map(version => ({
	...version,
	description: normalizeLocalizedRecord(version.description),
	groups: version.groups.map(group => ({
		...group,
		localizedNames: normalizeLocalizedRecord(group.localizedNames ?? {}),
		languages: Object.fromEntries(
			Object.entries(normalizeLocalizedRecord(group.languages)).map(([locale, sections]) => [
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

	return localeSet[0] ?? 'en-us';
}

export function getLatestVersionName(): string {
	return versions[0]?.name ?? 'v2.0.1';
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
	const version = getVersionByName(versionName);

	if (version === undefined) {
		return ['en-us'];
	}

	return SUPPORTED_LOCALES.filter(locale =>
		version.groups.some(group => {
			const localizedSections = group.languages[locale];

			return localizedSections !== undefined && localizedSections.length > 0;
		}),
	);
}

export function getLocalizedSections(group: DocsGroup, locale: DocsLocale): DocsSection[] {
	return group.languages[locale] ?? group.languages['en-us'] ?? [];
}

export function getLocalizedGroupName(group: DocsGroup, locale: DocsLocale): string {
	return group.localizedNames[locale] ?? group.localizedNames['en-us'] ?? group.name;
}

/**
 * Every real (locale, group, item, subItem) combination in the current
 * version. Shared by the sitemap and by the docs route's
 * `generateStaticParams` so both stay in sync with the actual nav data.
 */
export function getAllDocsSlugs(): { locale: DocsLocale; slug: string[] }[] {
	const versionName = getDefaultVersionName();
	const entries: { locale: DocsLocale; slug: string[] }[] = [];

	for (const locale of SUPPORTED_LOCALES) {
		for (const group of getGroupsForVersion(versionName, locale)) {
			for (const section of getLocalizedSections(group, locale)) {
				for (const item of section.items) {
					if (item.subItems === undefined || item.subItems.length === 0) {
						entries.push({ locale, slug: [group.shortName, item.shortName] });
						continue;
					}

					for (const subItem of item.subItems) {
						entries.push({ locale, slug: [group.shortName, item.shortName, subItem.shortName] });
					}
				}
			}
		}
	}

	return entries;
}

export function getGroupsForVersion(versionName: string, locale: DocsLocale): DocsGroup[] {
	const version = getVersionByName(versionName);

	if (version === undefined) {
		return [];
	}

	const resolvedLocale = fallbackLocale(locale, version);

	return version.groups.filter(group => {
		const localizedSections = group.languages[resolvedLocale] ?? group.languages['en-us'];

		return localizedSections !== undefined && localizedSections.length > 0;
	});
}

export function getGroupByShortName(versionName: string, locale: DocsLocale, groupShortName?: string): DocsGroup | undefined {
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
):
	| {
			group: DocsGroup;
			item: DocsItem;
			parentItem?: DocsItem;
			sections: DocsSection[];
	  }
	| undefined {
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

/**
 * Public doc URLs never encode the version (only one version exists today,
 * and the version dataset above stays purely internal). The URL shape is
 * `/{locale}/docs/orm/{group}/{item}/{subItem?}`.
 */
export function buildDocsPath(locale: DocsLocale, groupShortName: string, itemShortName: string, subItemShortName?: string): string {
	const base = `/${locale}/docs/orm/${groupShortName}/${itemShortName}`;

	return subItemShortName === undefined ? base : `${base}/${subItemShortName}`;
}

export function getFirstDocsPath(locale: DocsLocale): string {
	const versionName = getDefaultVersionName();
	const resolved = getItemByShortName(versionName, locale);

	if (resolved === undefined) {
		return `/${locale}/docs/orm`;
	}

	const resolvedEntry = resolveEntryItem(resolved.item);

	return buildDocsPath(locale, resolved.group.shortName, resolvedEntry.item.shortName);
}

function flattenSectionItems(locale: DocsLocale, groupShortName: string, sections: DocsSection[]): DocsNavigationItem[] {
	return sections.flatMap(section =>
		section.items.flatMap(item => {
			if (item.subItems === undefined || item.subItems.length === 0) {
				return [
					{
						item,
						path: buildDocsPath(locale, groupShortName, item.shortName),
					},
				];
			}

			return item.subItems.map(subItem => ({
				item: subItem,
				parentItem: item,
				path: buildDocsPath(locale, groupShortName, item.shortName, subItem.shortName),
			}));
		}),
	);
}

export function getAdjacentDocsItems(params: { currentItemShortName: string; groupShortName: string; locale: DocsLocale; sections: DocsSection[] }): {
	next?: DocsNavigationItem;
	previous?: DocsNavigationItem;
} {
	const flattenedItems = flattenSectionItems(params.locale, params.groupShortName, params.sections);
	const currentIndex = flattenedItems.findIndex(entry => entry.item.shortName === params.currentItemShortName);

	if (currentIndex === -1) {
		return {};
	}

	return {
		previous: flattenedItems[currentIndex - 1],
		next: flattenedItems[currentIndex + 1],
	};
}

export function resolveDocsPath(params: { groupShortName?: string; itemShortName?: string; locale: DocsLocale; subItemShortName?: string }): ResolvedDocsPath | undefined {
	const version = getVersionByName(getDefaultVersionName());

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
		path: buildDocsPath(
			resolvedLocale,
			resolved.group.shortName,
			resolved.parentItem?.shortName ?? resolved.item.shortName,
			resolved.parentItem?.shortName === undefined ? undefined : resolved.item.shortName,
		),
		sections: resolved.sections,
		version,
	};
}

export function parseDocsPath(slug?: string[]): {
	groupShortName?: string;
	itemShortName?: string;
	subItemShortName?: string;
} {
	const segments = (slug ?? []).filter(Boolean);

	return {
		groupShortName: segments[0],
		itemShortName: segments[1],
		subItemShortName: segments[2],
	};
}
