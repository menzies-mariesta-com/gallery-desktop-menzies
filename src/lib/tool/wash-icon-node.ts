/**
 * Leaf type for Wash Lucide SVG element tuples.
 * Kept separate from `wash-icons.ts` so `WashIcon.svelte` does not import the
 * icon data module (avoids TDZ / circular init with SvelteKit's `component` binding).
 */
export type WashIconNode = ReadonlyArray<readonly [string, Record<string, string>]>;
