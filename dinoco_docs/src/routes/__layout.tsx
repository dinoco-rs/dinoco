import { useEffect, type JSX } from 'react';
import { TuonoScripts } from 'tuono';
import type { TuonoLayoutProps } from 'tuono';

// import 'highlight.js/styles/base16/dracula.css';

import '../styles/global.css';
import { getSystemTheme, useDocs } from '../hooks/useDocs';

export default function RootLayout({ children }: TuonoLayoutProps): JSX.Element {
	const setTheme = useDocs(state => state.setTheme);

	useEffect(() => {
		console.log(getSystemTheme());

		setTheme(getSystemTheme());
	}, []);

	return (
		<html lang="pt-BR" className="font-montserrat" suppressHydrationWarning>
			<head>
				<meta name="viewport" content="width=device-width, initial-scale=1" />
				<title>Dinoco Docs</title>

				<link rel="preconnect" href="https://fonts.googleapis.com" />
				<link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
				<link href="https://fonts.googleapis.com/css2?family=Bungee&family=Montserrat:ital,wght@0,100..900;1,100..900&display=swap" rel="stylesheet" />

				<script
					dangerouslySetInnerHTML={{
						__html: `
            try {
                const _theme = localStorage.getItem('theme');
                const theme = _theme ? _theme : 'dark';
                
                if (theme === 'dark') {
                    document.documentElement.classList.add('dark');
                    document.documentElement.classList.remove('light');
                } else {
                    document.documentElement.classList.add('light');
                    document.documentElement.classList.remove('dark');
                }
            } catch (e) {}
        `,
					}}
				/>
			</head>

			<body>
				<main>{children}</main>

				<TuonoScripts />
			</body>
		</html>
	);
}
