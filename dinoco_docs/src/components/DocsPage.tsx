import React, { useEffect, useMemo, useState } from 'react';
import { useRouter } from 'tuono-router';

import DocsContentNavigation from './DocsContentNavigation';
import DocsSidebar from './DocsSidebar';
import Header from './Header';
import MarkdownContent from './MarkdownContent';
import OutdatedVersionNotice from './OutdatedVersionNotice';
import { useIntl } from '../hooks/useIntl';
import { type DocsInPageItemData, getAdjacentDocsItems, getLatestVersionName, getLatestVersionPath, isLatestVersion, parseDocsPath, resolveDocsPath } from '../jsons/versions';
import { type DocsConsumer, useDocs } from '../hooks/useDocs';

function toAnchorId(value: string): string {
	return value.toLowerCase().split(' ').join('-');
}

function getInPageItemTitle(item: DocsInPageItemData): string {
	return typeof item === 'string' ? item : item.title;
}

function flattenInPageItems(items: DocsInPageItemData[]): string[] {
	return items.flatMap(item => {
		const title = getInPageItemTitle(item);
		const nestedItems = typeof item === 'string' ? [] : (item.items ?? []);

		return [title, ...flattenInPageItems(nestedItems)];
	});
}

function findActiveInPagePath(items: DocsInPageItemData[], activeAnchorId?: string): string[] {
	if (activeAnchorId === undefined) {
		return [];
	}

	for (const item of items) {
		const title = getInPageItemTitle(item);
		const anchorId = toAnchorId(title);

		if (anchorId === activeAnchorId) {
			return [anchorId];
		}

		if (typeof item === 'string') {
			continue;
		}

		const nestedPath = findActiveInPagePath(item.items ?? [], activeAnchorId);

		if (nestedPath.length > 0) {
			return [anchorId, ...nestedPath];
		}
	}

	return [];
}

function hasActiveInPageItem(item: DocsInPageItemData, activePathIds: Set<string>): boolean {
	const title = getInPageItemTitle(item);
	const anchorId = toAnchorId(title);

	if (activePathIds.has(anchorId)) {
		return true;
	}

	if (typeof item === 'string') {
		return false;
	}

	return (item.items ?? []).some(child => hasActiveInPageItem(child, activePathIds));
}

function renderInPageItems(items: DocsInPageItemData[], activeAnchorId: string | undefined, activePathIds: Set<string>, level = 0): React.JSX.Element[] {
	return items.map(item => {
		const title = getInPageItemTitle(item);
		const anchorId = toAnchorId(title);
		const nestedItems = typeof item === 'string' ? [] : (item.items ?? []);
		const isInActivePath = hasActiveInPageItem(item, activePathIds);
		const isCurrent = activeAnchorId === anchorId;

		return (
			<React.Fragment key={`${level}-${title}`}>
				<a
					href={`#${anchorId}`}
					className={`cursor-pointer border-l -ml-[1px] pl-4 text-sm transition-colors ${
						isCurrent
							? 'border-dinoco-brand font-bold text-dinoco-brand dark:border-dinoco-cyan dark:text-dinoco-cyan'
							: isInActivePath
								? 'border-dinoco-brand/50 font-semibold text-dinoco-brand/80 dark:border-dinoco-cyan/50 dark:text-dinoco-cyan/80'
								: 'border-transparent text-slate-500 hover:border-dinoco-brand hover:text-slate-900 dark:text-slate-400 dark:hover:border-dinoco-cyan dark:hover:text-white'
					}`}
					style={{ paddingLeft: `${1 + level}rem` }}
				>
					{title}
				</a>

				{nestedItems.length > 0 && renderInPageItems(nestedItems, activeAnchorId, activePathIds, level + 1)}
			</React.Fragment>
		);
	});
}

