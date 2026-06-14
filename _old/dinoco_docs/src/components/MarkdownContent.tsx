import { promises as fs } from 'node:fs';
import path from 'node:path';

import React from 'react';
import { toJsxRuntime } from 'hast-util-to-jsx-runtime';
import rehypeShiki from '@shikijs/rehype';
import remarkGfm from 'remark-gfm';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import { unified } from 'unified';
import * as jsxRuntime from 'react/jsx-runtime';

import CodeBlockPre from './markdown/CodeBlockPre';

import type { Root } from 'hast';
import type { MarkdownCodeProps, MarkdownComponentProps, MarkdownContentProps } from '../types';

const dinocoGrammar = {
	$schema: 'https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json',
	name: 'Dinoco',
	patterns: [
		{ include: '#comments' },
		{ include: '#strings' },
		{ include: '#model_declaration' },
		{ include: '#config_declaration' },
		{ include: '#enum_declaration' },
		{ include: '#primitive_fields' },
		{ include: '#custom_fields' },
		{ include: '#model_decorators' },
		{ include: '#decorators' },
		{ include: '#named_parameters' },
		{ include: '#functions' },
		{ include: '#constants' },
	],
	repository: {
		comments: { match: '#.*$', name: 'comment.line.number-sign.dinoco' },
		strings: { begin: '"', end: '"', name: 'string.quoted.double.dinoco' },
		model_declaration: {
			match: '^\\s*(model)(\\s+([A-Za-z][a-zA-Z0-9_]*))?',
			captures: {
				1: { name: 'keyword.control.model.dinoco' },
				2: { name: 'entity.name.type.class.dinoco' },
			},
		},
		enum_declaration: {
			match: '^\\s*(enum)(\\s+([A-Za-z][a-zA-Z_]*))?',
			captures: {
				1: { name: 'keyword.control.enum.dinoco' },
				2: { name: 'entity.name.type.class.dinoco' },
			},
		},
		config_declaration: { match: '^\\s*(config)\\b', name: 'keyword.control.config.dinoco' },
		primitive_fields: {
			match: '^\\s*([a-zA-Z_][a-zA-Z0-9_]*)\\s+(String|Integer|Boolean|Json|Float|Date|DateTime)\\b(\\[\\]|\\?)?',
			captures: {
				1: { name: 'variable.other.property.dinoco' },
				2: { name: 'support.type.primitive.dinoco' },
				3: { name: 'keyword.operator.modifier.dinoco' },
			},
		},
		custom_fields: {
			match: '^\\s*([a-zA-Z_][a-zA-Z0-9_]*)\\s+([A-Za-z][a-zA-Z0-9_]*)\\b(\\[\\]|\\?)?',
			captures: {
				1: { name: 'variable.other.property.dinoco' },
				2: { name: 'constant.language.dinoco' },
				3: { name: 'keyword.operator.modifier.dinoco' },
			},
		},
		model_decorators: {
			patterns: [
				{
					begin: '(@@)(ids)(\\s*\\()',
					beginCaptures: {
						1: { name: 'punctuation.definition.decorator.dinoco' },
						2: { name: 'entity.name.function.decorator.dinoco' },
						3: { name: 'punctuation.section.group.begin.dinoco' },
					},
					end: '(\\))',
					endCaptures: {
						1: { name: 'punctuation.section.group.end.dinoco' },
					},
					patterns: [
						{ match: '(\\[|\\]|,)', name: 'punctuation.separator.array.dinoco' },
						{ match: '\\b([A-Za-z_][A-Za-z0-9_]*)\\b', name: 'constant.language.dinoco' },
					],
				},
				{
					begin: '(@@)(table_name)(\\s*\\()',
					beginCaptures: {
						1: { name: 'punctuation.definition.decorator.dinoco' },
						2: { name: 'entity.name.function.decorator.dinoco' },
						3: { name: 'punctuation.section.group.begin.dinoco' },
					},
					end: '(\\))',
					endCaptures: {
						1: { name: 'punctuation.section.group.end.dinoco' },
					},
					patterns: [{ include: '#strings' }],
				},
				{
					match: '(@@)([a-zA-Z_][a-zA-Z0-9_]*)',
					captures: {
						1: { name: 'punctuation.definition.decorator.dinoco' },
						2: { name: 'entity.name.function.decorator.dinoco' },
					},
				},
			],
		},
		decorators: {
			match: '(@)([a-zA-Z_][a-zA-Z0-9_]*)',
			captures: {
				1: { name: 'punctuation.definition.decorator.dinoco' },
				2: { name: 'entity.name.function.decorator.dinoco' },
			},
		},
		named_parameters: { match: '\\b([a-zA-Z_][a-zA-Z0-9_]*)\\s*(?=:)', name: 'variable.parameter' },
		functions: { match: '\\b([a-zA-Z_][a-zA-Z0-9_]*)\\s*(?=\\()', name: 'support.function.dinoco' },
		constants: {
			patterns: [
				{ match: '\\b(Cascade|SetNull|SetDefault)\\b', name: 'constant.language' },
				{ match: '\\b(true|false)\\b', name: 'constant.language.boolean' },
				{ match: '\\b\\d+(\\.\\d+)?\\b', name: 'constant.numeric' },
				{ match: ':\\s*\\[([a-zA-Z0-9_]+)\\]', captures: { 1: { name: 'constant.language' } } },
				{ match: '\\((\\w+)\\)', captures: { 1: { name: 'constant.language' } } },
			],
		},
	},
	scopeName: 'source.dinoco',
};

