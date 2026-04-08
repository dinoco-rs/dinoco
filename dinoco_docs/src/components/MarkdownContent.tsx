import React, { useEffect, useState } from 'react';
import clsx from 'clsx';
import type { MarkdownContentProps, MdxComponentProps, MdxCodeProps } from '../types';

const shikiVariables = {
	'--shiki-light': '#24292e',
	'--shiki-dark': '#e1e4e8',
	'--shiki-light-bg': '#fff',
	'--shiki-dark-bg': '#24292e',
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
	if (language === 'ts') return 'TypeScript';
	if (language === 'js') return 'JavaScript';
	if (language === 'tsx') return 'TSX';
	if (language === 'jsx') return 'JSX';
	if (language === 'bash' || language === 'shellscript' || language === 'sh') return 'Bash';
	if (language === 'json') return 'JSON';
	if (language === 'yaml') return 'YAML';
	if (language === 'toml') return 'TOML';
	if (language === 'sql') return 'SQL';
	if (language === 'rust') return 'Rust';
	if (language === 'dinoco') return 'Dinoco';
	if (language === 'txt' || language === 'plaintext') return 'Texto';

	return language.toUpperCase();
}

function toAnchorId(value: string): string {
	return value.toLowerCase().split(' ').join('-');
}

function createHeading(level: 'h1' | 'h2' | 'h3', baseClassName: string) {
	return function Heading({ children, className, ...props }: MdxComponentProps): React.JSX.Element {
		const text = React.Children.toArray(children).join('').trim();
		const id = text.length > 0 ? toAnchorId(text) : undefined;
		return React.createElement(level, { ...props, id, className: clsx(baseClassName, className) }, children);
	};
}

function MarkdownPre({ children, className, ...props }: MdxComponentProps): React.JSX.Element {
	const [copied, setCopied] = useState(false);
	const codeElement = React.Children.toArray(children)[0] as React.ReactElement<{
		children?: React.ReactNode;
		className?: string;
	}>;
	const language = normalizeLanguageClassName(codeElement?.props?.className);
	const code = getNodeText(codeElement?.props?.children).trim();
	const languageLabel = getLanguageLabel(language);

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
		<div {...props} style={shikiVariables} className={clsx('mb-6 overflow-hidden rounded-xl border border-light-300 bg-light-50 shadow-sm dark:border-[#242424] dark:bg-[#0c0c0c]', className)}>
			<div className="flex items-center justify-between border-b border-light-300 bg-light-100 px-4 py-2.5 dark:border-[#242424] dark:bg-[#050505]">
				<span className="text-xs font-semibold uppercase tracking-widest text-slate-500 dark:text-slate-400">{languageLabel}</span>

				<button
					type="button"
					onClick={() => handleCopy()}
					className={clsx(
						'cursor-pointer rounded-md border border-light-300 bg-white px-2.5 py-1 text-xs font-semibold text-slate-600 transition-colors hover:border-dinoco-brand/50 hover:text-dinoco-brand dark:border-[#242424] dark:bg-[#161616] dark:text-slate-300 dark:hover:border-dinoco-cyan/50 dark:hover:text-dinoco-cyan',
						copied && 'border-dinoco-cyan text-dinoco-cyan dark:border-dinoco-cyan dark:text-dinoco-cyan',
					)}
					aria-label={copied ? 'Código copiado' : 'Copiar código'}
				>
					{copied ? 'Copiado' : 'Copiar'}
				</button>
			</div>

			<pre className="overflow-x-auto p-4 text-[0.875rem]">{children}</pre>
		</div>
	);
}

const MarkdownContent: React.FC<MarkdownContentProps> = ({ component: Content }) => {
	const mdxComponents = {
		h1: createHeading('h1', 'mb-6 text-4xl font-extrabold tracking-tight text-slate-900 dark:text-white'),
		h2: createHeading('h2', 'mt-12 mb-6 scroll-mt-32 text-2xl font-bold tracking-tight text-slate-900 dark:text-white'),
		h3: createHeading('h3', 'mt-8 mb-4 scroll-mt-32 text-xl font-semibold tracking-tight text-slate-900 dark:text-white'),
		p: ({ children, className, ...props }: MdxComponentProps) => (
			<p {...props} className={clsx('mb-6 leading-7 text-slate-600 dark:text-slate-300', className)}>
				{children}
			</p>
		),
		ul: ({ children, className, ...props }: MdxComponentProps) => (
			<ul {...props} className={clsx('mb-6 list-disc space-y-2 pl-6 text-slate-600 marker:text-slate-400 dark:text-slate-300 dark:marker:text-[#242424]', className)}>
				{children}
			</ul>
		),
		li: ({ children, className, ...props }: MdxComponentProps) => (
			<li {...props} className={className}>
				{children}
			</li>
		),
		pre: MarkdownPre,
		code: ({ children, className, ...props }: MdxCodeProps) => {
			if (className === undefined) {
				return (
					<code
						{...props}
						className="rounded-md bg-light-200 px-1.5 py-0.5 font-mono text-[0.875em] font-semibold text-dinoco-deep dark:bg-[#161616] dark:text-dinoco-cyan border dark:border-[#242424]"
					>
						{children}
					</code>
				);
			}
			return (
				<code {...props} className={className}>
					{children}
				</code>
			);
		},
		blockquote: ({ children, className, ...props }: MdxComponentProps) => (
			<blockquote
				{...props}
				className={clsx('mb-6 border-l-4 border-dinoco-brand bg-dinoco-brand/5 px-6 py-4 text-slate-700 dark:border-dinoco-cyan dark:bg-[#161616] dark:text-slate-300 rounded-r-lg', className)}
			>
				{children}
			</blockquote>
		),
		a: ({ children, className, ...props }: React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
			<a
				{...props}
				className={clsx(
					'cursor-pointer font-medium text-dinoco-brand underline decoration-dinoco-brand/30 underline-offset-4 hover:decoration-dinoco-brand dark:text-dinoco-cyan dark:decoration-dinoco-cyan/30 dark:hover:decoration-dinoco-cyan',
					className,
				)}
			>
				{children}
			</a>
		),
	};

	return (
		<div className="w-full">
			<Content components={mdxComponents} />
		</div>
	);
};

export default MarkdownContent;
