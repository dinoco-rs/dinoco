import type { Blockquote, Paragraph, Root, Text } from 'mdast';

export const CALLOUT_TYPES = ['note', 'tip', 'warning', 'danger'] as const;
export type CalloutType = (typeof CALLOUT_TYPES)[number];

type BlockquoteWithCalloutData = Blockquote & {
	data?: {
		hName?: string;
		hProperties?: Record<string, string>;
	};
};

const MARKER_PATTERN = /^\[!(NOTE|TIP|WARNING|DANGER)\]\s*\n?/i;

function isParentNode(node: unknown): node is { children: unknown[] } {
	return typeof node === 'object' && node !== null && Array.isArray((node as { children?: unknown }).children);
}

function forEachBlockquote(node: unknown, visit: (blockquote: Blockquote) => void): void {
	if (!isParentNode(node)) {
		return;
	}

	for (const child of node.children) {
		if (typeof child === 'object' && child !== null && (child as { type?: string }).type === 'blockquote') {
			visit(child as Blockquote);
		}

		forEachBlockquote(child, visit);
	}
}

/**
 * Turns GitHub-style alert blockquotes (`> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`,
 * `> [!DANGER]`) into a `<blockquote data-callout="...">`, so the markdown
 * renderer can style each kind distinctly instead of treating every
 * blockquote as a generic quote.
 */
export function remarkCallouts() {
	return (tree: Root): void => {
		forEachBlockquote(tree, blockquote => {
			const firstParagraph = blockquote.children[0];

			if (firstParagraph === undefined || firstParagraph.type !== 'paragraph') {
				return;
			}

			const firstChild = (firstParagraph as Paragraph).children[0];

			if (firstChild === undefined || firstChild.type !== 'text') {
				return;
			}

			const match = MARKER_PATTERN.exec((firstChild as Text).value);

			if (match === null) {
				return;
			}

			const calloutType = match[1].toLowerCase() as CalloutType;
			const remainder = (firstChild as Text).value.slice(match[0].length);

			if (remainder.length === 0) {
				(firstParagraph as Paragraph).children.shift();
			} else {
				(firstChild as Text).value = remainder;
			}

			(blockquote as BlockquoteWithCalloutData).data = {
				hName: 'blockquote',
				hProperties: { 'data-callout': calloutType },
			};
		});
	};
}
