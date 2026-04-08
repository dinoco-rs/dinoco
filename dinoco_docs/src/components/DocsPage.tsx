import React, { useEffect, useMemo, useState } from 'react';
import { useRouter } from 'tuono-router';

import DocsContentNavigation from './DocsContentNavigation';
import DocsSidebar from './DocsSidebar';
import Header from './Header';
import MarkdownContent from './MarkdownContent';
import OutdatedVersionNotice from './OutdatedVersionNotice';
import { useIntl } from '../hooks/useIntl';
import { getAdjacentDocsItems, getLatestVersionName, getLatestVersionPath, isLatestVersion, parseDocsPath, resolveDocsPath } from '../jsons/versions';
import { type DocsConsumer, useDocs } from '../hooks/useDocs';

function toAnchorId(value: string): string {
	return value.toLowerCase().split(' ').join('-');
}

const DocsPage: React.FC = () => {
	const locale = useDocs(state => state.locale);

	const setConsumer = useDocs(state => state.setConsumer);
	const setVersion = useDocs(state => state.setVersion);
	const intl = useIntl();
	const router = useRouter();

	const [isSidebarOpen, setIsSidebarOpen] = useState(false);

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

					<article className="prose prose-slate max-w-none dark:prose-invert">
						<MarkdownContent component={resolved.item.component} />
					</article>

					<DocsContentNavigation previous={navigation.previous} next={navigation.next} />
				</main>

				<aside className="hidden w-64 shrink-0 pt-8 pb-24 xl:block">
					<div className="sticky top-32">
						<p className="mb-4 text-xs font-bold uppercase tracking-widest text-slate-900 dark:text-white">{intl.inPageLabel || 'Nesta Página'}</p>

						<nav className="flex flex-col space-y-2 border-l border-light-200 dark:border-[#242424]">
							{resolved.item.inPage.map(topic => (
								<a
									key={topic}
									href={`#${toAnchorId(topic)}`}
									className="cursor-pointer border-l -ml-[1px] border-transparent pl-4 text-sm text-slate-500 transition-colors hover:border-dinoco-brand hover:text-slate-900 dark:text-slate-400 dark:hover:border-dinoco-cyan dark:hover:text-white"
								>
									{topic}
								</a>
							))}
						</nav>
					</div>
				</aside>
			</div>

			{/* <DocsFooter /> */}
		</div>
	);
};

export default DocsPage;
