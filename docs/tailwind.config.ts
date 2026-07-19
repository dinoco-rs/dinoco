// tailwind.config.ts
import type { Config } from 'tailwindcss';

export default {
	content: ['./src/**/*.tsx', './src/**/*.ts'],
	theme: {
		extend: {
			colors: {
				dinoco: {
					DEFAULT: '#00A2E8',
					light: '#4DC4F4',
					dark: '#007BB5',
					neon: '#00FFFF',
				},
				dark: {
					bg: '#050505',
					surface: '#121212',
					border: '#27272A',
				},
			},
		},
	},
	plugins: [],
} satisfies Config;
