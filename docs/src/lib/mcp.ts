import { SITE_URL } from './site';
import { absolutizeLinks, extractSection, isDocsLocale, loadDocsIndex, searchDocs } from './docs-index';

import type { DocsPageEntry } from './docs-index';
import type { DocsLocale } from '../jsons/versions';

/**
 * A small, dependency-free Model Context Protocol server for the Dinoco docs,
 * served over Streamable HTTP in its stateless JSON mode: every POST carries
 * one JSON-RPC message (or a batch) and gets the JSON response back. It is
 * read-only and exposes public content only.
 */
const SERVER_INFO = { name: 'dinoco-docs', title: 'Dinoco documentation', version: '1.0.0' };
const SUPPORTED_PROTOCOLS = ['2025-06-18', '2025-03-26', '2024-11-05'];
const RESOURCE_PREFIX = 'dinoco://docs/';

const INSTRUCTIONS = [
	'Documentation for Dinoco, a schema-driven Rust ORM (schema.dinoco -> generated models, typed queries, migrations).',
	'Start with search_docs to find pages, then get_doc to read one. Use list_docs to see the whole outline.',
	'Pass locale "en-us" (default) or "pt-br". Page ids look like "orm/find-many" (group/item) or "guide/writing-migrations" (group/item/subitem).',
	'Docs cover: schema and config, queries (find, insert, update, delete, count, exists, where_complex), relations, manual migrations, code transforms (dinoco/transform.rs), and the CLI.',
].join(' ');

type JsonRpcId = string | number | null;
type JsonRpcRequest = { id?: JsonRpcId; jsonrpc?: string; method?: string; params?: Record<string, unknown> };
type JsonRpcResponse = { error?: { code: number; data?: unknown; message: string }; id: JsonRpcId; jsonrpc: '2.0'; result?: unknown };

class RpcError extends Error {
	constructor(
		readonly code: number,
		message: string,
	) {
		super(message);
	}
}

const INVALID_PARAMS = -32602;
const METHOD_NOT_FOUND = -32601;
const INVALID_REQUEST = -32600;

const localeProperty = {
	default: 'en-us',
	description: 'Documentation language.',
	enum: ['en-us', 'pt-br'],
	type: 'string',
};

const TOOLS = [
	{
		annotations: { openWorldHint: false, readOnlyHint: true, title: 'Search the Dinoco docs' },
		description: 'Full-text search over the Dinoco documentation. Returns the best matching pages with a snippet each. Use it first when you do not know which page covers a topic.',
		inputSchema: {
			properties: {
				group: { description: 'Restrict to one group: guide, orm, tooling, or reference.', type: 'string' },
				limit: { default: 8, description: 'Maximum results (1-25).', maximum: 25, minimum: 1, type: 'integer' },
				locale: localeProperty,
				query: { description: 'What to look for, e.g. "where_complex count" or "rollback manual migration".', type: 'string' },
			},
			required: ['query'],
			type: 'object',
		},
		name: 'search_docs',
	},
	{
		annotations: { openWorldHint: false, readOnlyHint: true, title: 'Read a Dinoco docs page' },
		description: 'Returns one documentation page as Markdown. Pass the page id from search_docs/list_docs (for example "orm/find-many" or "guide/transforms-recipes"), or the page URL. Optionally return only one section by heading.',
		inputSchema: {
			properties: {
				id: { description: 'Page id (group/item) or a full page URL.', type: 'string' },
				locale: localeProperty,
				section: { description: 'Return only the section under this heading (e.g. "Pagination").', type: 'string' },
			},
			required: ['id'],
			type: 'object',
		},
		name: 'get_doc',
	},
	{
		annotations: { openWorldHint: false, readOnlyHint: true, title: 'List Dinoco docs pages' },
		description: 'Lists every documentation page (id, title, description, URL), grouped by area. Use it to discover what exists.',
		inputSchema: {
			properties: {
				group: { description: 'Only this group.', type: 'string' },
				locale: localeProperty,
			},
			type: 'object',
		},
		name: 'list_docs',
	},
];

function text(value: string, structuredContent?: unknown) {
	return { content: [{ text: value, type: 'text' }], ...(structuredContent === undefined ? {} : { structuredContent }) };
}

function toolError(message: string) {
	return { content: [{ text: message, type: 'text' }], isError: true };
}

