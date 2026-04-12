'use client';

import React, { useEffect, useState } from 'react';

import type { MarkdownComponentProps } from '../../types';

const shikiVariables = {
	'--shiki-light-bg': '#fff',
	'--shiki-dark-bg': '#101010',
} as React.CSSProperties;

function getNodeText(node: React.ReactNode): string {
	if (typeof node === 'string' || typeof node === 'number') {
		return String(node);
	}

	if (Array.isArray(node)) {
		return node.map(getNodeText).join('');
	}

	if (React.isValidElement(node)) {
		return getNodeText((node.props as { children?: React.ReactNode }).children);
	}

	return '';
}

function normalizeLanguageClassName(className?: string): string {
	if (className === undefined) {
		return 'txt';
	}

	const parts = className
		.split(/\s+/)
		.map(part => part.trim())
		.filter(Boolean);
	const languageClass = parts.find(part => part.startsWith('language-'));

	if (languageClass !== undefined) {
		return languageClass.replace('language-', '');
	}

	return parts.find(part => part !== 'hljs') ?? 'txt';
}

function getLanguageLabel(language: string): string {
	if (language === 'bash' || language === 'shellscript' || language === 'sh') return 'Bash';
	if (language === 'json') return 'JSON';
	if (language === 'toml') return 'TOML';
	if (language === 'sql') return 'SQL';
	if (language === 'rust') return 'Rust';
	if (language === 'dinoco') return 'Dinoco';

	return language.toUpperCase();
}

const CodeBlockPre = ({ children, className, ...props }: MarkdownComponentProps): React.JSX.Element => {
	const [copied, setCopied] = useState(false);
	const codeElement = React.Children.toArray(children)[0] as React.ReactElement<{
		children?: React.ReactNode;
		className?: string;
	}> | undefined;
	const language = normalizeLanguageClassName(codeElement?.props?.className);
	const code = getNodeText(codeElement?.props?.children).trim();

	useEffect(() => {
		if (!copied) {
			return;
		}

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
		<div
			{...props}
			style={shikiVariables}
			className={['mb-6 overflow-hidden rounded-xl border border-light-300 bg-light-50 shadow-sm dark:border-[#242424] dark:bg-[#0c0c0c]', className].filter(Boolean).join(' ')}
		>
			<div className="flex items-center justify-between border-b border-light-300 bg-light-100 px-4 py-2.5 dark:border-[#242424] dark:bg-[#050505]">
				<p className="text-xs font-semibold uppercase tracking-widest text-slate-500 dark:text-slate-400">{getLanguageLabel(language)}</p>

				<button
					type="button"
					onClick={handleCopy}
					className={[
						'cursor-pointer rounded-md border border-light-300 bg-white px-2.5 py-1 text-xs font-semibold text-slate-600 transition-colors hover:border-dinoco-brand/50 hover:text-dinoco-brand dark:border-[#242424] dark:bg-[#161616] dark:text-slate-300 dark:hover:border-dinoco-cyan/50 dark:hover:text-dinoco-cyan',
						copied ? 'border-dinoco-cyan text-dinoco-cyan dark:border-dinoco-cyan dark:text-dinoco-cyan' : '',
					]
						.filter(Boolean)
						.join(' ')}
					aria-label={copied ? 'Codigo copiado' : 'Copiar codigo'}
				>
					{copied ? 'Copiado' : 'Copiar'}
				</button>
			</div>

			<pre className="overflow-x-auto p-4 text-[0.875rem]">{children}</pre>
		</div>
	);
};

export default CodeBlockPre;
