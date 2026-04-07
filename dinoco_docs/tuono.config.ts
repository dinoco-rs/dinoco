import type { TuonoConfig } from 'tuono/config';

import mdx from '@mdx-js/rollup';
import { common } from 'lowlight';
import rehypeHighlight from 'rehype-highlight';

import tailwindcss from '@tailwindcss/vite';

import dinocoHighlight from './src/jsons/dinocoHighlight';

const config: TuonoConfig = {
	vite: {
		plugins: [
			mdx({
				rehypePlugins: [
					[
						rehypeHighlight,
						{
							languages: {
								...common,
								dinoco: dinocoHighlight,
							},
							aliases: {
								dinoco: ['dinoco'],
							},
						},
					],
				],
			}),
			tailwindcss(),
		],
	},
};

export default config;
