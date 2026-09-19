import { isDocsLocale } from '../../src/lib/docs-index';
import { renderLlmsTxt } from '../../src/lib/llms';

export const dynamic = 'force-dynamic';

export async function GET(request: Request): Promise<Response> {
	const requested = new URL(request.url).searchParams.get('locale');
	const locale = isDocsLocale(requested) ? requested : 'en-us';

	return new Response(await renderLlmsTxt(locale), {
		headers: { 'Cache-Control': 'public, max-age=3600', 'Content-Type': 'text/plain; charset=utf-8' },
	});
}
