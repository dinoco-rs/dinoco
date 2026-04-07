import React, { useEffect, useMemo, useState } from 'react';
import { useRouter } from 'tuono-router';

import DocsSidebar from './DocsSidebar';
import Header from './Header';
import MarkdownContent from './MarkdownContent';
import { useIntl } from '../hooks/useIntl';
import { parseDocsPath, resolveDocsPath } from '../jsons/versions';
import { type DocsConsumer, useDocs } from '../hooks/useDocs';

function toAnchorId(value: string): string {
	return value.toLowerCase().split(' ').join('-');
}

const DocsPage: React.FC = () => {
	const locale = useDocs(state => state.locale);
	const theme = useDocs(state => state.theme);

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
			locale,
		});
	}, [locale, routeParams.groupShortName, routeParams.itemShortName, routeParams.versionName]);

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
		let link = document.getElementById('hljs-theme') as HTMLLinkElement;

		if (!link) {
			link = document.createElement('link');
			link.id = 'hljs-theme';
			link.rel = 'stylesheet';
			document.head.appendChild(link);
		}

		link.href =
			theme === 'dark'
				? 'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/base16/dracula.min.css'
				: 'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github.min.css';
	}, [theme]);

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

					<article className="prose prose-slate max-w-none dark:prose-invert">
						<MarkdownContent component={resolved.item.component} />
					</article>
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