function localeArgument(args: Record<string, unknown>): DocsLocale {
	if (args.locale === undefined) {
		return 'en-us';
	}

	if (!isDocsLocale(args.locale)) {
		throw new RpcError(INVALID_PARAMS, 'locale must be "en-us" or "pt-br"');
	}

	return args.locale;
}

function stringArgument(args: Record<string, unknown>, key: string, required = false): string | undefined {
	const value = args[key];

	if (value === undefined || value === null) {
		if (required) {
			throw new RpcError(INVALID_PARAMS, `${key} is required`);
		}

		return undefined;
	}

	if (typeof value !== 'string' || (required && value.trim() === '')) {
		throw new RpcError(INVALID_PARAMS, `${key} must be a non-empty string`);
	}

	return value;
}

function summarize(entry: DocsPageEntry) {
	return { description: entry.description, group: entry.group, id: entry.id, title: entry.title, url: entry.url };
}

function findPage(index: DocsPageEntry[], locale: DocsLocale, id: string): DocsPageEntry | undefined {
	const cleaned = id
		.trim()
		.replace(SITE_URL, '')
		.replace(/[?#].*$/, '')
		.replace(/^\/(en-us|pt-br)\/docs\/orm\//, '')
		.replace(/^\/+|\/+$/g, '');

	return index.find(entry => entry.locale === locale && entry.id === cleaned);
}

async function callTool(name: string, args: Record<string, unknown>) {
	const index = await loadDocsIndex();
	const locale = localeArgument(args);

	if (name === 'search_docs') {
		const query = stringArgument(args, 'query', true) as string;
		const limit = typeof args.limit === 'number' ? Math.min(Math.max(Math.trunc(args.limit), 1), 25) : 8;
		const hits = searchDocs(index, query, { group: stringArgument(args, 'group'), limit, locale });

		if (hits.length === 0) {
			return text(`No documentation pages matched "${query}". Try fewer or different keywords, or call list_docs.`, { results: [] });
		}

		const results = hits.map(hit => ({ ...summarize(hit.entry), snippet: hit.snippet }));
		const body = results.map((result, position) => `${position + 1}. ${result.title} (${result.group}) - id: ${result.id}\n   ${result.url}\n   ${result.snippet}`).join('\n');

		return text(body, { results });
	}

	if (name === 'get_doc') {
		const id = stringArgument(args, 'id', true) as string;
		const entry = findPage(index, locale, id);

		if (entry === undefined) {
			return toolError(`No page with id "${id}" for locale ${locale}. Use search_docs or list_docs to find valid ids.`);
		}

		const section = stringArgument(args, 'section');
		let markdown = entry.markdown;

		if (section !== undefined) {
			const found = extractSection(entry.markdown, section);

			if (found === undefined) {
				return toolError(`Page "${entry.id}" has no section "${section}". Available sections: ${entry.headings.join('; ')}`);
			}

			markdown = found;
		}

		return text(`Source: ${entry.url}\n\n${absolutizeLinks(markdown)}`, { ...summarize(entry), sections: entry.headings });
	}

	if (name === 'list_docs') {
		const group = stringArgument(args, 'group');
		const pages = index.filter(entry => entry.locale === locale && (group === undefined || entry.group === group));
		const groups = [...new Set(pages.map(entry => entry.group))];
		const body = groups
			.map(groupId => {
				const inGroup = pages.filter(entry => entry.group === groupId);

				return `## ${inGroup[0].groupName} (${groupId})\n${inGroup.map(entry => `- ${entry.id}: ${entry.title} - ${entry.description}`).join('\n')}`;
			})
			.join('\n\n');

		return text(body === '' ? 'No pages found.' : body, { pages: pages.map(summarize) });
	}

	throw new RpcError(INVALID_PARAMS, `Unknown tool: ${name}`);
}

async function listResources() {
	const index = await loadDocsIndex();

	return index.map(entry => ({
		description: entry.description,
		mimeType: 'text/markdown',
		name: `${entry.groupName}: ${entry.title}`,
		title: entry.title,
		uri: `${RESOURCE_PREFIX}${entry.locale}/${entry.id}`,
	}));
}

async function readResource(uri: string) {
	const index = await loadDocsIndex();

	if (!uri.startsWith(RESOURCE_PREFIX)) {
		throw new RpcError(INVALID_PARAMS, `Unknown resource: ${uri}`);
	}

	const [locale, ...rest] = uri.slice(RESOURCE_PREFIX.length).split('/');
	const entry = isDocsLocale(locale) ? findPage(index, locale, rest.join('/')) : undefined;

	if (entry === undefined) {
		throw new RpcError(-32002, `Resource not found: ${uri}`);
	}

	return { contents: [{ mimeType: 'text/markdown', text: absolutizeLinks(entry.markdown), uri }] };
}

async function dispatch(request: JsonRpcRequest): Promise<unknown> {
	const params = request.params ?? {};

	switch (request.method) {
		case 'initialize': {
			const requested = typeof params.protocolVersion === 'string' ? params.protocolVersion : undefined;

			return {
				capabilities: { resources: { listChanged: false, subscribe: false }, tools: { listChanged: false } },
				instructions: INSTRUCTIONS,
				protocolVersion: requested !== undefined && SUPPORTED_PROTOCOLS.includes(requested) ? requested : SUPPORTED_PROTOCOLS[0],
				serverInfo: SERVER_INFO,
			};
		}
		case 'ping':
			return {};
		case 'tools/list':
			return { tools: TOOLS };
		case 'tools/call': {
			const name = params.name;
			const args = params.arguments;

			if (typeof name !== 'string') {
				throw new RpcError(INVALID_PARAMS, 'name is required');
			}

			try {
				return await callTool(name, typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {});
			} catch (error) {
				// Bad arguments are reported to the model as a tool error so it can retry;
				// only unknown tools stay protocol errors.
				if (error instanceof RpcError && error.code === INVALID_PARAMS && !error.message.startsWith('Unknown tool')) {
					return toolError(error.message);
				}

				throw error;
			}
		}
		case 'resources/list':
			return { resources: await listResources() };
		case 'resources/templates/list':
			return {
				resourceTemplates: [
					{ description: 'A Dinoco documentation page as Markdown.', mimeType: 'text/markdown', name: 'docs-page', uriTemplate: `${RESOURCE_PREFIX}{locale}/{group}/{item}` },
				],
			};
		case 'resources/read':
			if (typeof params.uri !== 'string') {
				throw new RpcError(INVALID_PARAMS, 'uri is required');
			}

			return readResource(params.uri);
		default:
			throw new RpcError(METHOD_NOT_FOUND, `Method not found: ${request.method ?? '(none)'}`);
	}
}

async function handleOne(request: unknown): Promise<JsonRpcResponse | undefined> {
	if (typeof request !== 'object' || request === null || Array.isArray(request)) {
		return { error: { code: INVALID_REQUEST, message: 'Invalid Request' }, id: null, jsonrpc: '2.0' };
	}

	const message = request as JsonRpcRequest;
	const isNotification = message.id === undefined;

	if (message.jsonrpc !== '2.0' || typeof message.method !== 'string') {
		return isNotification ? undefined : { error: { code: INVALID_REQUEST, message: 'Invalid Request' }, id: message.id ?? null, jsonrpc: '2.0' };
	}

	// Notifications (e.g. notifications/initialized) never get a response.
	if (isNotification || message.method.startsWith('notifications/')) {
		return undefined;
	}

	try {
		return { id: message.id ?? null, jsonrpc: '2.0', result: await dispatch(message) };
	} catch (error) {
		if (error instanceof RpcError) {
			return { error: { code: error.code, message: error.message }, id: message.id ?? null, jsonrpc: '2.0' };
		}

		return { error: { code: -32603, message: 'Internal error' }, id: message.id ?? null, jsonrpc: '2.0' };
	}
}

/** Handles a parsed JSON-RPC body. Returns `undefined` when there is nothing to send back (notifications only). */
export async function handleMcpBody(body: unknown): Promise<JsonRpcResponse | JsonRpcResponse[] | undefined> {
	if (Array.isArray(body)) {
		if (body.length === 0) {
			return { error: { code: INVALID_REQUEST, message: 'Invalid Request' }, id: null, jsonrpc: '2.0' };
		}

		const responses = (await Promise.all(body.map(handleOne))).filter((response): response is JsonRpcResponse => response !== undefined);

		return responses.length === 0 ? undefined : responses;
	}

	return handleOne(body);
}

export const MCP_SERVER_CARD = {
	description: 'Read-only MCP server for the Dinoco documentation: search, read, and list pages in English and Portuguese.',
	endpoint: `${SITE_URL}/mcp`,
	name: SERVER_INFO.name,
	protocol: 'Streamable HTTP (stateless JSON)',
	tools: TOOLS.map(tool => ({ description: tool.description, name: tool.name })),
};
