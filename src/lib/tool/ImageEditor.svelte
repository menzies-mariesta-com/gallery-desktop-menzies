<script lang="ts">
	import { onMount } from 'svelte';
	import { fade, scale } from 'svelte/transition';
	import { m } from '$lib/paraglide/messages.js';
	import {
		commitImageEdit,
		discardEditPreview,
		forgetImageSrc,
		imageFileInfo,
		imageSrc,
		isTauri,
		previewImageEdit
	} from '$lib/tool/gallery-ipc';
	import {
		applyRotateClick,
		buildImageEditOps,
		editOpsReadyForPreview,
		filterIdSchema,
		galleryErrorCode,
		isRasterName,
		joinSaveAsFileName,
		opsFingerprint,
		outputExtension,
		replaceAllowed,
		rotateCssNudgeDeg,
		rotateFromQuarterTurns,
		suggestSaveAsStem,
		type BuildImageEditOpsInput,
		type FilterId,
		type ImageEditResult,
		type ImageFileInfo,
		type OutputFormat,
		type ResizeMode,
		type RotateClick
	} from '$lib/tool/image-edit';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import WashSelect from '$lib/tool/WashSelect.svelte';
	import { washIcons } from '$lib/tool/wash-icons';

	type Props = {
		sourcePath: string;
		sourceName: string;
		allowedRoot: string;
		onClose: () => void;
		onSaved: (path: string, name: string) => void;
	};

	let { sourcePath, sourceName, allowedRoot, onClose, onSaved }: Props = $props();

	let info = $state<ImageFileInfo | null>(null);
	/** Cumulative clockwise quarter turns (0..3). Left/right/180 clicks increment. */
	let rotateQuarterTurns = $state(0);
	let cropOn = $state(false);
	let cropX = $state('0');
	let cropY = $state('0');
	let cropW = $state('');
	let cropH = $state('');
	let resizeOn = $state(false);
	let resizeW = $state('');
	let resizeH = $state('');
	let resizeMode = $state<ResizeMode>('fitInside');
	let filters = $state<FilterId[]>([]);
	let qualityKind = $state<'keep' | 'reduce' | 'enhance'>('keep');
	let qualityMode = $state<'auto' | 'manual'>('auto');
	let qualityValue = $state('80');
	let maxBytes = $state('');
	let scalePercent = $state('125');
	let sharpen = $state('30');
	let contrast = $state('12');
	let format = $state<OutputFormat>('keep');
	let preview = $state<ImageEditResult | null>(null);
	let busy = $state<'save' | null>(null);
	let autoPreviewing = $state(false);
	let previewQueued = false;
	let confirmOpen = $state(false);
	let saveAsStem = $state('');
	let error = $state<string | null>(null);
	let alive = true;
	let previewSeq = 0;
	/** Gates auto-preview until image info has loaded (or failed). */
	let editorReady = $state(false);
	let reducedMotion = $state(false);
	let panelEntered = $state(false);
	let rotateNudgeDeg = $state(0);
	let rotateNudgeToken = 0;
	let lastPreviewFingerprint = '';
	// sourcePath is fixed for this editor session; live swaps use the preload effect.
	// svelte-ignore state_referenced_locally
	let shownSrc = $state(imageSrc(sourcePath));
	let shownLoadToken = 0;

	const filterOptions = $derived([
		{
			id: 'grayscale' as const,
			label: m.edit_filter_grayscale(),
			icon: washIcons.contrast
		},
		{
			id: 'invert' as const,
			label: m.edit_filter_invert(),
			icon: washIcons['flip-vertical-2']
		},
		{ id: 'sepia' as const, label: m.edit_filter_sepia(), icon: washIcons.blend },
		{ id: 'blur' as const, label: m.edit_filter_blur(), icon: washIcons.droplets },
		{ id: 'sharpen' as const, label: m.edit_filter_sharpen(), icon: washIcons.sparkles },
		{ id: 'brighten' as const, label: m.edit_filter_brighten(), icon: washIcons.sun },
		{ id: 'darken' as const, label: m.edit_filter_darken(), icon: washIcons.moon },
		{ id: 'warm' as const, label: m.edit_filter_warm(), icon: washIcons.flame },
		{ id: 'cool' as const, label: m.edit_filter_cool(), icon: washIcons.snowflake }
	]);

	const rotateActions = $derived([
		{
			click: 'left' as const,
			label: m.edit_rotate_ccw(),
			icon: washIcons['rotate-ccw']
		},
		{
			click: 'right' as const,
			label: m.edit_rotate_cw(),
			icon: washIcons['rotate-cw']
		},
		{
			click: 'half' as const,
			label: m.edit_rotate_180(),
			icon: washIcons['flip-horizontal-2']
		}
	]);

	const formatOptions = $derived([
		{ value: 'keep', label: m.edit_format_keep() },
		{ value: 'jpeg', label: m.edit_format_jpeg() },
		{ value: 'png', label: m.edit_format_png() },
		{ value: 'webp', label: m.edit_format_webp() },
		{ value: 'gif', label: m.edit_format_gif() },
		{ value: 'bmp', label: m.edit_format_bmp() }
	]);

	const qualityOptions = $derived([
		{ value: 'keep', label: m.edit_quality_keep() },
		{ value: 'reduce', label: m.edit_quality_reduce() },
		{ value: 'enhance', label: m.edit_quality_enhance() }
	]);

	const modeOptions = $derived([
		{ value: 'auto', label: m.edit_mode_auto() },
		{ value: 'manual', label: m.edit_mode_manual() }
	]);

	const resizeModeOptions = $derived([
		{ value: 'fitInside', label: m.edit_resize_fit() },
		{ value: 'exact', label: m.edit_resize_exact() }
	]);

	const canReplace = $derived(replaceAllowed(sourceName, format));
	const raster = $derived(isRasterName(sourceName));
	const root = $derived(allowedRoot || null);
	const controlsLocked = $derived(busy === 'save');
	const saveAsExt = $derived(outputExtension(sourceName, format));
	const saveAsExtLabel = $derived(saveAsExt ? `.${saveAsExt}` : '');
	const displaySrc = $derived(
		preview?.disposition === 'preview' ? imageSrc(preview.path) : imageSrc(sourcePath)
	);

	function opsInput(): BuildImageEditOpsInput {
		return {
			rotate: rotateFromQuarterTurns(rotateQuarterTurns),
			cropOn,
			cropX,
			cropY,
			cropW,
			cropH,
			resizeOn,
			resizeW,
			resizeH,
			resizeMode,
			filters,
			qualityKind,
			qualityMode,
			qualityValue,
			maxBytes,
			scalePercent,
			sharpen,
			contrast,
			format
		};
	}

	function messageFor(err: unknown): string {
		switch (galleryErrorCode(err)) {
			case 'target_exists':
				return m.edit_err_target_exists();
			case 'too_large':
				return m.edit_err_too_large();
			case 'crop_out_of_bounds':
				return m.edit_err_crop();
			case 'replace_format_mismatch':
				return m.edit_replace_blocked();
			case 'not_raster':
				return m.edit_not_raster();
			case 'name_extension_mismatch':
				return m.edit_err_extension();
			case 'bad_name':
			case 'name_required':
				return m.edit_err_name();
			default:
				return m.edit_err_generic();
		}
	}

	function noteText(note: string): string | null {
		if (note === 'gif_first_frame') return m.edit_gif_note();
		if (note === 'lossless_quality_ignored') return m.edit_lossless_note();
		if (note === 'enhance_classical') return m.edit_enhance_note();
		if (note === 'reduce_auto') return m.edit_reduce_auto_note();
		if (note === 'target_not_met') return m.edit_target_not_met();
		if (note === 'webp_lossless_only') return m.edit_webp_note();
		return null;
	}

	function bumpRotate(click: RotateClick) {
		if (!raster || controlsLocked) return;
		// left = CCW (ccw90), right = CW (cw90). Matches image::rotate90 = CW.
		rotateQuarterTurns = applyRotateClick(rotateQuarterTurns, click);
		if (reducedMotion) return;
		const token = ++rotateNudgeToken;
		rotateNudgeDeg = rotateCssNudgeDeg(click);
		window.setTimeout(() => {
			if (token === rotateNudgeToken) rotateNudgeDeg = 0;
		}, 200);
	}

	function toggleFilter(id: FilterId) {
		if (filters.includes(id)) {
			filters = filters.filter((item) => item !== id);
			return;
		}
		if (filters.length >= 8) return;
		const parsed = filterIdSchema.safeParse(id);
		if (parsed.success) filters = [...filters, parsed.data];
	}

	function toggleCrop() {
		cropOn = !cropOn;
		if (cropOn && info?.raster) {
			if (!cropW) cropW = String(info.width);
			if (!cropH) cropH = String(info.height);
		}
	}

	function toggleResize() {
		resizeOn = !resizeOn;
		if (resizeOn && info?.raster) {
			if (!resizeW) resizeW = String(info.width);
			if (!resizeH) resizeH = String(info.height);
		}
	}

	async function closeEditor() {
		alive = false;
		previewSeq += 1;
		const path = preview?.disposition === 'preview' ? preview.path : '';
		onClose();
		if (path) {
			forgetImageSrc(path);
			try {
				await discardEditPreview(path);
			} catch {
				/* cache eviction still bounds previews */
			}
		}
	}

	function clearPreviewFallback() {
		const previous = preview?.disposition === 'preview' ? preview.path : '';
		preview = null;
		lastPreviewFingerprint = '';
		if (previous) {
			forgetImageSrc(previous);
			void discardEditPreview(previous);
		}
	}

	function onPreviewImgError() {
		if (!alive) return;
		clearPreviewFallback();
		error = m.edit_err_generic();
	}

	async function safeDiscardPreview(path: string) {
		const activePreview = preview?.disposition === 'preview' ? preview.path : '';
		if (!path || path === activePreview) return;
		forgetImageSrc(path);
		try {
			await discardEditPreview(path);
		} catch {
			/* cache eviction still bounds previews */
		}
	}

	async function runPreview(options?: { quiet?: boolean }) {
		const quiet = options?.quiet === true;
		if (!raster || !isTauri()) return;
		if (busy === 'save') return;
		// Serialize IPC: queue one follow-up for the latest controls.
		if (autoPreviewing) {
			previewQueued = true;
			return;
		}

		const input = opsInput();
		if (!editOpsReadyForPreview(input)) return;

		let ops;
		try {
			ops = buildImageEditOps(input);
		} catch {
			return;
		}

		const fingerprint = opsFingerprint(ops);
		if (fingerprint === lastPreviewFingerprint && preview?.disposition === 'preview') {
			return;
		}

		const seq = ++previewSeq;
		autoPreviewing = true;
		if (!quiet) error = null;

		try {
			const next = await previewImageEdit(sourcePath, root, ops);
			if (!alive || seq !== previewSeq) {
				await safeDiscardPreview(next.path);
				return;
			}
			const previous = preview?.disposition === 'preview' ? preview.path : '';
			preview = next;
			lastPreviewFingerprint = fingerprint;
			if (previous && previous !== next.path) void safeDiscardPreview(previous);
		} catch (err) {
			if (alive && seq === previewSeq) {
				error =
					err instanceof Error && !galleryErrorCode(err) ? m.edit_err_invalid() : messageFor(err);
			}
		} finally {
			if (alive && seq === previewSeq) autoPreviewing = false;
			if (alive && previewQueued) {
				previewQueued = false;
				void runPreview({ quiet: true });
			}
		}
	}

	function openConfirm() {
		if (busy || !raster) return;
		error = null;
		try {
			buildImageEditOps(opsInput());
		} catch {
			error = m.edit_err_invalid();
			return;
		}
		saveAsStem = suggestSaveAsStem(sourceName);
		confirmOpen = true;
	}

	async function commit(disposition: 'replace' | 'saveAsNew') {
		if (busy) return;
		if (disposition === 'replace' && !canReplace) {
			error = m.edit_replace_blocked();
			return;
		}
		const saveAsName =
			disposition === 'saveAsNew' ? joinSaveAsFileName(saveAsStem, saveAsExt) : undefined;
		if (disposition === 'saveAsNew' && (!saveAsName || !saveAsStem.trim())) {
			error = m.edit_err_name();
			return;
		}
		busy = 'save';
		error = null;
		try {
			const ops = buildImageEditOps(opsInput());
			const saved = await commitImageEdit(sourcePath, root, ops, disposition, saveAsName);
			if (!alive) return;
			const previewPath = preview?.disposition === 'preview' ? preview.path : '';
			confirmOpen = false;
			onSaved(saved.path, saved.path.split(/[/\\]/).pop() ?? sourceName);
			if (previewPath) {
				forgetImageSrc(previewPath);
				void discardEditPreview(previewPath);
			}
			alive = false;
		} catch (err) {
			if (alive) error = messageFor(err);
		} finally {
			if (alive) busy = null;
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.stopPropagation();
		if (confirmOpen) {
			if (busy === 'save') return;
			confirmOpen = false;
			return;
		}
		void closeEditor();
	}

	onMount(() => {
		reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		requestAnimationFrame(() => {
			if (alive) panelEntered = true;
		});
		void (async () => {
			if (!isTauri() || !raster) {
				if (alive) editorReady = true;
				return;
			}
			try {
				info = await imageFileInfo(sourcePath, root);
				if (!alive || !info) return;
				cropW = String(info.width);
				cropH = String(info.height);
				if (!resizeW) resizeW = String(info.width);
				if (!resizeH) resizeH = String(info.height);
			} catch (err) {
				if (alive) error = messageFor(err);
			} finally {
				if (alive) editorReady = true;
			}
		})();
		return () => {
			alive = false;
			previewSeq += 1;
		};
	});

	/** Hard-cut preview: keep the prior frame until the next bitmap is decoded. */
	$effect(() => {
		const next = displaySrc;
		if (!next || next === shownSrc) return;
		const token = ++shownLoadToken;
		const probe = new Image();
		probe.decoding = 'async';
		probe.onload = () => {
			if (alive && token === shownLoadToken) shownSrc = next;
		};
		probe.onerror = () => {
			if (alive && token === shownLoadToken) onPreviewImgError();
		};
		probe.src = next;
	});

	$effect(() => {
		const input = opsInput();
		const ready = editorReady;
		if (!ready || !raster || !isTauri() || busy === 'save') return;

		const active =
			input.rotate !== 'none' ||
			input.cropOn ||
			input.resizeOn ||
			input.filters.length > 0 ||
			input.qualityKind !== 'keep' ||
			input.format !== 'keep';
		if (!active) {
			// Controls back to identity: drop preview file and show the source again.
			if (preview?.disposition === 'preview') clearPreviewFallback();
			lastPreviewFingerprint = '';
			return;
		}

		// Aggressive debounce: typing crop/resize fires many control updates.
		const handle = window.setTimeout(() => {
			void runPreview({ quiet: true });
		}, 520);
		return () => window.clearTimeout(handle);
	});
</script>

<svelte:window onkeydown={onKeydown} />

<div
	class="bg-base-100 image-editor-shell fixed inset-0 z-[340] flex flex-col overflow-hidden"
	class:image-editor-shell--ready={panelEntered}
	role="dialog"
	aria-modal="true"
	aria-labelledby="edit-title"
>
	<header class="border-ink-border/40 flex shrink-0 items-center gap-2 border-b px-3 py-2">
		<h2 id="edit-title" class="font-display text-primary min-w-0 flex-1 truncate text-lg font-bold">
			{m.edit_title()}
		</h2>
		{#if autoPreviewing}
			<p class="text-base-content/60 shrink-0 text-xs" aria-live="polite">{m.edit_previewing()}</p>
		{/if}
		<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.edit_close()}>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-secondary image-editor-press cursor-pointer"
				aria-label={m.edit_close()}
				onclick={() => void closeEditor()}
			>
				<WashIcon icon={washIcons.x} class="size-4" />
			</button>
		</div>
	</header>

	<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3 md:flex-row">
		<div
			class="bg-base-300 image-editor-stage relative flex min-h-48 flex-1 items-center justify-center overflow-hidden rounded-lg p-2"
			class:image-editor-stage--motion={!reducedMotion && rotateNudgeDeg !== 0}
			style={rotateNudgeDeg !== 0 ? `transform: rotate(${rotateNudgeDeg}deg)` : undefined}
		>
			<img
				src={shownSrc}
				alt={sourceName}
				class="image-editor-preview absolute inset-2 m-auto max-h-[calc(50vh-1rem)] max-w-[calc(100%-1rem)] object-contain md:max-h-[calc(100%-1rem)]"
				decoding="async"
				onerror={onPreviewImgError}
			/>
		</div>

		<aside class="image-editor-panel flex w-full shrink-0 flex-col md:w-80">
			<div class="image-editor-section space-y-1.5">
				<p class="truncate font-mono text-xs" title={sourceName}>{sourceName}</p>
				{#if info?.raster}
					<p class="text-base-content/70 text-xs">
						{m.edit_dimensions({ width: info.width, height: info.height })}
					</p>
				{/if}
				{#if !raster}
					<p class="text-error text-sm">{m.edit_not_raster()}</p>
				{/if}
			</div>

			<section class="image-editor-section" aria-labelledby="edit-rotate-heading">
				<h3 id="edit-rotate-heading" class="font-display text-base-content text-sm font-semibold">
					{m.edit_rotate()}
				</h3>
				<div class="mt-2 flex flex-wrap gap-1" role="group" aria-label={m.edit_rotate()}>
					{#each rotateActions as action (action.click)}
						<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={action.label}>
							<button
								type="button"
								class="btn btn-ghost btn-square btn-sm btn-secondary image-editor-press cursor-pointer"
								class:cursor-not-allowed={!raster || controlsLocked}
								disabled={!raster || controlsLocked}
								aria-label={action.label}
								onclick={() => bumpRotate(action.click)}
							>
								<WashIcon icon={action.icon} class="size-4" />
							</button>
						</div>
					{/each}
				</div>
			</section>

			<section class="image-editor-section" aria-labelledby="edit-geometry-heading">
				<h3 id="edit-geometry-heading" class="font-display text-base-content text-sm font-semibold">
					{m.edit_geometry()}
				</h3>
				<div class="mt-2 flex flex-wrap gap-1" role="group" aria-label={m.edit_geometry()}>
					<div
						class="tooltip tooltip-bottom"
						class:tooltip-primary={cropOn}
						class:tooltip-secondary={!cropOn}
						data-tip={m.edit_crop()}
					>
						<button
							type="button"
							class="btn btn-square btn-sm image-editor-press cursor-pointer"
							class:btn-primary={cropOn}
							class:btn-ghost={!cropOn}
							class:btn-secondary={!cropOn}
							class:cursor-not-allowed={!raster || controlsLocked}
							disabled={!raster || controlsLocked}
							aria-label={m.edit_crop()}
							aria-pressed={cropOn}
							onclick={toggleCrop}
						>
							<WashIcon icon={washIcons.crop} class="size-4" />
						</button>
					</div>
					<div
						class="tooltip tooltip-bottom"
						class:tooltip-primary={resizeOn}
						class:tooltip-secondary={!resizeOn}
						data-tip={m.edit_resize()}
					>
						<button
							type="button"
							class="btn btn-square btn-sm image-editor-press cursor-pointer"
							class:btn-primary={resizeOn}
							class:btn-ghost={!resizeOn}
							class:btn-secondary={!resizeOn}
							class:cursor-not-allowed={!raster || controlsLocked}
							disabled={!raster || controlsLocked}
							aria-label={m.edit_resize()}
							aria-pressed={resizeOn}
							onclick={toggleResize}
						>
							<WashIcon icon={washIcons.scaling} class="size-4" />
						</button>
					</div>
				</div>

				{#if cropOn}
					<div
						class="image-editor-fields mt-3 grid grid-cols-2 gap-2"
						transition:fade={{ duration: reducedMotion ? 0 : 160 }}
					>
						<label class="text-xs">
							{m.edit_crop_x()}<span
								class="text-error align-top text-sm leading-none"
								aria-hidden="true">*</span
							>
							<input
								class="input input-sm mt-1 w-full cursor-text"
								bind:value={cropX}
								inputmode="numeric"
								required
							/>
						</label>
						<label class="text-xs">
							{m.edit_crop_y()}<span
								class="text-error align-top text-sm leading-none"
								aria-hidden="true">*</span
							>
							<input
								class="input input-sm mt-1 w-full cursor-text"
								bind:value={cropY}
								inputmode="numeric"
								required
							/>
						</label>
						<label class="text-xs">
							{m.edit_crop_w()}<span
								class="text-error align-top text-sm leading-none"
								aria-hidden="true">*</span
							>
							<input
								class="input input-sm mt-1 w-full cursor-text"
								bind:value={cropW}
								inputmode="numeric"
								required
							/>
						</label>
						<label class="text-xs">
							{m.edit_crop_h()}<span
								class="text-error align-top text-sm leading-none"
								aria-hidden="true">*</span
							>
							<input
								class="input input-sm mt-1 w-full cursor-text"
								bind:value={cropH}
								inputmode="numeric"
								required
							/>
						</label>
					</div>
				{/if}

				{#if resizeOn}
					<div
						class="image-editor-fields mt-3 space-y-2"
						transition:fade={{ duration: reducedMotion ? 0 : 160 }}
					>
						<div class="grid grid-cols-2 gap-2">
							<label class="text-xs">
								{m.edit_resize_w()}<span
									class="text-error align-top text-sm leading-none"
									aria-hidden="true">*</span
								>
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={resizeW}
									inputmode="numeric"
									required
								/>
							</label>
							<label class="text-xs">
								{m.edit_resize_h()}<span
									class="text-error align-top text-sm leading-none"
									aria-hidden="true">*</span
								>
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={resizeH}
									inputmode="numeric"
									required
								/>
							</label>
						</div>
						<WashSelect
							options={resizeModeOptions}
							bind:value={resizeMode}
							aria-label={m.edit_resize()}
							class="select-sm"
							disabled={controlsLocked}
						/>
					</div>
				{/if}
			</section>

			<section class="image-editor-section" aria-labelledby="edit-filters-heading">
				<h3 id="edit-filters-heading" class="font-display text-base-content text-sm font-semibold">
					{m.edit_filters()}
				</h3>
				<div class="mt-2 flex flex-wrap gap-1" role="group" aria-label={m.edit_filters()}>
					{#each filterOptions as filter (filter.id)}
						<div
							class="tooltip tooltip-bottom"
							class:tooltip-secondary={filters.includes(filter.id)}
							data-tip={filter.label}
						>
							<button
								type="button"
								class="btn btn-square btn-sm image-editor-press cursor-pointer"
								class:btn-secondary={filters.includes(filter.id)}
								class:btn-ghost={!filters.includes(filter.id)}
								aria-label={filter.label}
								aria-pressed={filters.includes(filter.id)}
								disabled={!raster || controlsLocked}
								onclick={() => toggleFilter(filter.id)}
							>
								<WashIcon icon={filter.icon} class="size-4" />
							</button>
						</div>
					{/each}
				</div>
			</section>

			<section class="image-editor-section space-y-2" aria-labelledby="edit-quality-heading">
				<h3 id="edit-quality-heading" class="font-display text-base-content text-sm font-semibold">
					{m.edit_quality()}
				</h3>
				<WashSelect
					options={qualityOptions}
					bind:value={qualityKind}
					aria-label={m.edit_quality()}
					class="select-sm"
					disabled={!raster || controlsLocked}
				/>
				{#if qualityKind !== 'keep'}
					<div transition:fade={{ duration: reducedMotion ? 0 : 160 }}>
						<WashSelect
							options={modeOptions}
							bind:value={qualityMode}
							aria-label={m.edit_mode_auto()}
							class="select-sm"
							disabled={controlsLocked}
						/>
					</div>
				{/if}
				{#if qualityKind === 'enhance'}
					<p class="text-base-content/70 text-xs leading-relaxed">
						{m.edit_quality_enhance_help()}
					</p>
				{/if}
				{#if qualityKind !== 'keep' && qualityMode === 'manual'}
					<div class="space-y-2" transition:fade={{ duration: reducedMotion ? 0 : 160 }}>
						<label class="text-xs">
							{m.edit_quality_value()}<span
								class="text-error align-top text-sm leading-none"
								aria-hidden="true">*</span
							>
							<input
								class="input input-sm mt-1 w-full cursor-text"
								bind:value={qualityValue}
								inputmode="numeric"
								required
							/>
						</label>
						{#if qualityKind === 'reduce'}
							<label class="text-xs">
								{m.edit_max_bytes()}
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={maxBytes}
									inputmode="numeric"
								/>
							</label>
						{/if}
						{#if qualityKind === 'enhance'}
							<label class="text-xs">
								{m.edit_scale()}
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={scalePercent}
									inputmode="numeric"
								/>
							</label>
							<label class="text-xs">
								{m.edit_sharpen()}
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={sharpen}
									inputmode="numeric"
								/>
							</label>
							<label class="text-xs">
								{m.edit_contrast()}
								<input
									class="input input-sm mt-1 w-full cursor-text"
									bind:value={contrast}
									inputmode="numeric"
								/>
							</label>
						{/if}
					</div>
				{/if}
				{#if format === 'webp' && qualityKind !== 'keep'}
					<p class="text-base-content/70 text-xs">{m.edit_webp_note()}</p>
				{/if}
			</section>

			<section class="image-editor-section space-y-2" aria-labelledby="edit-format-heading">
				<h3 id="edit-format-heading" class="font-display text-base-content text-sm font-semibold">
					{m.edit_format()}
				</h3>
				<WashSelect
					options={formatOptions}
					bind:value={format}
					aria-label={m.edit_format()}
					class="select-sm"
					disabled={!raster || controlsLocked}
				/>
			</section>

			{#if preview}
				<ul class="text-base-content/70 space-y-1 px-0.5 text-xs">
					{#each preview.notes as note (note)}
						{#if noteText(note)}
							<li>{noteText(note)}</li>
						{/if}
					{/each}
				</ul>
			{/if}
			{#if error}
				<p class="text-error text-sm" role="alert">{error}</p>
			{/if}

			<div class="image-editor-section image-editor-section--actions">
				<div class="flex w-full gap-2">
					<button
						type="button"
						class="btn btn-error btn-sm image-editor-press min-w-0 flex-1"
						class:cursor-pointer={busy === null}
						class:cursor-not-allowed={busy !== null}
						class:btn-disabled={busy !== null}
						disabled={busy !== null}
						onclick={() => void closeEditor()}
					>
						{m.edit_discard()}
					</button>
					<button
						type="button"
						class="btn btn-primary btn-sm image-editor-press min-w-0 flex-1"
						class:cursor-pointer={raster && busy === null}
						class:cursor-not-allowed={!raster || busy !== null}
						class:btn-disabled={!raster || busy !== null}
						disabled={!raster || busy !== null}
						onclick={() => openConfirm()}
					>
						{m.edit_save()}
					</button>
				</div>
			</div>
		</aside>
	</div>
</div>

{#if confirmOpen}
	<div
		class="image-editor-dialog-backdrop fixed inset-0 z-[360] flex items-center justify-center bg-black/50 p-4"
		role="presentation"
		onclick={() => {
			if (busy !== 'save') confirmOpen = false;
		}}
	>
		<div
			class="bg-base-100 border-ink-border image-editor-dialog w-full max-w-sm space-y-3 rounded-lg border p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="disposition-title"
			tabindex="-1"
			onclick={(event) => event.stopPropagation()}
			onkeydown={(event) => event.stopPropagation()}
			transition:scale={{ duration: reducedMotion ? 0 : 180, start: 0.96 }}
		>
			<h3 id="disposition-title" class="font-display text-secondary text-xl font-bold">
				{m.edit_disposition_title()}
			</h3>
			{#if !canReplace}
				<p class="text-xs">{m.edit_replace_blocked()}</p>
			{/if}
			<label class="block text-sm" for="save-as-stem">
				{m.edit_save_as_stem()}<span
					class="text-error align-top text-sm leading-none"
					aria-hidden="true">*</span
				>
			</label>
			<div class="flex items-stretch gap-1">
				<input
					id="save-as-stem"
					class="input min-w-0 flex-1 cursor-text"
					bind:value={saveAsStem}
					required
					disabled={busy !== null}
					aria-describedby="save-as-ext-help"
				/>
				{#if saveAsExtLabel}
					<span
						class="bg-base-200 border-base-300 text-base-content/70 inline-flex shrink-0 items-center rounded-lg border px-3 font-mono text-sm"
						title={m.edit_save_as_ext_locked()}
						aria-label={m.edit_save_as_ext_locked()}
					>
						{saveAsExtLabel}
					</span>
				{/if}
			</div>
			<p id="save-as-ext-help" class="text-base-content/70 text-xs leading-relaxed">
				{m.edit_save_as_ext_help()}
			</p>
			{#if error}
				<p class="text-error text-sm" role="alert">{error}</p>
			{/if}
			<div class="flex flex-wrap gap-2 pt-1">
				<button
					type="button"
					class="btn btn-error btn-sm image-editor-press"
					class:cursor-pointer={canReplace && busy === null}
					class:cursor-not-allowed={!canReplace || busy !== null}
					class:loading={busy === 'save'}
					class:btn-disabled={!canReplace || busy !== null}
					disabled={!canReplace || busy !== null}
					aria-busy={busy === 'save'}
					onclick={() => void commit('replace')}
				>
					{m.edit_replace()}
				</button>
				<button
					type="button"
					class="btn btn-primary btn-sm image-editor-press"
					class:cursor-pointer={busy === null && saveAsStem.trim() !== ''}
					class:cursor-not-allowed={busy !== null || saveAsStem.trim() === ''}
					class:loading={busy === 'save'}
					class:btn-disabled={busy !== null || saveAsStem.trim() === ''}
					disabled={busy !== null || saveAsStem.trim() === ''}
					aria-busy={busy === 'save'}
					onclick={() => void commit('saveAsNew')}
				>
					{m.edit_save_as_new()}
				</button>
				<button
					type="button"
					class="btn btn-ghost btn-sm image-editor-press cursor-pointer"
					disabled={busy !== null}
					onclick={() => (confirmOpen = false)}
				>
					{m.edit_cancel()}
				</button>
			</div>
		</div>
	</div>
{/if}
