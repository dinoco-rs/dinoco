import React from 'react';
import Link from 'next/link';

import { clsx } from 'clsx';
import { FiArrowLeft, FiArrowRight } from 'react-icons/fi';

import type { DocsContentNavigationProps } from '../types';

const NavigationCard = ({ align, href, label, title }: { align: 'left' | 'right'; href: string; label: string; title: string }) => {
	const isLeft = align === 'left';

	return (
		<Link
			href={href}
			className={clsx(
				'group flex min-h-[112px] flex-1 flex-col justify-between rounded-2xl px-5 py-4 not-dark:shadow-md not-dark:border not-dark:border-gray-200',
				'border border-light-200 bg-light-100',
				'transition-all duration-200 ease-in-out',
				'hover:border-dinoco-brand hover:bg-light-50 hover:shadow-sm',
				'dark:border-dark-700 dark:bg-dark-900 dark:hover:border-dinoco-cyan dark:hover:bg-dark-800',
				isLeft ? 'items-start text-left' : 'items-end text-right',
			)}
		>
			<span className="text-xs font-bold uppercase tracking-[0.2em] text-slate-500 dark:text-slate-400">{label}</span>

			<div className={clsx('mt-2 flex w-full items-center gap-3', isLeft ? 'justify-start' : 'justify-end')}>
				{isLeft && <FiArrowLeft className="h-5 w-5 transition-transform group-hover:-translate-x-1" />}

				<span className="line-clamp-2 text-base font-semibold text-slate-900 transition-colors group-hover:text-dinoco-brand dark:text-white dark:group-hover:text-dinoco-cyan">{title}</span>

				{!isLeft && <FiArrowRight className="h-5 w-5 transition-transform group-hover:translate-x-1" />}
			</div>
		</Link>
	);
};

const DocsContentNavigation: React.FC<DocsContentNavigationProps> = ({ previous, next }) => {
	if (!previous && !next) {
		return null;
	}

	return (
		<nav aria-label="Navegação da documentação" className="mt-10 grid gap-4 border-t border-light-200 pt-6 dark:border-dark-700 md:grid-cols-2">
			{previous ? <NavigationCard label="Anterior" title={previous.item.name} href={previous.path} align="left" /> : <div className="hidden md:block" aria-hidden="true" />}

			{next ? <NavigationCard label="Próximo" title={next.item.name} href={next.path} align="right" /> : <div className="hidden md:block" aria-hidden="true" />}
		</nav>
	);
};

export default DocsContentNavigation;
