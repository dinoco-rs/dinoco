declare module '*.mdx' {
	const component: import('react').ComponentType<{
		components?: Record<string, import('react').ElementType>;
	}>;

	export default component;
}
