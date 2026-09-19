import { NextResponse } from 'next/server';

import { MCP_SERVER_CARD, handleMcpBody } from '../../src/lib/mcp';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

// The server is read-only and serves public documentation, so any origin may call it.
const CORS_HEADERS = {
	'Access-Control-Allow-Headers': 'Content-Type, Accept, Mcp-Session-Id, MCP-Protocol-Version, Authorization',
	'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
	'Access-Control-Allow-Origin': '*',
	'Access-Control-Max-Age': '86400',
};

export function OPTIONS(): NextResponse {
	return new NextResponse(null, { headers: CORS_HEADERS, status: 204 });
}

/** Browsers and crawlers get a description of the server; MCP clients use POST. */
export function GET(): NextResponse {
	return NextResponse.json(MCP_SERVER_CARD, { headers: CORS_HEADERS });
}

export async function POST(request: Request): Promise<NextResponse> {
	let body: unknown;

	try {
		body = await request.json();
	} catch {
		return NextResponse.json({ error: { code: -32700, message: 'Parse error' }, id: null, jsonrpc: '2.0' }, { headers: CORS_HEADERS, status: 400 });
	}

	const response = await handleMcpBody(body);

	// Only notifications/responses were sent: acknowledge without a body.
	if (response === undefined) {
		return new NextResponse(null, { headers: CORS_HEADERS, status: 202 });
	}

	return NextResponse.json(response, { headers: CORS_HEADERS });
}
