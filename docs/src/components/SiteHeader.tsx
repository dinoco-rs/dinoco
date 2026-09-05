'use client';

import React, { startTransition, useEffect, useRef, useState } from 'react';
import clsx from 'clsx';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { FaGithub } from 'react-icons/fa';
import { FiCheck, FiChevronDown, FiMoon, FiSun } from 'react-icons/fi';

import { getIntlMessages } from '../hooks/useIntl';
import { useThemePreference } from '../hooks/useThemePreference';
import { persistDocsLocale } from '../lib/docs-preferences';
import { SUPPORTED_LOCALES, getFirstDocsPath } from '../jsons/versions';

import type { DocsLocale } from '../jsons/versions';
import type { DocsTheme } from '../lib/docs-preferences';

type SiteHeaderProps = {
	initialLocale: DocsLocale;
	initialTheme: DocsTheme;
};

const SiteHeader = ({ initialLocale, initialTheme }: SiteHeaderProps): React.JSX.Element => {
	const router = useRouter();
	const intl = getIntlMessages(initialLocale);
	const { theme, setTheme } = useThemePreference(initialTheme);
	const [localeOpen, setLocaleOpen] = useState(false);
	const controlsRef = useRef<HTMLDivElement>(null);

	useEffect(() => {
		const handlePointerDown = (event: MouseEvent) => {
			if (controlsRef.current?.contains(event.target as Node) === true) {
				return;
			}

			setLocaleOpen(false);
		};

		document.addEventListener('mousedown', handlePointerDown);

		return () => document.removeEventListener('mousedown', handlePointerDown);
	}, []);

	const switchLocale = (nextLocale: DocsLocale) => {
		startTransition(() => {
			persistDocsLocale(nextLocale);
			setLocaleOpen(false);
			router.push(`/${nextLocale}`);
		});
	};

	return (
		<header className="sticky top-0 z-[120] w-full border-b border-light-300 bg-light-50/95 backdrop-blur-sm transition-colors duration-300 dark:border-[#242424] dark:bg-[#050505]/95">
			<div className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-4 sm:px-6 md:px-8" ref={controlsRef}>
				<Link href={`/${initialLocale}`} className="flex items-center gap-3">
					<img src="/logo.png" className="h-8 w-8 object-contain not-dark:invert" alt="Dinoco Logo" />
					<span className="font-bungee text-xl text-slate-900 dark:text-white">Dinoco</span>
				</Link>

				<div className="flex items-center gap-3 sm:gap-4">
					<Link href={getFirstDocsPath(initialLocale)} className="hidden text-sm font-semibold text-slate-600 transition-colors hover:text-dinoco-brand sm:block dark:text-slate-300 dark:hover:text-dinoco-cyan">
						{intl.footer.docs}
					</Link>

					<a href="https://github.com/dinoco-rs/dinoco" target="_blank" rel="noreferrer" className="flex items-center gap-2 text-slate-400 transition-colors hover:text-dinoco-brand not-dark:text-slate-600" title="GitHub">
						<FaGithub size={18} />
						<span className="hidden text-sm md:block">{intl.github}</span>
					</a>

					<div className="h-4 w-px bg-light-300 dark:bg-[#242424]" />

					<div className="relative">
						<button
							type="button"
							onClick={() => setLocaleOpen(previous => !previous)}
							className="flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-1 text-sm text-slate-600 transition-colors hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
						>
							{intl.locales[initialLocale]}
							<FiChevronDown size={14} className={clsx('transition-transform duration-200', localeOpen && 'rotate-180')} />
						</button>

						{localeOpen ? (
							<div className="absolute right-0 z-50 mt-2 flex w-36 flex-col overflow-hidden rounded-lg border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#161616]">
								{SUPPORTED_LOCALES.map(option => (
									<button
										key={option}
										type="button"
										onClick={() => switchLocale(option)}
										className={clsx(
											'flex w-full cursor-pointer items-center justify-between px-4 py-2.5 text-left text-sm transition-colors hover:bg-light-100 dark:hover:bg-[#242424]',
											initialLocale === option ? 'font-bold text-dinoco-brand dark:text-dinoco-cyan' : 'text-slate-600 dark:text-slate-300',
										)}
									>
										{intl.locales[option]}
										{initialLocale === option ? <FiCheck size={14} /> : null}
									</button>
								))}
							</div>
						) : null}
					</div>

					<div className="hidden items-center gap-2 rounded-full border border-light-300 bg-light-100 p-1 dark:border-[#242424] dark:bg-[#161616] md:flex">
						<button
							type="button"
							onClick={() => setTheme('light')}
							aria-label={intl.themeLight}
							title={intl.themeLight}
							className={clsx('cursor-pointer rounded-full p-2 transition-all', theme === 'light' ? 'bg-gray-200 text-orange-500 shadow-sm' : 'text-slate-400 hover:text-slate-600')}
						>
							<FiSun size={14} />
						</button>

						<button
							type="button"
							onClick={() => setTheme('dark')}
							aria-label={intl.themeDark}
							title={intl.themeDark}
							className={clsx('cursor-pointer rounded-full border p-2 transition-all', theme === 'dark' ? 'border-[#242424] bg-[#0c0c0c] text-dinoco-cyan shadow-sm' : 'border-transparent text-slate-400 hover:text-slate-500')}
						>
							<FiMoon size={14} />
						</button>
					</div>

					<div className="flex items-center justify-center md:hidden">
						<button
							type="button"
							onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
							aria-label={theme === 'light' ? intl.themeDark : intl.themeLight}
							title={theme === 'light' ? intl.themeDark : intl.themeLight}
							className="cursor-pointer rounded-full transition-all"
						>
							{theme === 'light' ? <FiMoon size={18} /> : <FiSun size={18} />}
						</button>
					</div>
				</div>
			</div>
		</header>
	);
};

export default SiteHeader;
