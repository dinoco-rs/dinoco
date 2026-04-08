import React from 'react';
import { Link } from 'tuono-router';
import { clsx } from 'clsx';
import { FiAlertTriangle, FiArrowRight } from 'react-icons/fi';

type OutdatedVersionNoticeProps = {
	currentVersionName: string;
	latestPath: string;
	latestVersionName: string;
};

const OutdatedVersionNotice: React.FC<OutdatedVersionNoticeProps> = ({ currentVersionName, latestPath, latestVersionName }) => {
	return (
		<div
			role="alert"
			className={clsx(
				'mb-8 flex flex-col gap-4 rounded-2xl px-5 py-4',

				'border border-amber-200 bg-amber-50 text-amber-950',

				'dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-100',

				'md:flex-row md:items-center md:justify-between',
			)}
		>
			<div className="flex items-start gap-3 md:items-center">
				<FiAlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-600 dark:text-amber-400 md:mt-0" aria-hidden="true" />
				<p className="text-sm font-medium leading-relaxed">
					Você está consultando a documentação da versão antiga <strong className="font-bold">{currentVersionName}</strong>. A versão mais recente é a{' '}
					<strong className="font-bold">{latestVersionName}</strong>.
				</p>
			</div>

			<Link
				href={latestPath}
				className={clsx(
					'group inline-flex w-fit shrink-0 items-center gap-2 rounded-full px-4 py-2 text-sm font-semibold',
					'transition-all duration-200 ease-in-out',

					'border border-amber-300 bg-white text-amber-950',
					'hover:bg-amber-100 hover:shadow-sm',

					'dark:border-amber-700 dark:bg-amber-900/40 dark:text-amber-50 dark:hover:bg-amber-900/70',
				)}
			>
				Ir para {latestVersionName}
				<FiArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
			</Link>
		</div>
	);
};

export default OutdatedVersionNotice;
