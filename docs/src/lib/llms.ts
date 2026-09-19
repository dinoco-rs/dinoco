import { SITE_URL } from './site';
import { absolutizeLinks, loadDocsIndex } from './docs-index';

import type { DocsLocale } from '../jsons/versions';

const SUMMARY: Record<DocsLocale, string> = {
	'en-us':
		'Dinoco is a schema-driven ORM for Rust. You describe your data in `schema.dinoco`; Dinoco generates typed models, builds migrations (automatic or hand-written in Rust), and provides type-safe queries for PostgreSQL, MySQL, and SQLite.',
	'pt-br':
		'O Dinoco é um ORM para Rust guiado por schema. Você descreve seus dados em `schema.dinoco`; o Dinoco gera models tipados, cria migrations (automáticas ou escritas à mão em Rust) e oferece queries type-safe para PostgreSQL, MySQL e SQLite.',
};

const MCP_NOTE: Record<DocsLocale, string> = {
	'en-us': `An MCP server with search, read, and list tools is available at ${SITE_URL}/mcp.`,
	'pt-br': `Um servidor MCP com ferramentas de busca, leitura e listagem está disponível em ${SITE_URL}/mcp.`,
};

/** `llms.txt`: a curated index of the docs for language models (llmstxt.org format). */
export async function renderLlmsTxt(locale: DocsLocale): Promise<string> {
	const index = (await loadDocsIndex()).filter(entry => entry.locale === locale);
	const groups = [...new Set(index.map(entry => entry.group))];
	const lines = [`# Dinoco`, '', `> ${SUMMARY[locale]}`, '', MCP_NOTE[locale], `Full text of every page: ${SITE_URL}/llms-full.txt${locale === 'en-us' ? '' : `?locale=${locale}`}`, ''];

	for (const group of groups) {
		const pages = index.filter(entry => entry.group === group);

		lines.push(`## ${pages[0].groupName}`, '');
		for (const page of pages) {
			lines.push(`- [${page.title}](${page.url}): ${page.description}`);
		}
		lines.push('');
	}

	return `${lines.join('\n').trimEnd()}\n`;
}

/** `llms-full.txt`: every page's Markdown in one document. */
export async function renderLlmsFullTxt(locale: DocsLocale): Promise<string> {
	const index = (await loadDocsIndex()).filter(entry => entry.locale === locale);
	const parts = [`# Dinoco documentation\n\n> ${SUMMARY[locale]}\n`];

	for (const page of index) {
		parts.push(`<!-- ${page.groupName} / ${page.title} -->\nSource: ${page.url}\n\n${absolutizeLinks(page.markdown).trim()}\n`);
	}

	return `${parts.join('\n---\n\n')}\n`;
}