const markdownComponents = {
	h1: ({ children, className, id, ...props }: MarkdownComponentProps) => (
		<h1 {...props} id={id} className={['mb-6 text-4xl font-extrabold tracking-tight text-slate-900 dark:text-white', className].filter(Boolean).join(' ')}>
			{children}
		</h1>
	),
	h2: ({ children, className, id, ...props }: MarkdownComponentProps) => (
		<h2 {...props} id={id} className={['mt-12 mb-6 scroll-mt-32 text-2xl font-bold tracking-tight text-slate-900 dark:text-white', className].filter(Boolean).join(' ')}>
			{children}
		</h2>
	),
	h3: ({ children, className, id, ...props }: MarkdownComponentProps) => (
		<h3 {...props} id={id} className={['mt-8 mb-4 scroll-mt-32 text-xl font-semibold tracking-tight text-slate-900 dark:text-white', className].filter(Boolean).join(' ')}>
			{children}
		</h3>
	),
	p: ({ children, className, ...props }: MarkdownComponentProps) => (
		<p {...props} className={['mb-6 leading-7 text-slate-600 dark:text-slate-300', className].filter(Boolean).join(' ')}>
			{children}
		</p>
	),
	ul: ({ children, className, ...props }: MarkdownComponentProps) => (
		<ul {...props} className={['mb-6 list-disc space-y-2 pl-6 text-slate-600 marker:text-slate-400 dark:text-slate-300 dark:marker:text-[#242424]', className].filter(Boolean).join(' ')}>
			{children}
		</ul>
	),
	ol: ({ children, className, ...props }: MarkdownComponentProps) => (
		<ol {...props} className={['mb-6 list-decimal space-y-2 pl-6 text-slate-600 marker:text-slate-400 dark:text-slate-300 dark:marker:text-[#242424]', className].filter(Boolean).join(' ')}>
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
				<code
					{...props}
					className="rounded-md border border-[#242424] bg-light-200 px-1.5 py-0.5 font-mono text-[0.875em] font-semibold text-dinoco-deep dark:bg-[#161616] dark:text-dinoco-cyan"
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
	blockquote: ({ children, className, ...props }: MarkdownComponentProps) => (
		<blockquote
			{...props}
			className={['mb-6 rounded-r-lg border-l-4 border-dinoco-brand bg-dinoco-brand/5 px-6 py-4 text-slate-700 dark:border-dinoco-cyan dark:bg-[#161616] dark:text-slate-300', className].filter(Boolean).join(' ')}
		>
			{children}
		</blockquote>
	),
	table: ({ children, className, ...props }: MarkdownComponentProps) => (
		<div className="mb-6 overflow-x-auto rounded-xl border border-light-300 bg-white shadow-sm dark:border-[#242424] dark:bg-[#0c0c0c]">
			<table {...props} className={['min-w-full border-collapse text-left text-sm text-slate-600 dark:text-slate-300', className].filter(Boolean).join(' ')}>
				{children}
			</table>
		</div>
	),
	thead: ({ children, className, ...props }: MarkdownComponentProps) => (
		<thead {...props} className={['bg-light-100 dark:bg-[#111111]', className].filter(Boolean).join(' ')}>
			{children}
		</thead>
	),
	tbody: ({ children, className, ...props }: MarkdownComponentProps) => (
		<tbody {...props} className={className}>
			{children}
		</tbody>
	),
	tr: ({ children, className, ...props }: MarkdownComponentProps) => (
		<tr {...props} className={['border-t border-light-300 dark:border-[#242424]', className].filter(Boolean).join(' ')}>
			{children}
		</tr>
	),
	th: ({ children, className, ...props }: MarkdownComponentProps) => (
		<th {...props} className={['border-l border-light-300 px-4 py-3 font-semibold tracking-tight text-slate-900 first:border-l-0 dark:border-[#242424] dark:text-white', className].filter(Boolean).join(' ')}>
			{children}
		</th>
	),
	td: ({ children, className, ...props }: MarkdownComponentProps) => (
		<td {...props} className={['border-l border-light-300 px-4 py-3 align-top first:border-l-0 dark:border-[#242424]', className].filter(Boolean).join(' ')}>
			{children}
		</td>
	),
	a: ({ children, className, ...props }: React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
		<a
			{...props}
			className={['cursor-pointer font-medium text-dinoco-brand underline decoration-dinoco-brand/30 underline-offset-4 hover:decoration-dinoco-brand dark:text-dinoco-cyan dark:decoration-dinoco-cyan/30 dark:hover:decoration-dinoco-cyan', className]
				.filter(Boolean)
				.join(' ')}
		>
			{children}
		</a>
	),
};

function toAnchorId(value: string): string {
	return value.toLowerCase().split(' ').join('-');
}

function visitHeadings(node: unknown): void {
	if (typeof node !== 'object' || node === null || !('type' in node)) {
		return;
	}

	const typedNode = node as {
		children?: unknown[];
		properties?: Record<string, unknown>;
		tagName?: string;
		type: string;
		value?: string;
	};

	if (typedNode.type === 'element' && ['h1', 'h2', 'h3'].includes(typedNode.tagName ?? '')) {
		const text = extractText(typedNode.children ?? []);

		if (text.length > 0) {
			typedNode.properties = {
				...(typedNode.properties ?? {}),
				id: toAnchorId(text),
			};
		}
	}

	for (const child of typedNode.children ?? []) {
		visitHeadings(child);
	}
}

function extractText(children: unknown[]): string {
	return children
		.map(child => {
			if (typeof child !== 'object' || child === null || !('type' in child)) {
				return '';
			}

			const typedChild = child as {
				children?: unknown[];
				type: string;
				value?: string;
			};

			if (typedChild.type === 'text') {
				return typedChild.value ?? '';
			}

			return extractText(typedChild.children ?? []);
		})
		.join('')
		.trim();
}

async function readMarkdownFile(contentPath: string): Promise<string> {
	const filePath = path.join(process.cwd(), 'src', 'content', contentPath);

	return fs.readFile(filePath, 'utf8');
}

const MarkdownContent = async ({ contentPath }: MarkdownContentProps): Promise<React.JSX.Element> => {
	const source = await readMarkdownFile(contentPath);
	const processor = unified()
		.use(remarkParse)
		.use(remarkGfm)
		.use(remarkRehype)
		.use(rehypeShiki, {
			addLanguageClass: true,
			defaultLanguage: 'txt',
			fallbackLanguage: 'txt',
			langs: [
				'bash',
				'rust',
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
	const hastTree = (await processor.run(markdownTree, {
		path: contentPath,
		value: source,
	})) as Root;

	visitHeadings(hastTree);

	const content = toJsxRuntime(hastTree, {
		Fragment: jsxRuntime.Fragment,
		components: markdownComponents,
		jsx: jsxRuntime.jsx,
		jsxs: jsxRuntime.jsxs,
	});

	return <div className="w-full">{content}</div>;
};

export default MarkdownContent;
