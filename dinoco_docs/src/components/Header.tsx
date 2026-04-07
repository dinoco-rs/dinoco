// 							<DropdownButton isOpen={versionOpen} onClick={() => closeOtherMenus('version')} className="py-1 px-2.5 h-8">

import React, { useEffect, useMemo, useRef, useState } from 'react';
import clsx from 'clsx';
import { useRouter } from 'tuono-router';
import { FaGithub, FaHeart } from 'react-icons/fa';
import { FiCheck, FiChevronDown, FiGlobe, FiMoon, FiSun, FiMenu, FiBox, FiDatabase, FiLayers, FiTerminal } from 'react-icons/fi';

import { buildDocsPath, getAvailableLocales, getGroupsForVersion, getVersionNames, parseDocsPath, resolveDocsPath } from '../jsons/versions';
import { useIntl } from '../hooks/useIntl';
import { type DocsLocale, useDocs } from '../hooks/useDocs';

import type { DropdownButtonProps, DropdownItemProps, HeaderProps } from '../types';

const iconMap = {
	box: FiBox,
	database: FiDatabase,
	rocket: FiLayers,
	terminal: FiTerminal,
} as Record<string, any>;

function DropdownButton({ isOpen, children, onClick, className }: DropdownButtonProps): React.JSX.Element {
	return (
		<button onClick={onClick} className={clsx('flex cursor-pointer items-center gap-2 rounded-md py-1.5 transition-colors', className)}>
			{children}
			<FiChevronDown size={14} className={clsx('text-slate-400 transition-transform duration-200', isOpen && 'rotate-180')} />
		</button>
	);
}

