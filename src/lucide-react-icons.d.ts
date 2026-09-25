/**
 * Ambient types for deep lucide-react icon modules.
 * Wash UI pins lucide-react 1.28.0; each icon file exports `__iconNode` for SVG tuples.
 */
declare module 'lucide-react/dist/esm/icons/*.mjs' {
	import type { WashIconNode } from '$lib/tool/wash-icon-node';

	export const __iconNode: WashIconNode;
	const Icon: unknown;
	export default Icon;
}
