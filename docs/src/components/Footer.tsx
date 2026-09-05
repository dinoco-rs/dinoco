import React from 'react';
import Link from 'next/link';
import { FaGithub, FaHeart } from 'react-icons/fa';

import { getIntlMessages } from '../hooks/useIntl';
import { buildDocsPath, getFirstDocsPath } from '../jsons/versions';

import type { DocsLocale } from '../jsons/versions';

type FooterProps = {
	locale: DocsLocale;
};

const Footer = ({ locale }: FooterProps): React.JSX.Element => {
	const intl = getIntlMessages(locale);
	const docsPath = getFirstDocsPath(locale);

	return (
		<footer className="border-t border-light-300 bg-light-50 dark:border-dark-700 dark:bg-dark-950">
			<div className="mx-auto grid w-full max-w-6xl gap-10 px-4 py-12 sm:px-6 md:grid-cols-[1.3fr_1fr_1fr_1fr] md:px-8">
				<div className="max-w-xs">
					<Link href={`/${locale}`} className="flex items-center gap-3">
						<img src="/logo.png" className="h-7 w-7 object-contain not-dark:invert" alt="Dinoco Logo" />
						<span className="font-bungee text-lg text-slate-900 dark:text-white">Dinoco</span>
					</Link>
					<p className="mt-3 text-sm leading-6 text-slate-500 dark:text-slate-400">{intl.footer.tagline}</p>
				</div>

				<div>
					<p className="mb-3 text-xs font-bold uppercase text-slate-900 dark:text-white">{intl.footer.product}</p>
					<ul className="space-y-2 text-sm text-slate-500 dark:text-slate-400">
						<li>
							<Link href={docsPath} className="transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
								{intl.footer.docs}
							</Link>
						</li>
					</ul>
				</div>

				<div>
					<p className="mb-3 text-xs font-bold uppercase text-slate-900 dark:text-white">{intl.footer.resources}</p>
					<ul className="space-y-2 text-sm text-slate-500 dark:text-slate-400">
						<li>
							<a href="https://marketplace.visualstudio.com/items?itemName=dinoco-rs.dinoco-vscode" target="_blank" rel="noreferrer" className="transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
								{intl.footer.vscode}
							</a>
						</li>
						<li>
							<Link href={buildDocsPath(locale, 'tooling', 'cli')} className="transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
								{intl.footer.cli}
							</Link>
						</li>
					</ul>
				</div>

				<div>
					<p className="mb-3 text-xs font-bold uppercase text-slate-900 dark:text-white">{intl.footer.community}</p>
					<ul className="space-y-2 text-sm text-slate-500 dark:text-slate-400">
						<li>
							<a href="https://github.com/dinoco-rs/dinoco" target="_blank" rel="noreferrer" className="flex items-center gap-2 transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
								<FaGithub size={14} /> {intl.footer.github}
							</a>
						</li>
						<li>
							<a href="https://github.com/dinoco-rs/dinoco/issues/new" target="_blank" rel="noreferrer" className="transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
								{intl.footer.issues}
							</a>
						</li>
						<li>
							<a href="https://buymeacoffee.com/theuszastro" target="_blank" rel="noreferrer" className="flex items-center gap-2 transition-colors hover:text-red-500">
								<FaHeart size={14} /> {intl.footer.support}
							</a>
						</li>
					</ul>
				</div>
			</div>

			<div className="border-t border-light-200 dark:border-dark-800">
				<div className="mx-auto flex w-full max-w-6xl flex-col items-center justify-between gap-2 px-4 py-5 text-xs text-slate-400 sm:flex-row sm:px-6 md:px-8 dark:text-slate-500">
					<span>Dinoco</span>
					<a href="https://github.com/dinoco-rs/dinoco/blob/main/LICENSE" target="_blank" rel="noreferrer" className="transition-colors hover:text-dinoco-brand dark:hover:text-dinoco-cyan">
						{intl.footer.license}
					</a>
				</div>
			</div>
		</footer>
	);
};

export default Footer;