const DocsPage: React.FC = () => {
	const locale = useDocs(state => state.locale);

	const setConsumer = useDocs(state => state.setConsumer);
	const setVersion = useDocs(state => state.setVersion);
	const intl = useIntl();
	const router = useRouter();

	const [isSidebarOpen, setIsSidebarOpen] = useState(false);
	const [activeAnchorId, setActiveAnchorId] = useState<string>();
	const [articleElement, setArticleElement] = useState<HTMLElement | null>(null);

	const routeParams = useMemo(() => parseDocsPath(router.pathname), [router.pathname]);
	const resolved = useMemo(() => {
		return resolveDocsPath({
			versionName: routeParams.versionName,
			groupShortName: routeParams.groupShortName,
			itemShortName: routeParams.itemShortName,
			subItemShortName: routeParams.subItemShortName,
			locale,
		});
	}, [locale, routeParams.groupShortName, routeParams.itemShortName, routeParams.subItemShortName, routeParams.versionName]);
	const navigation = useMemo(() => {
		if (resolved === undefined) {
			return {};
		}

		return getAdjacentDocsItems({
			versionName: resolved.version.name,
			groupShortName: resolved.group.shortName,
			sections: resolved.sections,
			currentItemShortName: resolved.item.shortName,
		});
	}, [resolved]);
	const outdatedNotice = useMemo(() => {
		if (resolved === undefined || isLatestVersion(resolved.version.name)) {
			return undefined;
		}

		return {
			currentVersionName: resolved.version.name,
			latestVersionName: getLatestVersionName(),
			latestPath: getLatestVersionPath({
				groupShortName: routeParams.groupShortName,
				itemShortName: routeParams.itemShortName,
				subItemShortName: routeParams.subItemShortName,
				locale,
			}),
		};
	}, [locale, resolved, routeParams.groupShortName, routeParams.itemShortName, routeParams.subItemShortName]);
	const inPageAnchorIds = useMemo(() => {
		if (resolved === undefined) {
			return [];
		}

		return flattenInPageItems(resolved.item.inPage).map(toAnchorId);
	}, [resolved]);
	const activeInPagePathIds = useMemo(() => {
		return new Set(findActiveInPagePath(resolved?.item.inPage ?? [], activeAnchorId));
	}, [activeAnchorId, resolved]);

	useEffect(() => {
		if (resolved === undefined) {
			return;
		}

		if (resolved.path !== router.pathname) {
			router.replace(resolved.path);
			return;
		}

		setVersion(resolved.version.name);
		setConsumer(resolved.group.shortName as DocsConsumer);
	}, [resolved, router, setConsumer, setVersion]);

	useEffect(() => {
		if (isSidebarOpen) {
			document.body.style.overflow = 'hidden';
		} else {
			document.body.style.overflow = 'unset';
		}
		return () => {
			document.body.style.overflow = 'unset';
		};
	}, [isSidebarOpen]);

	useEffect(() => {
		if (!articleElement || inPageAnchorIds.length === 0) {
			setActiveAnchorId(undefined);
			return;
		}

		const validAnchorIds = new Set(inPageAnchorIds);
		let frameId = 0;

		const syncActiveAnchor = () => {
			const headingElements = Array.from(articleElement.querySelectorAll<HTMLElement>('h1[id], h2[id], h3[id], h4[id], h5[id], h6[id]')).filter(element => validAnchorIds.has(element.id));

			if (headingElements.length === 0) return;

			const isAtBottom = Math.ceil(window.innerHeight + window.scrollY) >= document.documentElement.scrollHeight - 20;

			if (isAtBottom) {
				const lastHeadingId = headingElements[headingElements.length - 1].id;
				setActiveAnchorId(prev => (prev !== lastHeadingId ? lastHeadingId : prev));
				return;
			}

			const viewportOffset = 180;
			const passedHeadings = headingElements.filter(element => element.getBoundingClientRect().top <= viewportOffset);
			const nextActiveAnchorId = passedHeadings[passedHeadings.length - 1]?.id ?? headingElements[0]?.id;

			if (nextActiveAnchorId !== undefined) {
				setActiveAnchorId(prev => (prev !== nextActiveAnchorId ? nextActiveAnchorId : prev));
			}
		};

		const observer = new MutationObserver(() => {
			syncActiveAnchor();
		});
		observer.observe(articleElement, { childList: true, subtree: true });

		if (window.location.hash) {
			setActiveAnchorId(window.location.hash.slice(1));
		}

		syncActiveAnchor();

		const scheduleSync = () => {
			if (frameId !== 0) return;
			frameId = window.requestAnimationFrame(() => {
				frameId = 0;
				syncActiveAnchor();
			});
		};

		const handleHashChange = () => {
			const hash = window.location.hash.slice(1);
			if (hash.length > 0) {
				setActiveAnchorId(hash);
				return;
			}
			syncActiveAnchor();
		};

		window.addEventListener('scroll', scheduleSync, { passive: true });
		window.addEventListener('resize', scheduleSync);
		window.addEventListener('hashchange', handleHashChange);

		return () => {
			if (frameId !== 0) window.cancelAnimationFrame(frameId);
			window.removeEventListener('scroll', scheduleSync);
			window.removeEventListener('resize', scheduleSync);
			window.removeEventListener('hashchange', handleHashChange);
			observer.disconnect();
		};
	}, [articleElement, inPageAnchorIds]);

	if (resolved === undefined) {
		return null;
	}

	return (
		<div className="flex min-h-screen flex-col bg-light-50 font-montserrat transition-colors duration-300 dark:bg-[#050505]">
			<Header onMenuToggle={() => setIsSidebarOpen(true)} />

			<div className="mx-auto flex w-full max-w-[100%] flex-1 items-start px-4 sm:px-6 md:px-8">
				<DocsSidebar
					currentGroup={resolved.group}
					currentItem={resolved.item}
					locale={locale}
					currentVersionName={resolved.version.name}
					groups={resolved.version.groups}
					sections={resolved.sections}
					isOpen={isSidebarOpen}
					onClose={() => setIsSidebarOpen(false)}
				/>

				<main className="min-w-0 flex-1 pt-8 pb-24 lg:px-8 xl:px-12">
					<div className="mb-6 flex items-center gap-2 text-sm font-semibold text-slate-500 dark:text-slate-400">
						<span className="text-dinoco-brand dark:text-dinoco-cyan">{resolved.version.name}</span>
						<span>/</span>
						<span>{resolved.group.name}</span>
						<span>/</span>
						<span className="text-slate-900 dark:text-slate-200">{resolved.item.name}</span>
					</div>

					{outdatedNotice && (
						<OutdatedVersionNotice currentVersionName={outdatedNotice.currentVersionName} latestVersionName={outdatedNotice.latestVersionName} latestPath={outdatedNotice.latestPath} />
					)}

					<article ref={setArticleElement} className="prose prose-slate max-w-none dark:prose-invert">
						<MarkdownContent component={resolved.item.component} />
					</article>

					<DocsContentNavigation previous={navigation.previous} next={navigation.next} />
				</main>

				<aside className="hidden w-64 shrink-0 pt-8 pb-24 xl:block sticky top-24">
					<div className="max-h-[calc(100vh-7rem)] overflow-y-auto">
						<p className="mb-4 text-xs font-bold uppercase tracking-widest text-slate-900 dark:text-white">{intl.inPageLabel || 'Nesta Página'}</p>

						<nav className="flex flex-col space-y-2 border-l border-light-200 dark:border-[#242424]">{renderInPageItems(resolved.item.inPage, activeAnchorId, activeInPagePathIds)}</nav>
					</div>
				</aside>
			</div>

			{/* <DocsFooter /> */}
		</div>
	);
};

export default DocsPage;
