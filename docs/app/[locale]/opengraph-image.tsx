import { ImageResponse } from 'next/og';

export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';

const taglineByLocale: Record<string, string> = {
	'en-us': 'Open-source tools for building fast, reliable software.',
	'pt-br': 'Ferramentas open-source para construir software rápido e confiável.',
};

export default async function OpengraphImage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const tagline = taglineByLocale[locale] ?? taglineByLocale['en-us'];

	return new ImageResponse(
		(
			<div
				style={{
					alignItems: 'center',
					background: '#050505',
					display: 'flex',
					flexDirection: 'column',
					height: '100%',
					justifyContent: 'center',
					width: '100%',
				}}
			>
				<div style={{ color: '#ffffff', display: 'flex', fontSize: 120, fontWeight: 700 }}>Dinoco</div>
				<div style={{ color: '#00ffff', display: 'flex', fontSize: 32, marginTop: 24, maxWidth: 820, textAlign: 'center' }}>{tagline}</div>
			</div>
		),
		{ ...size },
	);
}
