declare module '*.mdx' {
	const component: import('react').ComponentType<{
		components?: Record<string, import('react').ElementType>;
	}>;
	export const title: string | undefined;

	export default component;
}
