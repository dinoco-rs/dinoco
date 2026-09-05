import React from 'react';
import Link from 'next/link';
import { FaGithub } from 'react-icons/fa';
import { FiArrowRight, FiClock, FiDatabase } from 'react-icons/fi';

import Footer from './Footer';
import SiteHeader from './SiteHeader';
import { getIntlMessages } from '../hooks/useIntl';
import { buildDocsPath, getFirstDocsPath } from '../jsons/versions';

import type { DocsLocale } from '../jsons/versions';
import type { DocsTheme } from '../lib/docs-preferences';

type HomePageProps = {
	locale: DocsLocale;
	theme: DocsTheme;
};

const HomePage = ({ locale, theme }: HomePageProps): React.JSX.Element => {
	const intl = getIntlMessages(locale);
	const docsPath = getFirstDocsPath(locale);
	const quickstartPath = buildDocsPath(locale, 'guide', 'quickstart');

	return (
		<div className="flex min-h-screen flex-col bg-light-50 font-montserrat transition-colors duration-300 dark:bg-dark-950">
			<SiteHeader initialLocale={locale} initialTheme={theme} />

			<main className="flex-1">
				<section className="mx-auto w-full max-w-5xl px-4 pb-16 pt-20 text-center sm:px-6 sm:pt-28 md:px-8">
					<h1 className="font-bungee text-4xl text-slate-900 sm:text-6xl dark:text-white">{intl.home.heroTitle}</h1>
					<p className="mx-auto mt-6 max-w-2xl text-lg leading-8 text-slate-600 dark:text-slate-300">{intl.home.heroSubtitle}</p>

					<div className="mt-10 flex flex-wrap items-center justify-center gap-3">
						<Link
							href={quickstartPath}
							className="flex items-center gap-2 rounded-md bg-dinoco-brand px-5 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-dinoco-deep dark:bg-dinoco-cyan dark:text-dark-950 dark:hover:bg-dinoco-cyan/80"
						>
							{intl.home.getStarted}
							<FiArrowRight size={16} />
						</Link>

						<Link
							href={docsPath}
							className="rounded-md border border-light-300 px-5 py-2.5 text-sm font-semibold text-slate-700 transition-colors hover:border-dinoco-brand hover:text-dinoco-brand dark:border-dark-700 dark:text-slate-200 dark:hover:border-dinoco-cyan dark:hover:text-dinoco-cyan"
						>
							{intl.home.viewDocs}
						</Link>

						<a
							href="https://github.com/dinoco-rs/dinoco"
							target="_blank"
							rel="noreferrer"
							className="flex items-center gap-2 rounded-md border border-light-300 px-5 py-2.5 text-sm font-semibold text-slate-700 transition-colors hover:border-dinoco-brand hover:text-dinoco-brand dark:border-dark-700 dark:text-slate-200 dark:hover:border-dinoco-cyan dark:hover:text-dinoco-cyan"
						>
							<FaGithub size={16} />
							{intl.home.viewGithub}
						</a>
					</div>
				</section>

				<section className="mx-auto w-full max-w-5xl px-4 py-12 sm:px-6 md:px-8">
					<div className="mb-8 text-center">
						<h2 className="text-2xl font-bold text-slate-900 dark:text-white">{intl.home.projectsTitle}</h2>
						<p className="mt-2 text-slate-500 dark:text-slate-400">{intl.home.projectsSubtitle}</p>
					</div>

					<div className="grid gap-4 sm:grid-cols-3">
						<Link
							href={docsPath}
							className="group flex flex-col justify-between rounded-lg border border-light-200 bg-light-100 p-6 transition-all duration-200 hover:border-dinoco-brand hover:bg-light-50 hover:shadow-sm dark:border-dark-700 dark:bg-dark-900 dark:hover:border-dinoco-cyan dark:hover:bg-dark-800"
						>
							<div>
								<div className="mb-4 flex h-10 w-10 items-center justify-center rounded-md bg-dinoco-brand/10 text-dinoco-brand dark:bg-dinoco-cyan/10 dark:text-dinoco-cyan">
									<FiDatabase size={20} />
								</div>
								<h3 className="text-lg font-semibold text-slate-900 dark:text-white">{intl.home.ormName}</h3>
								<p className="mt-2 text-sm leading-6 text-slate-500 dark:text-slate-400">{intl.home.ormDescription}</p>
							</div>

							<div className="mt-6 flex items-center justify-between">
								<span className="inline-flex items-center gap-1.5 text-xs font-bold uppercase text-emerald-600 dark:text-emerald-400">
									<span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
									{intl.home.ormStatus}
								</span>
								<span className="flex items-center gap-1 text-sm font-semibold text-dinoco-brand transition-transform group-hover:translate-x-1 dark:text-dinoco-cyan">
									{intl.home.ormCta}
									<FiArrowRight size={14} />
								</span>
							</div>
						</Link>

						<div className="flex flex-col justify-between rounded-lg border border-dashed border-light-300 bg-light-100/50 p-6 dark:border-dark-700 dark:bg-dark-900/50">
							<div>
								<div className="mb-4 flex h-10 w-10 items-center justify-center rounded-md bg-slate-400/10 text-slate-400 dark:bg-slate-500/10 dark:text-slate-500">
									<FiClock size={20} />
								</div>
								<h3 className="text-lg font-semibold text-slate-500 dark:text-slate-400">{intl.home.supersonicName}</h3>
								<p className="mt-2 text-sm leading-6 text-slate-400 dark:text-slate-500">{intl.home.supersonicDescription}</p>
							</div>

							<span className="mt-6 inline-flex w-fit items-center gap-1.5 rounded-full bg-slate-400/10 px-2.5 py-1 text-xs font-bold uppercase text-slate-500 dark:bg-slate-500/10 dark:text-slate-400">{intl.home.comingSoonBadge}</span>
						</div>

						<div className="flex flex-col justify-between rounded-lg border border-dashed border-light-300 bg-light-100/50 p-6 dark:border-dark-700 dark:bg-dark-900/50">
							<div>
								<div className="mb-4 flex h-10 w-10 items-center justify-center rounded-md bg-slate-400/10 text-slate-400 dark:bg-slate-500/10 dark:text-slate-500">
									<FiClock size={20} />
								</div>
								<h3 className="text-lg font-semibold text-slate-500 dark:text-slate-400">{intl.home.dinocoDbName}</h3>
								<p className="mt-2 text-sm leading-6 text-slate-400 dark:text-slate-500">{intl.home.dinocoDbDescription}</p>
							</div>

							<span className="mt-6 inline-flex w-fit items-center gap-1.5 rounded-full bg-slate-400/10 px-2.5 py-1 text-xs font-bold uppercase text-slate-500 dark:bg-slate-500/10 dark:text-slate-400">{intl.home.comingSoonBadge}</span>
						</div>
					</div>
				</section>

				<section className="mx-auto w-full max-w-5xl px-4 py-16 sm:px-6 md:px-8">
					<div className="rounded-lg border border-light-200 bg-light-100 px-6 py-10 text-center dark:border-dark-700 dark:bg-dark-900 sm:px-12">
						<h2 className="text-2xl font-bold text-slate-900 dark:text-white">{intl.home.openSourceTitle}</h2>
						<p className="mx-auto mt-3 max-w-2xl text-slate-600 dark:text-slate-300">{intl.home.openSourceBody}</p>

						<a
							href="https://github.com/dinoco-rs/dinoco"
							target="_blank"
							rel="noreferrer"
							className="mt-6 inline-flex items-center gap-2 rounded-md bg-dinoco-brand px-5 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-dinoco-deep dark:bg-dinoco-cyan dark:text-dark-950 dark:hover:bg-dinoco-cyan/80"
						>
							<FaGithub size={16} />
							{intl.home.openSourceCta}
						</a>
					</div>
				</section>
			</main>

			<Footer locale={locale} />
		</div>
	);
};

export default HomePage;
