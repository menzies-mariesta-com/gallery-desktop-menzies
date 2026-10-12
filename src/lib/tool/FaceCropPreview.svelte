<script lang="ts">
	import { imageSrc } from '$lib/tool/gallery-ipc';
	import type { FacePreview } from '$lib/tool/face-index';

	let {
		preview,
		alt,
		shape = 'circle',
		class: className = ''
	}: {
		preview: FacePreview | null;
		alt: string;
		shape?: 'circle' | 'square';
		class?: string;
	} = $props();

	const crop = $derived.by(() => {
		if (!preview) return null;
		const src = imageSrc(preview.path);
		const imgW = preview.imageWidth;
		const imgH = preview.imageHeight;
		const [bx, by, bw, bh] = preview.bbox;
		if (imgW < 8 || imgH < 8 || bw < 1 || bh < 1) {
			return { src, mode: 'cover' as const };
		}
		const pad = 0.35;
		const side = Math.max(8, Math.max(bw, bh) * (1 + pad));
		const cx = bx + bw / 2;
		const cy = by + bh / 2;
		const left = Math.max(0, Math.min(imgW - side, cx - side / 2));
		const top = Math.max(0, Math.min(imgH - side, cy - side / 2));
		const size = Math.max(1, Math.min(side, imgW - left, imgH - top));
		return {
			src,
			mode: 'bbox' as const,
			widthPct: (imgW / size) * 100,
			heightPct: (imgH / size) * 100,
			leftPct: (-left / size) * 100,
			topPct: (-top / size) * 100
		};
	});
</script>

{#if crop}
	<div
		class="bg-base-200 relative overflow-hidden {shape === 'circle'
			? 'rounded-full'
			: 'rounded-box'} {className}"
	>
		{#if crop.mode === 'cover'}
			<img
				src={crop.src}
				{alt}
				draggable="false"
				loading="lazy"
				class="pointer-events-none size-full object-cover"
			/>
		{:else}
			<img
				src={crop.src}
				{alt}
				draggable="false"
				loading="lazy"
				class="pointer-events-none absolute max-w-none"
				style:width="{crop.widthPct}%"
				style:height="{crop.heightPct}%"
				style:left="{crop.leftPct}%"
				style:top="{crop.topPct}%"
			/>
		{/if}
	</div>
{:else}
	<div
		class="bg-base-200 text-base-content/40 flex items-center justify-center {shape === 'circle'
			? 'rounded-full'
			: 'rounded-box'} {className}"
		aria-hidden="true"
	></div>
{/if}
