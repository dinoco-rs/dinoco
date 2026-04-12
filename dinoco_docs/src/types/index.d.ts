import type React from 'react';
import type { DocsGroup, DocsItem, DocsLocale, DocsNavigationItem, DocsSection } from '../jsons/versions';

export type DocsSidebarProps = {
	currentGroup: DocsGroup;
	currentItem: DocsItem;
	locale: DocsLocale;
	currentVersionName: string;
	sections: DocsSection[];
	isOpen: boolean;
	onClose: () => void;
};

export type DropdownButtonProps = {
	isOpen: boolean;
	children: React.ReactNode;
	onClick: () => void;
	className?: string;
};

export type DropdownItemProps = {
	isActive: boolean;
	children: React.ReactNode;
	onClick: () => void;
};

export type MarkdownContentProps = {
	contentPath: string;
};

export type MarkdownComponentProps = React.HTMLAttributes<HTMLElement> & {
	children?: React.ReactNode;
};

export type MarkdownCodeProps = React.HTMLAttributes<HTMLElement> & {
	children?: React.ReactNode;
	className?: string;
};

export type HeaderProps = {
	onMenuToggle: () => void;
};

export type DocsContentNavigationProps = {
	next?: DocsNavigationItem;
	previous?: DocsNavigationItem;
};
