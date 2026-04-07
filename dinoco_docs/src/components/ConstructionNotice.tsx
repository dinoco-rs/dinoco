import React from 'react';
import clsx from 'clsx';
import { FiTool } from 'react-icons/fi';
import { useIntl } from '../hooks/useIntl';

export type ConstructionNoticeProps = {
	badge?: string;
	title?: string;
	description?: string;
	className?: string;
};

const ConstructionNotice: React.FC<ConstructionNoticeProps> = ({ badge, title, description, className }) => {
	const intl = useIntl();

	const displayBadge = badge ?? intl.constructionBadge;
	const displayTitle = title ?? intl.constructionTitle;
	const displayDescription = description ?? intl.constructionDescription;

	return (
		<div
			className={clsx(
				'my-8 flex flex-col gap-5 rounded-2xl border-2 border-dashed p-6 sm:flex-row sm:items-start',
				'border-light-300 bg-light-50/50 dark:border-[#242424] dark:bg-[#0c0c0c]/50',
				className,
			)}
		>
			<div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-full bg-dinoco-brand/10 text-dinoco-brand dark:bg-dinoco-cyan/10 dark:text-dinoco-cyan">
				<FiTool size={24} />
			</div>

			<div className="flex flex-col">
				{displayBadge && (
					<span className="mb-3 w-fit rounded-full border border-dinoco-brand/20 bg-dinoco-brand/10 px-2.5 py-0.5 text-[0.65rem] font-bold uppercase tracking-widest text-dinoco-brand dark:border-dinoco-cyan/20 dark:bg-dinoco-cyan/10 dark:text-dinoco-cyan">
						{displayBadge}
					</span>
				)}

				<h3 className="mb-2 mt-0 text-lg font-bold text-slate-900 dark:text-white">{displayTitle}</h3>

				<p className="mb-0 text-sm leading-relaxed text-slate-600 dark:text-slate-400">{displayDescription}</p>
			</div>
		</div>
	);
};

export default ConstructionNotice;
