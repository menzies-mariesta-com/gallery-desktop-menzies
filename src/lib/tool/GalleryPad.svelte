<script lang="ts">
	import { onMount } from 'svelte';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import {
		getDefaultPicturesDir,
		imageSrc,
		isTauri,
		listGalleryEntries,
		loadGallerySettings,
		saveGallerySettings
	} from '$lib/tool/gallery-ipc';
	import { subscribeGalleryRefresh } from '$lib/tool/gallery-refresh';
	import type {
		GalleryDiskSettings,
		GalleryEntry,
		SortBy,
		SortDir,
		TypeFilter
	} from '$lib/store/local-storage/settings';
	import { loadSettings, saveSettings } from '$lib/store/local-storage/settings';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import WashSelect from '$lib/tool/WashSelect.svelte';
	import { washIcons } from '$lib/tool/wash-icons';

	let picturesRoot = $state('');
	let allowedRoot = $state('');
	let currentDir = $state('');
	let entries = $state<GalleryEntry[]>([]);
	let searchQuery = $state('');
	let sortBy = $state<SortBy>('name');
	let sortDir = $state<SortDir>('asc');
	let typeFilter = $state<TypeFilter>('all');
	let loading = $state(false);
	let error = $state<string | null>(null);
	let lightboxIndex = $state<number | null>(null);
	let showOnboarding = $state(false);
	let toast = $state<{ tone: 'error' | 'info'; text: string } | null>(null);
	let toastTimer: ReturnType<typeof setTimeout> | null = null;
	let prefsReady = $state(false);

	const JPG_EXTS = new Set(['jpg', 'jpeg']);

	const sortByOptions = $derived([
		{ value: 'name', label: m.sort_name() },
		{ value: 'size', label: m.sort_size() },
		{ value: 'modified', label: m.sort_modified() }
	]);

	const sortDirOptions = $derived([
		{ value: 'asc', label: m.sort_asc() },
		{ value: 'desc', label: m.sort_desc() }
	]);

	const typeFilterOptions = $derived([
		{ value: 'all', label: m.filter_all() },
		{ value: 'jpg', label: m.filter_jpg() },
		{ value: 'png', label: m.filter_png() },
		{ value: 'webp', label: m.filter_webp() },
		{ value: 'gif', label: m.filter_gif() },
		{ value: 'bmp', label: m.filter_bmp() },
		{ value: 'svg', label: m.filter_svg() }
	]);

	const filteredEntries = $derived.by(() => {
		const q = searchQuery.trim().toLowerCase();
		let list = entries.filter((entry) => {
			if (q && !entry.name.toLowerCase().includes(q)) return false;
			if (entry.kind === 'folder') return true;
			if (typeFilter === 'all') return true;
			const ext = extensionOf(entry.name);
			if (typeFilter === 'jpg') return JPG_EXTS.has(ext);
			return ext === typeFilter;
		});

		const dirMul = sortDir === 'asc' ? 1 : -1;
		const folders = list.filter((e) => e.kind === 'folder');
		const images = list.filter((e) => e.kind === 'image');

		const compare = (a: GalleryEntry, b: GalleryEntry): number => {
			if (sortBy === 'name') {
				return a.name.toLowerCase().localeCompare(b.name.toLowerCase()) * dirMul;
			}
			if (sortBy === 'size') {
				const as = a.size ?? 0;
				const bs = b.size ?? 0;
				if (as === bs) {
					return a.name.toLowerCase().localeCompare(b.name.toLowerCase()) * dirMul;
				}
				return (as < bs ? -1 : 1) * dirMul;
			}
			const am = a.modifiedMs ?? 0;
			const bm = b.modifiedMs ?? 0;
			if (am === bm) {
				return a.name.toLowerCase().localeCompare(b.name.toLowerCase()) * dirMul;
			}
			return (am < bm ? -1 : 1) * dirMul;
		};

		folders.sort(compare);
		images.sort(compare);
		return [...folders, ...images];
	});

	const visibleImages = $derived(filteredEntries.filter((e) => e.kind === 'image'));
	const folderCount = $derived(filteredEntries.filter((e) => e.kind === 'folder').length);
	const imageCount = $derived(visibleImages.length);

	const lightboxImage = $derived(
		lightboxIndex !== null && lightboxIndex >= 0 && lightboxIndex < visibleImages.length
			? visibleImages[lightboxIndex]
			: null
	);

	const canGoUp = $derived(
		Boolean(
			currentDir &&
			allowedRoot &&
			currentDir !== allowedRoot &&
			isPathUnder(currentDir, allowedRoot)
		)
	);

	const filtersAtDefault = $derived(
		searchQuery.trim() === '' && sortBy === 'name' && sortDir === 'asc' && typeFilter === 'all'
	);

	const breadcrumbParts = $derived.by(() => {
		if (!currentDir || !allowedRoot) {
			return [{ label: m.pictures_default(), path: allowedRoot || picturesRoot }];
		}
		const rootLabel =
			allowedRoot === picturesRoot || !picturesRoot ? m.pictures_default() : baseName(allowedRoot);
		const parts: { label: string; path: string }[] = [{ label: rootLabel, path: allowedRoot }];
		if (currentDir === allowedRoot) return parts;

		const rel = relativePath(allowedRoot, currentDir);
		if (!rel) return parts;
		let acc = allowedRoot;
		for (const segment of rel.split(/[/\\]/).filter(Boolean)) {
			acc = joinPath(acc, segment);
			parts.push({ label: segment, path: acc });
		}
		return parts;
	});

	function extensionOf(name: string): string {
		const i = name.lastIndexOf('.');
		if (i < 0) return '';
		return name.slice(i + 1).toLowerCase();
	}

	function baseName(path: string): string {
		const parts = path.split(/[/\\]/).filter(Boolean);
		return parts[parts.length - 1] ?? path;
	}

	function joinPath(base: string, segment: string): string {
		if (base.endsWith('/') || base.endsWith('\\')) return `${base}${segment}`;
		const sep = base.includes('\\') ? '\\' : '/';
		return `${base}${sep}${segment}`;
	}

	function relativePath(root: string, full: string): string | null {
		const normRoot = root.replace(/\\/g, '/').replace(/\/+$/, '');
		const normFull = full.replace(/\\/g, '/').replace(/\/+$/, '');
		if (normFull === normRoot) return '';
		if (!normFull.startsWith(normRoot + '/')) return null;
		return normFull.slice(normRoot.length + 1);
	}

	function isPathUnder(child: string, root: string): boolean {
		const normRoot = root.replace(/\\/g, '/').replace(/\/+$/, '');
		const normChild = child.replace(/\\/g, '/').replace(/\/+$/, '');
		return normChild === normRoot || normChild.startsWith(normRoot + '/');
	}

	function parentDir(path: string): string | null {
		const norm = path.replace(/\\/g, '/').replace(/\/+$/, '');
		const idx = norm.lastIndexOf('/');
		if (idx <= 0) return null;
		return path.includes('\\') ? norm.slice(0, idx).replace(/\//g, '\\') : norm.slice(0, idx);
	}

	function showToast(tone: 'error' | 'info', text: string) {
		if (toastTimer) clearTimeout(toastTimer);
		toast = { tone, text };
		toastTimer = setTimeout(() => {
			toast = null;
			toastTimer = null;
		}, 4000);
	}

	function diskPayload(onboarded = true): GalleryDiskSettings {
		return {
			lastFolder: currentDir || null,
			recentFolders: [],
			onboarded,
			sortBy,
			sortDir,
			typeFilter
		};
	}

	async function persistPrefs() {
		if (!prefsReady || !isTauri()) return;
		try {
			await saveGallerySettings(diskPayload(true));
		} catch {
			/* non-fatal */
		}
	}

	async function refresh() {
		if (!currentDir) {
			entries = [];
			return;
		}
		loading = true;
		error = null;
		try {
			entries = await listGalleryEntries(currentDir, allowedRoot || null);
			await persistPrefs();
		} catch {
			error = m.load_error();
			entries = [];
			showToast('error', m.load_error());
		} finally {
			loading = false;
		}
	}

	async function enterFolder(path: string) {
		if (!allowedRoot || !isPathUnder(path, allowedRoot)) {
			showToast('error', m.load_error());
			return;
		}
		currentDir = path;
		lightboxIndex = null;
		searchQuery = '';
		await refresh();
	}

	async function goUp() {
		if (!canGoUp) return;
		const parent = parentDir(currentDir);
		if (!parent || !allowedRoot || !isPathUnder(parent, allowedRoot)) return;
		await enterFolder(parent);
	}

	function openLightboxForPath(path: string) {
		const index = visibleImages.findIndex((img) => img.path === path);
		if (index >= 0) lightboxIndex = index;
	}

	function closeLightbox() {
		lightboxIndex = null;
	}

	function prevImage() {
		if (lightboxIndex === null || visibleImages.length === 0) return;
		lightboxIndex = (lightboxIndex - 1 + visibleImages.length) % visibleImages.length;
	}

	function nextImage() {
		if (lightboxIndex === null || visibleImages.length === 0) return;
		lightboxIndex = (lightboxIndex + 1) % visibleImages.length;
	}

	function onKeydown(event: KeyboardEvent) {
		if (lightboxIndex === null) return;
		if (event.key === 'Escape') closeLightbox();
		if (event.key === 'ArrowLeft') prevImage();
		if (event.key === 'ArrowRight') nextImage();
	}

	function finishOnboarding() {
		const settings = loadSettings();
		settings.onboarded = true;
		saveSettings(settings);
		showOnboarding = false;
		void saveGallerySettings(diskPayload(true));
	}

	async function onSortByChange(value: string) {
		sortBy = value as SortBy;
		await persistPrefs();
	}

	async function onSortDirChange(value: string) {
		sortDir = value as SortDir;
		await persistPrefs();
	}

	async function onTypeFilterChange(value: string) {
		typeFilter = value as TypeFilter;
		await persistPrefs();
	}

	async function clearFilters() {
		if (filtersAtDefault) return;
		searchQuery = '';
		sortBy = 'name';
		sortDir = 'asc';
		typeFilter = 'all';
		await persistPrefs();
	}

	onMount(() => {
		const unsub = subscribeGalleryRefresh(() => {
			void refresh();
		});

		void (async () => {
			if (!isTauri()) {
				showOnboarding = !loadSettings().onboarded;
				prefsReady = true;
				return;
			}
			try {
				const disk = await loadGallerySettings();
				sortBy = disk.sortBy;
				sortDir = disk.sortDir;
				typeFilter = disk.typeFilter;
				const local = loadSettings();
				showOnboarding = !(disk.onboarded || local.onboarded);

				picturesRoot = await getDefaultPicturesDir();
				allowedRoot = picturesRoot;

				let start = picturesRoot;
				if (disk.lastFolder && isPathUnder(disk.lastFolder, picturesRoot)) {
					start = disk.lastFolder;
				} else if (disk.lastFolder) {
					// Legacy root outside Pictures: treat that folder as allowed root.
					allowedRoot = disk.lastFolder;
					start = disk.lastFolder;
				}

				currentDir = start;
				prefsReady = true;
				await refresh();
			} catch {
				try {
					picturesRoot = await getDefaultPicturesDir();
					allowedRoot = picturesRoot;
					currentDir = picturesRoot;
					prefsReady = true;
					await refresh();
				} catch {
					prefsReady = true;
					showToast('error', m.load_error());
				}
			}
		})();

		return () => {
			unsub();
			if (toastTimer) clearTimeout(toastTimer);
		};
	});
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full min-h-0 w-full flex-col gap-3">
	{#if showOnboarding}
		<div
			class="rounded-box border-ink-border bg-base-100 absolute inset-0 z-50 flex items-center justify-center border p-4"
			role="dialog"
			aria-modal="true"
			aria-labelledby="onboard-title"
		>
			<div class="max-w-md space-y-4 text-center">
				<WashIcon icon={washIcons.image} class="text-primary mx-auto size-12" />
				<h1 id="onboard-title" class="font-display text-primary text-2xl font-bold">
					{m.onboarding_title()}
				</h1>
				<p class="text-base-content/80 text-sm leading-relaxed">{m.onboarding_body()}</p>
				<button type="button" class="btn btn-primary cursor-pointer" onclick={finishOnboarding}>
					{m.onboarding_continue()}
				</button>
			</div>
		</div>
	{/if}

	<div class="flex shrink-0 flex-col gap-2">
		<nav class="flex min-w-0 flex-wrap items-center gap-1 text-sm" aria-label={m.folder_label()}>
			<div class={washRecipes.tooltipIcon('secondary', 'bottom')} data-tip={m.go_up()}>
				<button
					type="button"
					class="btn btn-ghost btn-square btn-sm btn-secondary"
					class:cursor-pointer={canGoUp && !loading}
					class:cursor-not-allowed={!canGoUp || loading}
					disabled={!canGoUp || loading}
					aria-label={m.go_up()}
					onclick={() => void goUp()}
				>
					<WashIcon icon={washIcons['arrow-up']} class="size-4" />
				</button>
			</div>
			{#each breadcrumbParts as part, i (part.path)}
				{#if i > 0}
					<span class="text-base-content/40 px-0.5" aria-hidden="true">/</span>
				{/if}
				{#if i === breadcrumbParts.length - 1}
					<span class="text-base-content max-w-[12rem] truncate font-medium" title={part.path}
						>{part.label}</span
					>
				{:else}
					<button
						type="button"
						class="text-primary max-w-[10rem] cursor-pointer truncate hover:underline"
						title={part.path}
						onclick={() => void enterFolder(part.path)}
					>
						{part.label}
					</button>
				{/if}
			{/each}
			<p class="text-base-content/60 ms-auto shrink-0 text-xs">
				{m.item_count({ folders: folderCount, images: imageCount })}
			</p>
		</nav>

		<div class="flex flex-wrap items-end gap-2">
			<label class="relative min-w-[10rem] flex-1">
				<span class="sr-only">{m.search_placeholder()}</span>
				<span
					class="text-base-content/50 pointer-events-none absolute top-1/2 left-2 -translate-y-1/2"
				>
					<WashIcon icon={washIcons.search} class="size-4" />
				</span>
				<input
					type="search"
					class="input input-sm border-ink-border w-full cursor-text ps-8"
					placeholder={m.search_placeholder()}
					bind:value={searchQuery}
				/>
			</label>
			<div class="w-36 shrink-0">
				<WashSelect
					options={sortByOptions}
					bind:value={sortBy}
					aria-label={m.sort_label()}
					class="select-sm"
					onchange={onSortByChange}
				/>
			</div>
			<div class="w-36 shrink-0">
				<WashSelect
					options={sortDirOptions}
					bind:value={sortDir}
					aria-label={m.sort_direction()}
					class="select-sm"
					onchange={onSortDirChange}
				/>
			</div>
			<div class="w-36 shrink-0">
				<WashSelect
					options={typeFilterOptions}
					bind:value={typeFilter}
					aria-label={m.filter_label()}
					class="select-sm"
					onchange={onTypeFilterChange}
				/>
			</div>
			<div class={washRecipes.tooltipIcon('secondary', 'bottom')} data-tip={m.clear_filters()}>
				<button
					type="button"
					class="btn btn-ghost btn-square btn-sm btn-secondary"
					class:cursor-pointer={!filtersAtDefault}
					class:cursor-not-allowed={filtersAtDefault}
					class:btn-disabled={filtersAtDefault}
					disabled={filtersAtDefault}
					aria-label={m.clear_filters()}
					onclick={() => void clearFilters()}
				>
					<WashIcon icon={washIcons['filter-x']} class="size-4" />
				</button>
			</div>
		</div>
	</div>

	<div
		class="bg-base-200/40 rounded-box border-ink-border/20 min-h-0 flex-1 overflow-auto border p-2"
	>
		{#if !isTauri()}
			<p class="text-base-content/70 p-6 text-center text-sm">{m.empty_browser()}</p>
		{:else if loading && entries.length === 0}
			<p class="text-base-content/70 p-6 text-center text-sm" aria-live="polite">{m.loading()}</p>
		{:else if error}
			<p class="text-error p-6 text-center text-sm">{error}</p>
		{:else if entries.length === 0}
			<p class="text-base-content/70 p-6 text-center text-sm">{m.empty_gallery()}</p>
		{:else if filteredEntries.length === 0}
			<p class="text-base-content/70 p-6 text-center text-sm">{m.empty_filter()}</p>
		{:else}
			<ul
				class="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
			>
				{#each filteredEntries as entry (entry.path)}
					<li>
						{#if entry.kind === 'folder'}
							<button
								type="button"
								class="group border-ink-border/20 bg-base-100 hover:border-primary/40 focus-visible:outline-primary flex w-full cursor-pointer flex-col overflow-hidden rounded-lg border text-left transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
								aria-label="{m.open_folder_tile()}: {entry.name}"
								onclick={() => void enterFolder(entry.path)}
							>
								<div
									class="bg-base-300 text-primary flex aspect-square w-full items-center justify-center"
								>
									<WashIcon icon={washIcons.folder} class="size-12 opacity-80" />
								</div>
								<span class="truncate px-2 py-1.5 text-xs" title={entry.name}>{entry.name}</span>
							</button>
						{:else}
							<button
								type="button"
								class="group border-ink-border/20 bg-base-100 hover:border-primary/40 focus-visible:outline-primary flex w-full cursor-pointer flex-col overflow-hidden rounded-lg border text-left transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
								onclick={() => openLightboxForPath(entry.path)}
							>
								<div class="bg-base-300 aspect-square w-full overflow-hidden">
									<img
										src={imageSrc(entry.path)}
										alt={entry.name}
										class="h-full w-full object-cover"
										loading="lazy"
									/>
								</div>
								<span class="truncate px-2 py-1.5 text-xs" title={entry.name}>{entry.name}</span>
							</button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</div>

{#if lightboxImage}
	<div
		class="fixed inset-0 z-[300] flex flex-col bg-black/85 p-3"
		role="dialog"
		aria-modal="true"
		aria-label={lightboxImage.name}
	>
		<div class="mb-2 flex shrink-0 items-center justify-between gap-2 text-white">
			<p class="min-w-0 flex-1 truncate font-mono text-sm">{lightboxImage.name}</p>
			<div class="flex shrink-0 items-center gap-1">
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.lightbox_prev()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm cursor-pointer text-white"
						aria-label={m.lightbox_prev()}
						onclick={prevImage}
					>
						<WashIcon icon={washIcons['chevron-left']} class="size-5" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.lightbox_next()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm cursor-pointer text-white"
						aria-label={m.lightbox_next()}
						onclick={nextImage}
					>
						<WashIcon icon={washIcons['chevron-right']} class="size-5" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-error" data-tip={m.lightbox_close()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-error cursor-pointer"
						aria-label={m.lightbox_close()}
						onclick={closeLightbox}
					>
						<WashIcon icon={washIcons.x} class="size-5" />
					</button>
				</div>
			</div>
		</div>
		<button
			type="button"
			class="flex min-h-0 flex-1 cursor-pointer items-center justify-center"
			aria-label={m.lightbox_close()}
			onclick={closeLightbox}
		>
			<img
				src={imageSrc(lightboxImage.path)}
				alt={lightboxImage.name}
				class="max-h-full max-w-full object-contain"
			/>
		</button>
	</div>
{/if}

{#if toast}
	<div class="toast toast-bottom toast-end z-[400]">
		<div
			class="alert shadow-lg"
			class:alert-error={toast.tone === 'error'}
			class:alert-info={toast.tone === 'info'}
		>
			<span>{toast.text}</span>
		</div>
	</div>
{/if}
