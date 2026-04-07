import React, { useEffect, useState } from 'react';
import { FiCheck, FiCopy } from 'react-icons/fi';
import clsx from 'clsx';
import type { CodeBlockProps } from '../types';

function normalizeLanguage(language: string): string {
	const parts = language
		.split(/\s+/)
		.map(part => part.trim())
		.filter(Boolean);
	const normalized = parts.map(part => part.replace(/^language-/, '')).find(part => part !== 'hljs');
	return normalized ?? 'txt';
}

function getLanguageLabel(language: string): string {
	const normalized = normalizeLanguage(language);
	if (normalized === 'ts') return 'TypeScript';
	if (normalized === 'js') return 'JavaScript';
	if (normalized === 'dinoco') return 'Dinoco';
	if (normalized === 'rust') return 'Rust';

	return normalized.toUpperCase();
}

const CodeBlock: React.FC<CodeBlockProps> = ({ children, code, language }) => {
	const [copied, setCopied] = useState(false);
	const languageLabel = getLanguageLabel(language);

	useEffect(() => {
		if (!copied) return;
		const timeout = window.setTimeout(() => setCopied(false), 2000);
		return () => window.clearTimeout(timeout);
	}, [copied]);

	const handleCopy = async () => {
		try {
			await navigator.clipboard.writeText(code);
			setCopied(true);
		} catch {
			setCopied(false);
		}
	};

	return (
		<div className="group relative overflow-hidden rounded-xl bg-[#0c0c0c] border border-light-300 shadow-sm dark:border-[#242424]">
			<div className="flex items-center justify-between border-b border-light-300 bg-light-100 px-4 py-2.5 dark:border-[#242424] dark:bg-[#050505]">
				<span className="text-xs font-semibold uppercase tracking-widest text-slate-500 dark:text-slate-400">{languageLabel}</span>
				<button
					type="button"
					onClick={() => void handleCopy()}
					className={clsx(
						'flex cursor-pointer items-center gap-1.5 rounded-md border border-light-300 bg-white px-2.5 py-1 text-xs font-semibold text-slate-600 transition-all hover:border-dinoco-brand/50 hover:text-dinoco-brand dark:border-[#242424] dark:bg-[#161616] dark:text-slate-300 dark:hover:border-dinoco-cyan/50 dark:hover:text-dinoco-cyan',
						copied && 'border-dinoco-cyan text-dinoco-cyan dark:border-dinoco-cyan dark:text-dinoco-cyan',
					)}
					aria-label={copied ? 'Código copiado' : 'Copiar código'}
				>
					{copied ? <FiCheck size={14} /> : <FiCopy size={14} />}

					<span>{copied ? 'Copiado' : 'Copiar'}</span>
				</button>
			</div>

			<pre className="overflow-x-auto  text-[0.875rem] leading-relaxed text-slate-800 dark:text-slate-50 dinoco-code-block">{children}</pre>
		</div>
	);
};

export default CodeBlock;
