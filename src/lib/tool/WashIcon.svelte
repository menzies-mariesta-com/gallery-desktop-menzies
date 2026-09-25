<script lang="ts">
	import type { WashIconNode } from '$lib/tool/wash-icon-node';

	type Props = {
		icon: WashIconNode;
		class?: string;
		strokeWidth?: number | string;
		/** Accessible name when the icon is not decorative. Prefer aria-hidden for button children. */
		title?: string;
	};

	let { icon, class: className = 'size-4', strokeWidth = 2, title }: Props = $props();

	function attrsFor(raw: Record<string, string>): Record<string, string> {
		const out: Record<string, string> = {};
		for (const [key, value] of Object.entries(raw)) {
			if (key === 'key') continue;
			out[key] = value;
		}
		return out;
	}
</script>

<svg
	xmlns="http://www.w3.org/2000/svg"
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
	class="lucide {className}"
	aria-hidden={title ? undefined : 'true'}
	role={title ? 'img' : undefined}
>
	{#if title}
		<title>{title}</title>
	{/if}
	{#each icon as [tag, raw], i (raw.key ?? `${tag}-${i}`)}
		{@const attrs = attrsFor(raw)}
		{#if tag === 'path'}
			<path {...attrs} />
		{:else if tag === 'circle'}
			<circle {...attrs} />
		{:else if tag === 'rect'}
			<rect {...attrs} />
		{:else if tag === 'line'}
			<line {...attrs} />
		{:else if tag === 'polyline'}
			<polyline {...attrs} />
		{:else if tag === 'polygon'}
			<polygon {...attrs} />
		{:else if tag === 'ellipse'}
			<ellipse {...attrs} />
		{/if}
	{/each}
</svg>
