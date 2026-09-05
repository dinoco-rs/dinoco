import React from 'react';
import { FiAlertOctagon, FiAlertTriangle, FiInfo, FiZap } from 'react-icons/fi';

import type { DocsLocale } from '../../jsons/versions';
import type { CalloutType } from '../../lib/remark-callouts';

type CalloutProps = {
	children?: React.ReactNode;
	locale: DocsLocale;
	type: CalloutType;
};

const labels: Record<DocsLocale, Record<CalloutType, string>> = {
	'en-us': { danger: 'Danger', note: 'Note', tip: 'Tip', warning: 'Warning' },
	'pt-br': { danger: 'Perigo', note: 'Nota', tip: 'Dica', warning: 'Aviso' },
};

const icons: Record<CalloutType, React.ComponentType<{ size?: number }>> = {
	danger: FiAlertOctagon,
	note: FiInfo,
	tip: FiZap,
	warning: FiAlertTriangle,
};

const styles: Record<CalloutType, string> = {
	danger: 'border-red-500 bg-red-500/5 text-red-700 dark:border-red-400 dark:text-red-300 [&_svg]:text-red-500 dark:[&_svg]:text-red-400',
	note: 'border-dinoco-brand bg-dinoco-brand/5 text-slate-700 dark:border-dinoco-cyan dark:text-slate-300 [&_svg]:text-dinoco-brand dark:[&_svg]:text-dinoco-cyan',
	tip: 'border-emerald-500 bg-emerald-500/5 text-emerald-700 dark:border-emerald-400 dark:text-emerald-300 [&_svg]:text-emerald-500 dark:[&_svg]:text-emerald-400',
	warning: 'border-amber-500 bg-amber-500/5 text-amber-800 dark:border-amber-400 dark:text-amber-300 [&_svg]:text-amber-500 dark:[&_svg]:text-amber-400',
};

const Callout = ({ children, locale, type }: CalloutProps): React.JSX.Element => {
	const Icon = icons[type];
	const label = labels[locale][type];

	return (
		<div className={`mb-6 rounded-r-lg border-l-4 px-5 py-4 [&>p]:mb-0 [&>p:not(:first-of-type)]:mt-2 ${styles[type]}`}>
			<div className="mb-1.5 flex items-center gap-2 text-xs font-bold uppercase tracking-wide">
				<Icon size={14} />
				{label}
			</div>
			{children}
		</div>
	);
};

export default Callout;
