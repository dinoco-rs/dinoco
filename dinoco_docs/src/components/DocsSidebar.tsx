import React, { useEffect, useMemo, useState } from 'react';
import clsx from 'clsx';
import { Link } from 'tuono-router';
import { FiBox, FiDatabase, FiLayers, FiTerminal, FiX, FiChevronDown } from 'react-icons/fi';

import { buildDocsPath, getGroupByShortName, getLocalizedSections } from '../jsons/versions';
import type { DocsSidebarProps } from '../types';
import type { DocsItem, DocsGroup } from '../jsons/versions';

export const sidebarIconMap = {
	box: FiBox,
	database: FiDatabase,
	rocket: FiLayers,
	terminal: FiTerminal,
} as const;

const NavItem = ({
	item,
	currentGroup,
	currentItem,
	currentVersionName,
	onClose,
}: {
	item: DocsItem;
	currentGroup: DocsGroup;
	currentItem: DocsItem;
	currentVersionName: string;
	onClose: () => void;
}) => {
	const hasSubItems = item.subItems !== undefined && item.subItems.length > 0;
	const firstSubItem = hasSubItems ? item.subItems![0] : undefined;
	const isItemActive = currentItem.shortName === item.shortName;
	const isChildActive = hasSubItems && item.subItems!.some(sub => sub.shortName === currentItem.shortName);

	const [isOpen, setIsOpen] = useState(isItemActive || isChildActive);

	useEffect(() => {
		if (isItemActive || isChildActive) {
			setIsOpen(true);
		}
	}, [isItemActive, isChildActive]);

	return (
		<li className="relative">
			<div className="flex items-center justify-between">
				{hasSubItems ? (
					<div
						className={clsx(
							'flex items-center border-l -ml-[1px] text-sm transition-colors',
							isItemActive || isChildActive ? 'border-dinoco-brand dark:border-dinoco-cyan' : 'border-transparent hover:border-slate-300 dark:hover:border-slate-600',
						)}
					>
						<Link
							href={buildDocsPath(currentVersionName, currentGroup.shortName, item.shortName, firstSubItem?.shortName)}
							onClick={onClose}
							className={clsx(
								'block min-w-0 flex-1 cursor-pointer pl-4 py-1.5 text-left',
								isItemActive || isChildActive
									? 'font-semibold text-dinoco-brand dark:text-dinoco-cyan'
									: 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white',
							)}
						>
							{item.name}
						</Link>

						<button
							onClick={() => setIsOpen(!isOpen)}
							className="cursor-pointer px-3 py-2 text-slate-400 transition-colors hover:text-slate-900 dark:text-slate-500 dark:hover:text-white"
							aria-expanded={isOpen}
							aria-label={isOpen ? 'Fechar subitens' : 'Abrir subitens'}
						>
							<FiChevronDown size={14} className={clsx('transition-transform duration-200', isOpen ? 'rotate-180' : '')} />
						</button>
					</div>
				) : (
					<Link
						href={buildDocsPath(currentVersionName, currentGroup.shortName, item.shortName)}
						onClick={onClose}
						className={clsx(
							'block w-full cursor-pointer border-l -ml-[1px] pl-4 py-1.5 text-sm transition-colors',
							isItemActive
								? 'border-dinoco-brand font-semibold text-dinoco-brand dark:border-dinoco-cyan dark:text-dinoco-cyan'
								: 'border-transparent text-slate-600 hover:border-slate-200 hover:text-slate-900 dark:text-slate-400 dark:hover:border-slate-600 dark:hover:text-white',
						)}
					>
						{item.name}
					</Link>
				)}
			</div>

			{hasSubItems && isOpen && (
				<ul className="ml-4 mt-1 space-y-1 border-l border-light-300 dark:border-[#505050]">
					{item.subItems!.map(subItem => {
						const isSubActive = currentItem.shortName === subItem.shortName;
						return (
							<li key={subItem.shortName}>
								<Link
									href={buildDocsPath(currentVersionName, currentGroup.shortName, item.shortName, subItem.shortName)}
									onClick={onClose}
									className={clsx(
										'block cursor-pointer border-l -ml-[1px] pl-4 py-1.5 text-sm transition-colors',
										isSubActive
											? 'border-dinoco-brand font-semibold text-dinoco-brand dark:border-dinoco-cyan dark:text-dinoco-cyan'
											: 'border-transparent text-slate-500 hover:border-slate-300 hover:text-slate-900 dark:text-slate-400 dark:hover:border-slate-600 dark:hover:text-white',
									)}
								>
									{subItem.name}
								</Link>
							</li>
						);
					})}
				</ul>
			)}
		</li>
	);
};

const DocsSidebar: React.FC<DocsSidebarProps> = ({ currentGroup, currentItem, currentVersionName, locale, sections, isOpen, onClose }) => {
	const localizedSections = useMemo(() => {
		const localizedGroup = getGroupByShortName(currentVersionName, locale, currentGroup.shortName);

		if (localizedGroup === undefined) {
			return sections;
		}

		return getLocalizedSections(localizedGroup, locale);
	}, [currentGroup.shortName, currentVersionName, locale, sections]);

	return (
		<>
			{isOpen && <div className="fixed inset-0 z-[250] cursor-pointer bg-dark-950/60 backdrop-blur-sm lg:hidden" onClick={onClose} />}

			<aside
				className={clsx(
					'docs-sidebar-scroll fixed inset-y-0 left-0 z-[250] lg:z-[100] w-[18rem] overflow-x-hidden overflow-y-auto border-r border-light-200 bg-light-50 px-4 pb-10 pt-3.5 transform transition-transform duration-300 dark:border-[#242424] dark:bg-[#0c0c0c] lg:sticky lg:top-20 lg:block lg:h-[calc(100vh-5rem)] lg:w-64 lg:translate-x-0 lg:border-none lg:bg-transparent lg:px-0 lg:pt-0 lg:dark:bg-transparent',
					isOpen ? 'translate-x-0' : '-translate-x-full',
				)}
			>
				<div className="flex items-center gap-2 md:gap-4 md:mb-0"></div>

				<div className="mb-5 flex items-center lg:hidden gap-2">
					<button onClick={onClose} className="cursor-pointer rounded-md p-2 text-slate-500 hover:bg-light-200 dark:text-slate-400 dark:hover:bg-[#161616] lg:hidden">
						<FiX size={20} />
					</button>

					<div className="flex items-center gap-3">
						<img src="/logo.png" className="h-8 w-8 object-contain not-dark:invert" alt="Dinoco Logo" />
						<div className="flex items-center gap-1.5 text-xl">
							<span className="font-bungee text-slate-900 dark:text-white">Dinoco</span>
						</div>
					</div>
				</div>

				<nav className="space-y-8 pt-6">
					{localizedSections.map(section => (
						<div key={section.title}>
							<h4 className="mb-3 text-xs font-bold uppercase tracking-widest text-slate-900 dark:text-white">{section.title}</h4>

							<ul className="space-y-1 border-l border-light-300 dark:border-[#505050]">
								{section.items.map(item => (
									<NavItem
										key={`${currentVersionName}:${currentGroup.shortName}:${currentItem.shortName}:${item.shortName}`}
										item={item}
										currentGroup={currentGroup}
										currentItem={currentItem}
										currentVersionName={currentVersionName}
										onClose={onClose}
									/>
								))}
							</ul>
						</div>
					))}
				</nav>
			</aside>
		</>
	);
};

export default DocsSidebar;
