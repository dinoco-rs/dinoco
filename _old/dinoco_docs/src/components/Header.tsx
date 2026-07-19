'use client';

import React, { startTransition, useEffect, useMemo, useRef, useState } from 'react';
import clsx from 'clsx';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { FaGithub, FaHeart } from 'react-icons/fa';
import { FiBox, FiCheck, FiChevronDown, FiDatabase, FiGlobe, FiLayers, FiMenu, FiMoon, FiSun, FiTerminal } from 'react-icons/fi';

import { getIntlMessages } from '../hooks/useIntl';
import { useThemePreference } from '../hooks/useThemePreference';
import { persistDocsLocale } from '../lib/docs-preferences';
import { buildDocsPath, getAvailableLocales, getFirstDocsPath, getGroupsForVersion, getVersionNames, parseDocsPath, resolveDocsPath } from '../jsons/versions';

import type { DropdownButtonProps, DropdownItemProps, HeaderProps } from '../types';
import type { DocsLocale, ResolvedDocsPath } from '../jsons/versions';
import type { DocsTheme } from '../lib/docs-preferences';

type HeaderComponentProps = HeaderProps & {
	initialLocale: DocsLocale;
	initialTheme: DocsTheme;
	pathname: string;
	resolved: ResolvedDocsPath;
};

const iconMap = {
	box: FiBox,
	database: FiDatabase,
	rocket: FiLayers,
	terminal: FiTerminal,
} as const;

function DropdownButton({ isOpen, children, onClick, className }: DropdownButtonProps): React.JSX.Element {
	return (
		<button type="button" onClick={onClick} className={clsx('flex cursor-pointer items-center gap-2 rounded-md py-1.5 transition-colors', className)}>
			{children}
			<FiChevronDown size={14} className={clsx('text-slate-400 transition-transform duration-200', isOpen && 'rotate-180')} />
		</button>
	);
}

function DropdownItem({ isActive, children, onClick }: DropdownItemProps): React.JSX.Element {
	return (
		<button
			type="button"
			onClick={onClick}
			className={clsx(
				'flex w-full cursor-pointer items-center justify-between px-4 py-2.5 text-left text-sm transition-colors hover:bg-light-100 dark:hover:bg-[#242424]',
				isActive ? 'font-bold text-dinoco-brand dark:text-dinoco-cyan' : 'text-slate-600 dark:text-slate-300',
			)}
		>
			{children}
		</button>
	);
}

