import type { LanguageFn } from 'highlight.js';

const dinocoHighlight: LanguageFn = hljs => {
	const KEYWORDS = {
		keyword: 'model enum config',
		type: 'String Integer Boolean Json Float Date DateTime',
		literal: 'true false Cascade SetNull SetDefault',
	};

	const STRINGS = hljs.QUOTE_STRING_MODE;
	const NUMBERS = hljs.NUMBER_MODE;
	const COMMENTS = hljs.COMMENT('#', '$');

	const DECORATORS = {
		className: 'meta',
		begin: /@@?/,
		end: /[A-Za-z0-9_]+/,
		excludeBegin: false,
	};

	const NAMED_PARAMS = {
		className: 'variable.parameter',
		begin: /[A-Za-z_][A-Za-z0-9_]*/,
		relevance: 0,
		end: /:/,
		excludeEnd: true,
	};

	return {
		name: 'Dinoco',
		aliases: ['dinoco'],
		keywords: KEYWORDS,
		contains: [
			COMMENTS,
			STRINGS,
			NUMBERS,
			DECORATORS,
			{
				className: 'class',
				beginKeywords: 'model enum',
				end: /[{(\s]/,
				excludeEnd: true,
				contains: [hljs.TITLE_MODE],
			},
			{
				className: 'field',
				begin: /^[ \t]*[a-zA-Z_][a-zA-Z0-9_]*/,
				returnBegin: true,
				end: /$/,
				contains: [
					{
						className: 'attr',
						begin: /[a-zA-Z_][a-zA-Z0-9_]*/,
					},
					{
						className: 'type',
						begin: /[ \t]+[A-Za-z][a-zA-Z0-9_]*/,
						keywords: KEYWORDS,
						relevance: 0,
					},
					{
						className: 'operator',
						begin: /\[\]|\?/,
					},
				],
			},
			{
				className: 'title.function',
				begin: /[a-zA-Z_][a-zA-Z0-9_]*(?=\()/,
			},
			{
				begin: /\(/,
				end: /\)/,
				contains: [
					STRINGS,
					NUMBERS,
					NAMED_PARAMS,
					{
						className: 'keyword',
						begin: /Cascade|SetNull|SetDefault/,
					},
				],
			},
		],
	};
};

export default dinocoHighlight;
