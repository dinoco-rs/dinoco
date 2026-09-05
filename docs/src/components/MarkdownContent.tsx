import { promises as fs } from 'node:fs';
import path from 'node:path';

import rehypeShiki from '@shikijs/rehype';
import { toJsxRuntime } from 'hast-util-to-jsx-runtime';
import React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import remarkGfm from 'remark-gfm';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import { unified } from 'unified';

import Callout from './markdown/Callout';
import CodeBlockPre from './markdown/CodeBlockPre';
import dinocoGrammar from '../lib/dinoco.tmLanguage.json';
import { resolveDocsLocale } from '../lib/docs-preferences';
import { CALLOUT_TYPES, remarkCallouts } from '../lib/remark-callouts';
import { toAnchorId } from '../lib/to-anchor-id';

import type { Root } from 'hast';
import type { DocsLocale } from '../jsons/versions';
import type { CalloutType } from '../lib/remark-callouts';
import type { MarkdownCodeProps, MarkdownComponentProps, MarkdownContentProps } from '../types';

function classes(...values: Array<string | undefined>): string {
	return values.filter(Boolean).join(' ');
}

function isCalloutType(value: unknown): value is CalloutType {
	return typeof value === 'string' && (CALLOUT_TYPES as readonly string[]).includes(value);
}

