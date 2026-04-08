import type { TuonoConfig } from 'tuono/config';

import mdx from '@mdx-js/rollup';
import rehypeShiki from '@shikijs/rehype';
import tailwindcss from '@tailwindcss/vite';
import remarkGfm from 'remark-gfm';

import dinocoGrammar from '../dinoco_vscode/configs/dinoco.tmLanguage.json';

const config: TuonoConfig = {
	server: {
		host: '0.0.0.0',
		port: 3000,
		origin: 'https://docs.dinoco.io',
	},
	vite: {
		plugins: [
			mdx({
				providerImportSource: '@mdx-js/react',
				remarkPlugins: [remarkGfm],
				rehypePlugins: [
					[
						rehypeShiki,
						{
							themes: {
								light: 'github-light',
								dark: 'github-dark',
							},
							langs: [
								'bash',
								'shellscript',
								'rust',
								'sql',
								'toml',

								{
									...dinocoGrammar,
									name: 'dinoco',
									displayName: 'Dinoco',
								},
							],
							defaultLanguage: 'txt',
							fallbackLanguage: 'txt',
							addLanguageClass: true,
						},
					],
				],
			}),
			tailwindcss(),
		],
	},
};

export default config;