const Header = ({ initialLocale, initialTheme, pathname, resolved, onMenuToggle }: HeaderComponentProps): React.JSX.Element => {
	const router = useRouter();
	const intl = getIntlMessages(initialLocale);
	const { theme, setTheme } = useThemePreference(initialTheme);
	const [localeOpen, setLocaleOpen] = useState(false);
	const [versionOpen, setVersionOpen] = useState(false);
	const [mobileConsumerOpen, setMobileConsumerOpen] = useState(false);
	const controlsRef = useRef<HTMLDivElement>(null);

	const versionOptions = useMemo(() => getVersionNames(), []);
	const routeParams = useMemo(() => parseDocsPath(pathname), [pathname]);
	const displayedVersion = resolved.version.name;
	const displayedConsumer = resolved.group.shortName;
	const localeOptions = useMemo(() => getAvailableLocales(displayedVersion), [displayedVersion]);
	const consumerOptions = useMemo(() => getGroupsForVersion(displayedVersion, initialLocale), [displayedVersion, initialLocale]);

	useEffect(() => {
		const handlePointerDown = (event: MouseEvent) => {
			if (controlsRef.current?.contains(event.target as Node) === true) {
				return;
			}

			setLocaleOpen(false);
			setVersionOpen(false);
			setMobileConsumerOpen(false);
		};

		document.addEventListener('mousedown', handlePointerDown);

		return () => document.removeEventListener('mousedown', handlePointerDown);
	}, []);

	const closeOtherMenus = (menu: 'locale' | 'version' | 'mobileConsumer') => {
		setLocaleOpen(menu === 'locale' ? !localeOpen : false);
		setVersionOpen(menu === 'version' ? !versionOpen : false);
		setMobileConsumerOpen(menu === 'mobileConsumer' ? !mobileConsumerOpen : false);
	};

	const navigateToResolvedPath = (nextVersion: string, nextLocale: DocsLocale, nextConsumer: string) => {
		const nextResolved = resolveDocsPath({
			versionName: nextVersion,
			groupShortName: nextConsumer,
			itemShortName: routeParams.itemShortName,
			subItemShortName: routeParams.subItemShortName,
			locale: nextLocale,
		});
		const nextPath =
			nextResolved === undefined
				? getFirstDocsPath(nextVersion, nextLocale)
				: buildDocsPath(
						nextResolved.version.name,
						nextResolved.group.shortName,
						nextResolved.parentItem?.shortName ?? nextResolved.item.shortName,
						nextResolved.parentItem?.shortName === undefined ? undefined : nextResolved.item.shortName,
					);

		startTransition(() => {
			persistDocsLocale(nextLocale);
			setLocaleOpen(false);
			setVersionOpen(false);
			setMobileConsumerOpen(false);

			if (pathname !== nextPath) {
				router.replace(nextPath);
				return;
			}

			router.refresh();
		});
	};

	const currentConsumerObj = consumerOptions.find(option => option.shortName === displayedConsumer) ?? resolved.group;
	const currentConsumerLabel = currentConsumerObj.name.charAt(0).toUpperCase() + currentConsumerObj.name.slice(1);
	const CurrentConsumerIcon = iconMap[currentConsumerObj.icon as keyof typeof iconMap] ?? FiTerminal;

	function renderVersionAndLocale(md = true) {
		return (
			<div className={clsx(md ? 'hidden items-center gap-2 md:flex' : 'mb-3 grid w-full grid-cols-2 gap-3 md:hidden')}>
				<div className="relative w-full">
					<DropdownButton
						isOpen={localeOpen}
						onClick={() => closeOtherMenus('locale')}
						className={clsx(md ? 'h-8 shrink-1 px-0 py-1' : 'h-8 w-full justify-between rounded-md border border-light-300 bg-transparent px-3 hover:bg-light-100 dark:border-[#242424] dark:hover:bg-[#161616]')}
					>
						<div className="flex items-center gap-1.5">
							{!md ? <FiGlobe size={14} className="text-slate-500 dark:text-slate-400" /> : null}
							<span className={clsx('text-sm', !md && 'font-medium text-slate-700 dark:text-slate-300')}>{intl.locales[initialLocale]}</span>
						</div>
					</DropdownButton>

					{localeOpen ? (
						<div
							className={clsx(
								'absolute z-50 flex flex-col overflow-hidden rounded-lg border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#161616]',
								md ? 'right-0 mt-3 w-36' : 'left-0 mt-1 w-full',
							)}
						>
							{localeOptions.map(option => (
								<DropdownItem
									key={option}
									isActive={initialLocale === option}
									onClick={() => {
										navigateToResolvedPath(displayedVersion, option, displayedConsumer);
									}}
								>
									{intl.locales[option]}
									{initialLocale === option ? <FiCheck size={14} /> : null}
								</DropdownItem>
							))}
						</div>
					) : null}
				</div>

				<div className="relative w-full">
					<DropdownButton
						isOpen={versionOpen}
						onClick={() => closeOtherMenus('version')}
						className={clsx(md ? 'h-8 px-2.5 py-1' : 'h-8 w-full justify-between rounded-md border border-light-300 bg-transparent px-3 hover:bg-light-100 dark:border-[#242424] dark:hover:bg-[#161616]')}
					>
						<span className="text-sm font-semibold text-dinoco-deep dark:text-dinoco-cyan">{displayedVersion}</span>
					</DropdownButton>

					{versionOpen ? (
						<div
							className={clsx(
								'absolute z-50 flex flex-col overflow-hidden rounded-lg border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#161616]',
								md ? 'right-0 mt-2 w-32' : 'left-0 mt-1 w-full',
							)}
						>
							{versionOptions.map(option => (
								<DropdownItem
									key={option}
									isActive={displayedVersion === option}
									onClick={() => {
										navigateToResolvedPath(option, initialLocale, displayedConsumer);
									}}
								>
									{option}
									{displayedVersion === option ? <FiCheck size={14} /> : null}
								</DropdownItem>
							))}
						</div>
					) : null}
				</div>
			</div>
		);
	}

	return (
		<header className="top-0 z-[120] block w-full border-b border-light-300 bg-light-50/95 backdrop-blur-sm transition-colors duration-300 dark:border-[#242424] dark:bg-[#050505]/95 md:sticky">
			<div className="flex w-full flex-col px-4 sm:px-6 md:px-8" ref={controlsRef}>
				<div className="mt-4 mb-2 flex items-center justify-between sm:mb-0 md:h-10">
					<div className="mb-2 flex items-center gap-2 md:mb-0 md:gap-4">
						<button type="button" onClick={onMenuToggle} className="cursor-pointer rounded-md p-2 text-slate-500 hover:bg-light-200 dark:text-slate-400 dark:hover:bg-[#161616] lg:hidden">
							<FiMenu size={20} />
						</button>

						<Link href="/" className="flex items-center gap-3">
							<img src="/logo.png" className="h-8 w-8 object-contain not-dark:invert" alt="Dinoco Logo" />
							<div className="flex items-center gap-1.5 text-xl">
								<span className="font-bungee text-slate-900 dark:text-white">Dinoco</span>
							</div>
						</Link>
					</div>

					<div className="flex items-center gap-3 sm:gap-4">
						{renderVersionAndLocale(true)}

						<a href="https://github.com/dinoco-rs/dinoco" target="_blank" rel="noreferrer" className="flex items-center gap-2 text-slate-400 transition-colors hover:text-dinoco-brand not-dark:text-slate-600" title="GitHub">
							<FaGithub size={18} />
							<span className="hidden text-sm md:block">{intl.github}</span>
						</a>

						<a href="https://buymeacoffee.com/theuszastro" target="_blank" rel="noreferrer" className="flex items-center gap-2 text-slate-400 transition-colors hover:text-red-500 not-dark:text-slate-600" title="Donate">
							<FaHeart size={18} />
							<span className="hidden text-sm md:block">{intl.donate}</span>
						</a>

						<div className="h-4 w-px bg-light-300 dark:bg-[#242424]" />

						<div className="hidden items-center gap-2 rounded-full border border-light-300 bg-light-100 p-1 dark:border-[#242424] dark:bg-[#161616] md:flex">
							<button
								type="button"
								onClick={() => setTheme('light')}
								className={clsx('cursor-pointer rounded-full p-2 transition-all', theme === 'light' ? 'bg-gray-200 text-orange-500 shadow-sm' : 'text-slate-400 hover:text-slate-600')}
							>
								<FiSun size={14} />
							</button>

							<button
								type="button"
								onClick={() => setTheme('dark')}
								className={clsx('cursor-pointer rounded-full border p-2 transition-all', theme === 'dark' ? 'border-[#242424] bg-[#0c0c0c] text-dinoco-cyan shadow-sm' : 'border-transparent text-slate-400 hover:text-slate-500')}
							>
								<FiMoon size={14} />
							</button>
						</div>

						<div className="flex items-center justify-center md:hidden">
							<button type="button" onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')} className="cursor-pointer rounded-full transition-all">
								{theme === 'light' ? <FiMoon size={18} /> : <FiSun size={18} />}
							</button>
						</div>
					</div>
				</div>

				<div className="flex items-center gap-3 sm:gap-4">{renderVersionAndLocale(false)}</div>

				<nav className="hidden gap-4 overflow-x-auto no-scrollbar sm:flex">
					{consumerOptions.map(option => {
						const isActive = displayedConsumer === option.shortName;
						const Icon = iconMap[option.icon as keyof typeof iconMap] ?? FiTerminal;

						return (
							<button
								type="button"
								key={option.shortName}
								onClick={() => navigateToResolvedPath(displayedVersion, initialLocale, option.shortName)}
								className={clsx(
									'flex cursor-pointer items-center gap-2 whitespace-nowrap border-b-2 py-3 text-sm font-semibold transition-colors',
									isActive
										? 'border-dinoco-brand text-dinoco-brand dark:border-dinoco-cyan dark:text-dinoco-cyan'
										: 'border-transparent text-slate-500 hover:border-slate-300 hover:text-slate-800 dark:text-slate-400 dark:hover:border-slate-600 dark:hover:text-slate-200',
								)}
							>
								<Icon />
								{option.name.charAt(0).toUpperCase() + option.name.slice(1)}
							</button>
						);
					})}
				</nav>

				<div className="relative mb-3 block w-full sm:hidden">
					<button
						type="button"
						onClick={() => closeOtherMenus('mobileConsumer')}
						className="flex w-full cursor-pointer items-center justify-between rounded-md border border-light-300 bg-light-100 px-4 py-2.5 text-sm font-semibold transition-colors dark:border-[#242424] dark:bg-[#161616] dark:text-white"
					>
						<div className="flex items-center gap-2 text-slate-300">
							<CurrentConsumerIcon />
							<span>{currentConsumerLabel}</span>
						</div>

						<FiChevronDown size={16} className={clsx('text-slate-300 transition-transform duration-200', mobileConsumerOpen && 'rotate-180')} />
					</button>

					{mobileConsumerOpen ? (
						<div className="absolute left-0 top-full z-50 mt-1 w-full overflow-hidden rounded-md border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#0c0c0c]">
							{consumerOptions.map(option => {
								const Icon = iconMap[option.icon as keyof typeof iconMap] ?? FiTerminal;

								return (
									<DropdownItem
										key={option.shortName}
										isActive={displayedConsumer === option.shortName}
										onClick={() => {
											navigateToResolvedPath(displayedVersion, initialLocale, option.shortName);
										}}
									>
										<div className="flex items-center gap-2">
											<Icon />
											{option.name.charAt(0).toUpperCase() + option.name.slice(1)}
										</div>

										{displayedConsumer === option.shortName ? <FiCheck size={14} /> : null}
									</DropdownItem>
								);
							})}
						</div>
					) : null}
				</div>
			</div>
		</header>
	);
};

export default Header;