function createMarkdownComponents(locale: DocsLocale) {
	return {
		h1: ({ children, className, id, ...props }: MarkdownComponentProps) => (
			<h1 {...props} id={id} className={classes('mb-6 text-4xl font-extrabold text-slate-900 dark:text-white', className)}>
				{children}
			</h1>
		),
		h2: ({ children, className, id, ...props }: MarkdownComponentProps) => (
			<h2 {...props} id={id} className={classes('mb-6 mt-12 scroll-mt-32 text-2xl font-bold text-slate-900 dark:text-white', className)}>
				{children}
			</h2>
		),
		h3: ({ children, className, id, ...props }: MarkdownComponentProps) => (
			<h3 {...props} id={id} className={classes('mb-4 mt-8 scroll-mt-32 text-xl font-semibold text-slate-900 dark:text-white', className)}>
				{children}
			</h3>
		),
		p: ({ children, className, ...props }: MarkdownComponentProps) => (
			<p {...props} className={classes('mb-6 leading-7 text-slate-600 dark:text-slate-300', className)}>
				{children}
			</p>
		),
		ul: ({ children, className, ...props }: MarkdownComponentProps) => (
			<ul {...props} className={classes('mb-6 list-disc space-y-2 pl-6 text-slate-600 marker:text-slate-400 dark:text-slate-300 dark:marker:text-slate-600', className)}>
				{children}
			</ul>
		),
		ol: ({ children, className, ...props }: MarkdownComponentProps) => (
			<ol {...props} className={classes('mb-6 list-decimal space-y-2 pl-6 text-slate-600 marker:text-slate-400 dark:text-slate-300 dark:marker:text-slate-600', className)}>
				{children}
			</ol>
		),
		li: ({ children, className, ...props }: MarkdownComponentProps) => (
			<li {...props} className={className}>
				{children}
			</li>
		),
		pre: CodeBlockPre,
		code: ({ children, className, ...props }: MarkdownCodeProps) => {
			if (className === undefined) {
				return (
					<code {...props} className="rounded-md border border-light-300 bg-light-200 px-1.5 py-0.5 font-mono text-[0.875em] font-semibold text-dinoco-deep dark:border-dark-700 dark:bg-dark-800 dark:text-dinoco-cyan">
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
		blockquote: ({ children, className, ...props }: MarkdownComponentProps & { 'data-callout'?: string }) => {
			const calloutType = props['data-callout'];

			if (isCalloutType(calloutType)) {
				return (
					<Callout type={calloutType} locale={locale}>
						{children}
					</Callout>
				);
			}

			return (
				<blockquote {...props} className={classes('mb-6 rounded-r-lg border-l-4 border-dinoco-brand bg-dinoco-brand/5 px-6 py-4 text-slate-700 dark:border-dinoco-cyan dark:bg-dark-800 dark:text-slate-300', className)}>
					{children}
				</blockquote>
			);
		},
		table: ({ children, className, ...props }: MarkdownComponentProps) => (
			<div className="mb-6 overflow-x-auto rounded-lg border border-light-300 bg-white shadow-sm dark:border-dark-700 dark:bg-dark-900">
				<table {...props} className={classes('min-w-full border-collapse text-left text-sm text-slate-600 dark:text-slate-300', className)}>
					{children}
				</table>
			</div>
		),
		thead: ({ children, className, ...props }: MarkdownComponentProps) => (
			<thead {...props} className={classes('bg-light-100 dark:bg-dark-800', className)}>
				{children}
			</thead>
		),
		tbody: ({ children, className, ...props }: MarkdownComponentProps) => (
			<tbody {...props} className={className}>
				{children}
			</tbody>
		),
		tr: ({ children, className, ...props }: MarkdownComponentProps) => (
			<tr {...props} className={classes('border-t border-light-300 dark:border-dark-700', className)}>
				{children}
			</tr>
		),
		th: ({ children, className, ...props }: MarkdownComponentProps) => (
			<th {...props} className={classes('border-l border-light-300 px-4 py-3 font-semibold text-slate-900 first:border-l-0 dark:border-dark-700 dark:text-white', className)}>
				{children}
			</th>
		),
		td: ({ children, className, ...props }: MarkdownComponentProps) => (
			<td {...props} className={classes('border-l border-light-300 px-4 py-3 align-top first:border-l-0 dark:border-dark-700', className)}>
				{children}
			</td>
		),
		a: ({ children, className, ...props }: React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
			<a {...props} className={classes('cursor-pointer font-medium text-dinoco-brand underline decoration-dinoco-brand/30 underline-offset-4 hover:decoration-dinoco-brand dark:text-dinoco-cyan dark:decoration-dinoco-cyan/30 dark:hover:decoration-dinoco-cyan', className)}>
				{children}
			</a>
		),
	};
}

type HastNode = {
	children?: unknown[];
	properties?: Record<string, unknown>;
	tagName?: string;
	type: string;
	value?: string;
};

function asHastNode(value: unknown): HastNode | undefined {
	if (typeof value !== 'object' || value === null || !('type' in value)) {
		return undefined;
	}

	return value as HastNode;
}

function extractText(children: unknown[]): string {
	return children
		.map(child => {
			const node = asHastNode(child);

			if (node === undefined) {
				return '';
			}

			return node.type === 'text' ? (node.value ?? '') : extractText(node.children ?? []);
		})
		.join('')
		.trim();
}

function addHeadingAnchors(value: unknown): void {
	const node = asHastNode(value);

	if (node === undefined) {
		return;
	}

	if (node.type === 'element' && ['h1', 'h2', 'h3'].includes(node.tagName ?? '')) {
		const text = extractText(node.children ?? []);

		if (text.length > 0) {
			node.properties = { ...node.properties, id: toAnchorId(text) };
		}
	}

	for (const child of node.children ?? []) {
		addHeadingAnchors(child);
	}
}

async function readMarkdownFile(contentPath: string): Promise<string> {
	const contentRoot = path.join(process.cwd(), 'src', 'content');
	const filePath = path.join(contentRoot, contentPath);

	if (!filePath.startsWith(`${contentRoot}${path.sep}`)) {
		throw new Error(`Invalid documentation content path: ${contentPath}`);
	}

	return fs.readFile(filePath, 'utf8');
}

function localeFromContentPath(contentPath: string): DocsLocale {
	const [, localeSegment] = contentPath.split('/');

	return resolveDocsLocale(localeSegment);
}

const MarkdownContent = async ({ contentPath }: MarkdownContentProps): Promise<React.JSX.Element> => {
	const source = await readMarkdownFile(contentPath);
	const processor = unified()
		.use(remarkParse)
		.use(remarkGfm)
		.use(remarkCallouts)
		.use(remarkRehype)
		.use(rehypeShiki, {
			addLanguageClass: true,
			defaultLanguage: 'txt',
			fallbackLanguage: 'txt',
			langs: [
				'bash',
				'json',
				'rust',
				'sql',
				'toml',
				{
					...dinocoGrammar,
					displayName: 'Dinoco',
					name: 'dinoco',
				},
			],
			themes: {
				dark: 'github-dark',
				light: 'github-light',
			},
		});
	const markdownTree = processor.parse(source);
	const hastTree = (await processor.run(markdownTree, { path: contentPath, value: source })) as Root;

	addHeadingAnchors(hastTree);

	const content = toJsxRuntime(hastTree, {
		Fragment: jsxRuntime.Fragment,
		components: createMarkdownComponents(localeFromContentPath(contentPath)),
		jsx: jsxRuntime.jsx,
		jsxs: jsxRuntime.jsxs,
	});

	return <div className="w-full">{content}</div>;
};

export default MarkdownContent;