function DropdownItem({ isActive, children, onClick }: DropdownItemProps): React.JSX.Element {
	return (
		<button
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

const Header: React.FC<HeaderProps> = ({ onMenuToggle }) => {
	const locale = useDocs(state => state.locale);
	const version = useDocs(state => state.version);
	const consumer = useDocs(state => state.consumer);
	const theme = useDocs(state => state.theme);

	const setLocale = useDocs(state => state.setLocale);
	const setTheme = useDocs(state => state.setTheme);
	const setVersion = useDocs(state => state.setVersion);
	const setConsumer = useDocs(state => state.setConsumer);

	const intl = useIntl();
	const router = useRouter();

	const [localeOpen, setLocaleOpen] = useState(false);
	const [versionOpen, setVersionOpen] = useState(false);
	const [mobileConsumerOpen, setMobileConsumerOpen] = useState(false);

	const controlsRef = useRef<HTMLDivElement>(null);

	const versionOptions = useMemo(() => getVersionNames(), []);
	const routeParams = useMemo(() => parseDocsPath(router.pathname), [router.pathname]);
	const currentResolved = useMemo(() => {
		return resolveDocsPath({
			versionName: routeParams.versionName ?? version,
			groupShortName: routeParams.groupShortName ?? consumer,
			itemShortName: routeParams.itemShortName,
			subItemShortName: routeParams.subItemShortName,
			locale,
		});
	}, [consumer, locale, routeParams.groupShortName, routeParams.itemShortName, routeParams.subItemShortName, routeParams.versionName, version]);

	const displayedVersion = currentResolved?.version.name ?? version;
	const displayedConsumer = currentResolved?.group.shortName ?? consumer;
	const localeOptions = useMemo(() => getAvailableLocales(displayedVersion), [displayedVersion]);
	const consumerOptions = useMemo(() => {
		return getGroupsForVersion(displayedVersion, locale);
	}, [displayedVersion, locale]);

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
		const resolved = resolveDocsPath({
			versionName: nextVersion,
			groupShortName: nextConsumer,
			itemShortName: routeParams.itemShortName,
			subItemShortName: routeParams.subItemShortName,
			locale: nextLocale,
		});
		if (resolved === undefined) return;

		setVersion(resolved.version.name);
		setLocale(nextLocale);
		setConsumer(resolved.group.shortName);

		router.push(buildDocsPath(resolved.version.name, resolved.group.shortName, resolved.parentItem?.shortName ?? resolved.item.shortName, resolved.parentItem?.shortName === undefined ? undefined : resolved.item.shortName));
	};

	const currentConsumerObj = consumerOptions.find(o => o.shortName === displayedConsumer);
	const currentConsumerLabel = currentConsumerObj ? currentConsumerObj.name.charAt(0).toUpperCase() + currentConsumerObj.name.slice(1) : displayedConsumer;
	const CurrentConsumerIcon = iconMap[currentConsumerObj!.icon];

	function renderVersionAndLocale(md = true) {
		return (
			<div className={clsx(md ? 'hidden md:flex items-center gap-2' : 'grid grid-cols-2 gap-3 w-full mb-3 md:hidden')}>
				<div className="relative w-full">
					<DropdownButton
						isOpen={localeOpen}
						onClick={() => closeOtherMenus('locale')}
						className={clsx(
							'h-8',
							md
								? 'shrink-1 py-1 px-0'
								: 'w-full justify-between rounded-md border border-light-300 bg-transparent px-3 hover:bg-light-100 dark:border-[#242424] dark:hover:bg-[#161616]',
						)}
					>
						<div className="flex items-center gap-1.5">
							{!md && <FiGlobe size={14} className="text-slate-500 dark:text-slate-400" />}
							<span className={clsx('text-sm', !md && 'font-medium text-slate-700 dark:text-slate-300')}>{intl.locales[locale]}</span>
						</div>
					</DropdownButton>

					{localeOpen && (
						<div
							className={clsx(
								'absolute z-50 flex flex-col overflow-hidden rounded-lg border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#161616]',
								md ? 'right-0 mt-3 w-36' : 'left-0 mt-1 w-full',
							)}
						>
							{localeOptions.map(option => (
								<DropdownItem
									key={option}
									isActive={locale === option}
									onClick={() => {
										navigateToResolvedPath(displayedVersion, option, displayedConsumer);
										setLocaleOpen(false);
									}}
								>
									{intl.locales[option]}
									{locale === option && <FiCheck size={14} />}
								</DropdownItem>
							))}
						</div>
					)}
				</div>

				<div className="relative w-full">
					<DropdownButton
						isOpen={versionOpen}
						onClick={() => closeOtherMenus('version')}
						className={clsx(
							'h-8',
							md ? 'py-1 px-2.5' : 'w-full justify-between rounded-md border border-light-300 bg-transparent px-3 hover:bg-light-100 dark:border-[#242424] dark:hover:bg-[#161616]',
						)}
					>
						<span className="text-sm font-semibold text-dinoco-deep dark:text-dinoco-cyan">{displayedVersion}</span>
					</DropdownButton>

					{versionOpen && (
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
										navigateToResolvedPath(option, locale, displayedConsumer);
										setVersionOpen(false);
									}}
								>
									{option}
									{displayedVersion === option && <FiCheck size={14} />}
								</DropdownItem>
							))}
						</div>
					)}
				</div>
			</div>
		);
	}

	return (
		<header className="block md:sticky top-0 z-40 w-full border-b border-light-300 bg-light-50/95 backdrop-blur-md transition-colors duration-300 dark:border-[#242424] dark:bg-[#050505]/95">
			<div className="flex flex-col w-full px-4 sm:px-6 md:px-8" ref={controlsRef}>
				<div className="flex md:h-10 mt-4 mb-2 sm:mb-0 items-center justify-between">
					<div className="flex items-center gap-2 md:gap-4 mb-2 md:mb-0">
						<button onClick={onMenuToggle} className="cursor-pointer rounded-md p-2 text-slate-500 hover:bg-light-200 dark:text-slate-400 dark:hover:bg-[#161616] lg:hidden">
							<FiMenu size={20} />
						</button>

						<div className="flex items-center gap-3">
							<img src="/logo.png" className="h-8 w-8 object-contain not-dark:invert" alt="Dinoco Logo" />
							<div className="flex items-center gap-1.5 text-xl">
								<span className="font-bungee text-slate-900 dark:text-white">Dinoco</span>
							</div>
						</div>
					</div>

					<div className="flex items-center gap-3 sm:gap-4">
						{renderVersionAndLocale(true)}

						<a
							href="https://github.com/dinoco-rs/dinoco"
							target="_blank"
							rel="noreferrer"
							className="cursor-pointer not-dark:text-slate-600 text-slate-400 transition-colors hover:text-dinoco-brand flex items-center gap-2"
							title="GitHub"
						>
							<FaGithub size={18} />
							<span className="text-sm hidden md:block">{intl.github}</span>
						</a>

						<a
							href="https://buymeacoffee.com/theuszastro"
							target="_blank"
							rel="noreferrer"
							className="cursor-pointer not-dark:text-slate-600 text-slate-400 transition-colors hover:text-red-500 flex items-center gap-2"
							title="Donate"
						>
							<FaHeart size={18} />
							<span className="text-sm hidden md:block">{intl.donate}</span>
						</a>

						<div className="h-4 w-px bg-light-300 dark:bg-[#242424]"></div>

						<div className="hidden md:flex items-center rounded-full border border-light-300 bg-light-100 p-1 gap-2 dark:border-[#242424] dark:bg-[#161616]">
							<button
								onClick={() => setTheme('light')}
								className={clsx('cursor-pointer rounded-full p-2 transition-all', theme === 'light' ? 'bg-gray-200 text-orange-500 shadow-sm' : 'text-slate-400 hover:text-slate-600')}
							>
								<FiSun size={14} />
							</button>

							<button
								onClick={() => setTheme('dark')}
								className={clsx(
									'cursor-pointer rounded-full p-2 transition-all border',
									theme === 'dark' ? 'border-[#242424] bg-[#0c0c0c] text-dinoco-cyan shadow-sm' : 'border-transparent text-slate-400 hover:text-slate-500',
								)}
							>
								<FiMoon size={14} />
							</button>
						</div>

						{/* <div className="hidden md:flex items-center rounded-full border border-light-300 bg-light-100 p-1 gap-2 dark:border-[#242424] dark:bg-[#161616]">
							<button
								onClick={() => setTheme('light')}
								className={clsx('cursor-pointer rounded-full p-2 transition-all not-dark:bg-gray-200 not-dark:text-orange-500 not-dark:shadow-sm')}
							>
								<FiSun size={14} />
							</button>

							<button
								onClick={() => setTheme('dark')}
								className={clsx('cursor-pointer rounded-full p-2 transition-all', 'text-slate-400 hover:text-slate-300 dark:bg-[#0c0c0c] dark:text-dinoco-cyan dark:border-[#242424] ')}
							>
								<FiMoon size={14} />
							</button>
						</div> */}

						<div className="md:hidden flex items-center justify-center">
							<button onClick={() => setTheme(theme == 'light' ? 'dark' : 'light')} className="cursor-pointer rounded-full transition-all">
								{theme == 'light' ? <FiMoon size={18} /> : <FiSun size={18} />}
							</button>
						</div>
					</div>
				</div>

				<div className="flex items-center gap-3 sm:gap-4">{renderVersionAndLocale(false)}</div>

				<nav className="hidden sm:flex gap-4 overflow-x-auto no-scrollbar">
					{consumerOptions.map(option => {
						const isActive = displayedConsumer === option.shortName;
						const Icon = iconMap[option.icon] as any;

						return (
							<button
								key={option.name}
								onClick={() => navigateToResolvedPath(displayedVersion, locale, option.shortName)}
								className={clsx(
									'cursor-pointer whitespace-nowrap border-b-2 py-3 text-sm font-semibold transition-colors flex items-center gap-2',
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

				<div className="block sm:hidden w-full relative mb-3">
					<button
						onClick={() => closeOtherMenus('mobileConsumer')}
						className="flex w-full cursor-pointer items-center justify-between rounded-md border border-light-300 bg-light-100 px-4 py-2.5 text-sm font-semibold transition-colors dark:border-[#242424] dark:bg-[#161616] dark:text-white"
					>
						<div className="flex items-center gap-2 text-slate-600">
							<CurrentConsumerIcon />

							<span>{currentConsumerLabel}</span>
						</div>

						<FiChevronDown size={16} className={clsx('text-slate-500 transition-transform duration-200', mobileConsumerOpen && 'rotate-180')} />
					</button>

					{mobileConsumerOpen && (
						<div className="absolute left-0 top-full z-50 mt-1 w-full overflow-hidden rounded-md border border-light-200 bg-light-50 shadow-xl dark:border-[#242424] dark:bg-[#0c0c0c]">
							{consumerOptions.map(option => {
								const Icon = iconMap[option.icon] as any;

								return (
									<DropdownItem
										key={option.shortName}
										isActive={displayedConsumer === option.shortName}
										onClick={() => {
											navigateToResolvedPath(displayedVersion, locale, option.shortName);
											setMobileConsumerOpen(false);
										}}
									>
										<div className="flex items-center gap-2">
											<Icon />
											{option.name.charAt(0).toUpperCase() + option.name.slice(1)}
										</div>

										{displayedConsumer === option.shortName && <FiCheck size={14} />}
									</DropdownItem>
								);
							})}
						</div>
					)}
				</div>
			</div>
		</header>
	);
};

export default Header;
