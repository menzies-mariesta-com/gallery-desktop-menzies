<script lang="ts">
	import { onMount } from 'svelte';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import {
		copyImageToClipboard,
		deleteGalleryImage,
		getDefaultPicturesDir,
		imageSrc,
		isTauri,
		listGalleryEntries,
		loadGallerySettings,
		loadImageOrder,
		renameGalleryImage,
		saveGallerySettings,
		saveImageOrder
	} from '$lib/tool/gallery-ipc';
	import { galleryErrorCode, isRasterName } from '$lib/tool/image-edit';
	import { extensionCatalog } from '$lib/tool/extensions';
	import ImageEditor from '$lib/tool/ImageEditor.svelte';
	import PeoplePanel from '$lib/tool/PeoplePanel.svelte';
	import { subscribeRecognitionAvailable } from '$lib/store/state/recognition';
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
	import { portal } from '$lib/util/portal';

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
	let toast = $state<{ tone: 'success' | 'error' | 'info'; text: string } | null>(null);
	let toastTimer: ReturnType<typeof setTimeout> | null = null;
	let refreshTimer: ReturnType<typeof setTimeout> | null = null;
	let refreshSeq = 0;
	let prefsReady = $state(false);
	let reorderMode = $state(false);
	let manualOrder = $state<string[]>([]);
	let dragPath = $state<string | null>(null);
	let editPath = $state<string | null>(null);
	let renameTarget = $state<GalleryEntry | null>(null);
	let renameStem = $state('');
	let renameExt = $state('');
	let renaming = $state(false);
	let copying = $state(false);
	let deleteTarget = $state<GalleryEntry | null>(null);
	let deleting = $state(false);
	let recognitionOn = $state(false);
	let showPeople = $state(false);

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

	const reorderBlocked = $derived(searchQuery.trim() !== '' || typeFilter !== 'all');
	const showManualOrder = $derived(reorderMode && !reorderBlocked);

	const orderedImages = $derived.by(() => {
		const images = entries.filter((entry) => entry.kind === 'image');
		if (!showManualOrder || manualOrder.length === 0) {
			return filteredEntries.filter((entry) => entry.kind === 'image');
		}
		const byPath: Record<string, GalleryEntry> = {};
		for (const image of images) byPath[image.path] = image;
		const ordered: GalleryEntry[] = [];
		const seen: Record<string, true> = {};
		for (const path of manualOrder) {
			const hit = byPath[path];
			if (hit && !seen[path]) {
				ordered.push(hit);
				seen[path] = true;
			}
		}
		for (const image of images) {
			if (!seen[image.path]) ordered.push(image);
		}
		return ordered;
	});

	const displayEntries = $derived.by(() => {
		if (!showManualOrder) return filteredEntries;
		const folders = filteredEntries.filter((entry) => entry.kind === 'folder');
		return [...folders, ...orderedImages];
	});

	const visibleImages = $derived(
		(showManualOrder ? orderedImages : filteredEntries).filter((entry) => entry.kind === 'image')
	);
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

	function showToast(tone: 'success' | 'error' | 'info', text: string) {
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

	async function refresh(options?: { soft?: boolean }) {
		if (!currentDir) {
			entries = [];
			return;
		}
		const seq = ++refreshSeq;
		const soft = options?.soft === true || entries.length > 0;
		if (!soft) loading = true;
		error = null;
		try {
			const nextEntries = await listGalleryEntries(currentDir, allowedRoot || null);
			if (seq !== refreshSeq) return;
			let nextOrder: string[] = [];
			try {
				nextOrder = await loadImageOrder(currentDir, allowedRoot || null);
			} catch {
				nextOrder = [];
			}
			if (seq !== refreshSeq) return;
			entries = nextEntries;
			manualOrder = nextOrder;
			await persistPrefs();
		} catch {
			if (seq !== refreshSeq) return;
			error = m.load_error();
			entries = [];
			showToast('error', m.load_error());
		} finally {
			if (seq === refreshSeq) loading = false;
		}
	}

	/** Coalesce Titlebar / extension refresh bursts. */
	function scheduleRefresh() {
		if (refreshTimer) clearTimeout(refreshTimer);
		refreshTimer = setTimeout(() => {
			refreshTimer = null;
			void refresh({ soft: true });
		}, 280);
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
		if (editPath) return;
		if (deleteTarget) {
			if (event.key === 'Escape' && !deleting) deleteTarget = null;
			return;
		}
		if (renameTarget) {
			if (event.key === 'Escape') renameTarget = null;
			return;
		}
		if (lightboxIndex === null) return;
		if (event.key === 'Escape') closeLightbox();
		if (event.key === 'ArrowLeft') prevImage();
		if (event.key === 'ArrowRight') nextImage();
	}

	function startDelete(entry: GalleryEntry) {
		if (deleting || !isTauri()) return;
		deleteTarget = entry;
	}

	async function confirmDelete() {
		if (!deleteTarget || deleting) return;
		const target = deleteTarget;
		const keepIndex = lightboxIndex;
		deleting = true;
		try {
			await deleteGalleryImage(target.path, allowedRoot || null);
			deleteTarget = null;
			showToast('success', m.delete_saved({ name: target.name }));
			await refresh();
			if (keepIndex === null) return;
			if (visibleImages.length === 0) {
				lightboxIndex = null;
				return;
			}
			lightboxIndex = Math.min(keepIndex, visibleImages.length - 1);
		} catch {
			showToast('error', m.delete_error());
		} finally {
			deleting = false;
		}
	}

	function openEditor(entry: GalleryEntry) {
		if (!isRasterName(entry.name)) {
			showToast('error', m.edit_not_raster());
			return;
		}
		editPath = entry.path;
	}

	async function onEdited(path: string, name: string) {
		editPath = null;
		lightboxIndex = null;
		showToast('info', m.edit_saved({ name }));
		await refresh();
		const index = visibleImages.findIndex((image) => image.path === path);
		if (index >= 0) lightboxIndex = index;
	}

	async function copyImage(entry: GalleryEntry) {
		if (copying || !isTauri()) return;
		copying = true;
		try {
			await copyImageToClipboard(entry.path, allowedRoot || null);
			showToast('info', m.copy_clipboard_ok());
		} catch {
			showToast('error', m.copy_error());
		} finally {
			copying = false;
		}
	}

	function startRename(entry: GalleryEntry) {
		renameTarget = entry;
		const i = entry.name.lastIndexOf('.');
		if (i > 0) {
			renameStem = entry.name.slice(0, i);
			renameExt = entry.name.slice(i);
		} else {
			renameStem = entry.name;
			renameExt = '';
		}
	}

	async function submitRename() {
		if (!renameTarget || renaming) return;
		const stem = renameStem.trim();
		if (!stem) return;
		const target = renameTarget;
		renaming = true;
		try {
			// Send stem only. Rust joins the locked original extension.
			const renamed = await renameGalleryImage(target.path, allowedRoot || null, stem);
			renameTarget = null;
			showToast('info', m.rename_saved({ name: renamed.name }));
			await refresh();
			const index = visibleImages.findIndex((image) => image.path === renamed.path);
			if (index >= 0) lightboxIndex = index;
		} catch (err) {
			const code = galleryErrorCode(err);
			if (code === 'target_exists') showToast('error', m.rename_exists());
			else if (code === 'extension_locked') showToast('error', m.rename_extension());
			else showToast('error', m.rename_error());
		} finally {
			renaming = false;
		}
	}

	async function persistOrder(paths: string[]) {
		if (!currentDir) return;
		manualOrder = paths;
		try {
			await saveImageOrder(currentDir, allowedRoot || null, paths);
		} catch {
			showToast('error', m.reorder_error());
			try {
				manualOrder = await loadImageOrder(currentDir, allowedRoot || null);
			} catch {
				manualOrder = [];
			}
		}
	}

	function moveImage(path: string, delta: number) {
		if (!showManualOrder) return;
		const paths = orderedImages.map((image) => image.path);
		const index = paths.indexOf(path);
		const nextIndex = index + delta;
		if (index < 0 || nextIndex < 0 || nextIndex >= paths.length) return;
		const next = [...paths];
		const [item] = next.splice(index, 1);
		if (!item) return;
		next.splice(nextIndex, 0, item);
		void persistOrder(next);
	}

	function dropOn(target: string) {
		if (!showManualOrder || !dragPath || dragPath === target) {
			dragPath = null;
			return;
		}
		const paths = orderedImages.map((image) => image.path);
		const from = paths.indexOf(dragPath);
		const to = paths.indexOf(target);
		dragPath = null;
		if (from < 0 || to < 0) return;
		const next = [...paths];
		const [item] = next.splice(from, 1);
		if (!item) return;
		next.splice(to, 0, item);
		void persistOrder(next);
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
			scheduleRefresh();
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

		const unsubscribeRecognition = subscribeRecognitionAvailable((value) => {
			recognitionOn = value;
			if (!value) showPeople = false;
		});
		if (isTauri()) {
			void extensionCatalog()
				.then((items) => {
					const available = Boolean(items?.some((item) => item.recognitionAvailable));
					recognitionOn = available;
					if (!available) showPeople = false;
				})
				.catch(() => {
					recognitionOn = false;
					showPeople = false;
				});
		}

		return () => {
			unsub();
			unsubscribeRecognition();
			if (toastTimer) clearTimeout(toastTimer);
			if (refreshTimer) clearTimeout(refreshTimer);
			refreshSeq += 1;
		};
	});
</script>

<svelte:window onkeydown={onKeydown} />

<div class="relative flex h-full min-h-0 w-full flex-col gap-3">
	{#if showOnboarding}
		<div
			class="bg-base-100/75 absolute inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm"
			role="dialog"
			aria-modal="true"
			aria-labelledby="onboard-title"
		>
			<div
				class="{washRecipes.washPanel} border-ink-border/60 max-w-md space-y-4 border p-6 text-center shadow-lg"
			>
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

	<header
		class="{washRecipes.washPanelFlush} border-ink-border/50 bg-base-100/85 shrink-0 space-y-2 border px-3 py-2 backdrop-blur-sm"
	>
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
			<div
				class={washRecipes.tooltipIcon('secondary', 'bottom')}
				data-tip={reorderBlocked ? m.reorder_blocked() : m.reorder_images()}
			>
				<button
					type="button"
					class="btn btn-ghost btn-square btn-sm btn-secondary"
					class:cursor-pointer={!reorderBlocked}
					class:cursor-not-allowed={reorderBlocked}
					class:btn-disabled={reorderBlocked}
					disabled={reorderBlocked}
					aria-pressed={reorderMode}
					aria-label={m.reorder_images()}
					onclick={() => (reorderMode = !reorderMode)}
				>
					<WashIcon icon={washIcons['grip-vertical']} class="size-4" />
				</button>
			</div>
			{#if recognitionOn}
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-primary cursor-pointer"
					aria-pressed={showPeople}
					onclick={() => (showPeople = !showPeople)}
				>
					{showPeople ? m.people_photos() : m.people()}
				</button>
			{/if}
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
	</header>

	<div
		class="{washRecipes.washPanel} border-ink-border/40 bg-base-100/80 min-h-0 flex-1 overflow-auto border p-2"
	>
		{#if showPeople && recognitionOn}
			<PeoplePanel folder={currentDir} {allowedRoot} />
		{:else if !isTauri()}
			<div class="flex min-h-[14rem] flex-col items-center justify-center p-6 text-center">
				<WashIcon icon={washIcons.image} class="text-primary/60 mb-3 size-10" />
				<p class="text-base-content/70 text-sm">{m.empty_browser()}</p>
			</div>
		{:else if loading && entries.length === 0}
			<div
				class="flex min-h-[14rem] flex-col items-center justify-center p-6 text-center"
				aria-live="polite"
			>
				<span class="loading loading-spinner loading-md text-primary mb-3" aria-hidden="true"
				></span>
				<p class="text-base-content/70 text-sm">{m.loading()}</p>
			</div>
		{:else if error}
			<div class="flex min-h-[14rem] flex-col items-center justify-center p-6 text-center">
				<p class="text-error text-sm">{error}</p>
			</div>
		{:else if entries.length === 0}
			<div class="flex min-h-[14rem] flex-col items-center justify-center p-6 text-center">
				<WashIcon icon={washIcons.folder} class="text-primary/60 mb-3 size-10" />
				<p class="text-base-content/70 text-sm">{m.empty_gallery()}</p>
			</div>
		{:else if filteredEntries.length === 0}
			<div class="flex min-h-[14rem] flex-col items-center justify-center p-6 text-center">
				<WashIcon icon={washIcons.search} class="text-primary/60 mb-3 size-10" />
				<p class="text-base-content/70 text-sm">{m.empty_filter()}</p>
			</div>
		{:else}
			{#if showManualOrder}
				<p class="text-base-content/70 mb-2 px-1 text-xs">{m.reorder_help()}</p>
			{:else if reorderMode && reorderBlocked}
				<p class="text-base-content/70 mb-2 px-1 text-xs">{m.reorder_blocked()}</p>
			{/if}
			<ul
				class="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
			>
				{#each displayEntries as entry (entry.path)}
					<li
						class="gallery-tile"
						draggable={showManualOrder && entry.kind === 'image'}
						class:cursor-grab={showManualOrder && entry.kind === 'image'}
						ondragstart={() => (dragPath = entry.path)}
						ondragover={(event) => {
							if (showManualOrder && entry.kind === 'image') event.preventDefault();
						}}
						ondrop={() => dropOn(entry.path)}
					>
						{#if entry.kind === 'folder'}
							<button
								type="button"
								class="group border-ink-border/20 bg-base-100 hover:border-primary/40 focus-visible:outline-primary flex w-full cursor-pointer flex-col overflow-hidden rounded-lg border text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
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
								class="group border-ink-border/20 bg-base-100 hover:border-primary/40 focus-visible:outline-primary flex w-full cursor-pointer flex-col overflow-hidden rounded-lg border text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
								onclick={() => openLightboxForPath(entry.path)}
								onkeydown={(event) => {
									if (!showManualOrder) return;
									if (!event.altKey) return;
									if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
										event.preventDefault();
										moveImage(entry.path, -1);
									}
									if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
										event.preventDefault();
										moveImage(entry.path, 1);
									}
								}}
							>
								<div class="bg-base-300 aspect-square w-full overflow-hidden">
									<img
										src={imageSrc(entry.path)}
										alt={entry.name}
										class="h-full w-full object-cover"
										loading="lazy"
										decoding="async"
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
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.edit_image()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary text-white"
						class:cursor-pointer={isRasterName(lightboxImage.name)}
						class:cursor-not-allowed={!isRasterName(lightboxImage.name)}
						disabled={!isRasterName(lightboxImage.name)}
						aria-label={m.edit_image()}
						onclick={() => openEditor(lightboxImage)}
					>
						<WashIcon icon={washIcons.pencil} class="size-4" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.copy_image()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary text-white"
						class:cursor-pointer={!copying}
						class:cursor-not-allowed={copying}
						class:loading={copying}
						disabled={copying}
						aria-busy={copying}
						aria-label={m.copy_image()}
						onclick={() => void copyImage(lightboxImage)}
					>
						<WashIcon icon={washIcons['clipboard-copy']} class="size-4" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.rename_image()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer text-white"
						aria-label={m.rename_image()}
						onclick={() => startRename(lightboxImage)}
					>
						<WashIcon icon={washIcons.type} class="size-4" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-error" data-tip={m.delete_image()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-error"
						class:cursor-pointer={!deleting}
						class:cursor-not-allowed={deleting}
						class:btn-disabled={deleting}
						disabled={deleting}
						aria-label={m.delete_image()}
						onclick={() => startDelete(lightboxImage)}
					>
						<WashIcon icon={washIcons.trash} class="size-4" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.lightbox_prev()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer text-white"
						aria-label={m.lightbox_prev()}
						onclick={prevImage}
					>
						<WashIcon icon={washIcons['chevron-left']} class="size-5" />
					</button>
				</div>
				<div class="tooltip tooltip-bottom tooltip-secondary" data-tip={m.lightbox_next()}>
					<button
						type="button"
						class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer text-white"
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
				decoding="async"
			/>
		</button>
	</div>
{/if}

{#if editPath && lightboxImage}
	<ImageEditor
		sourcePath={editPath}
		sourceName={lightboxImage.name}
		{allowedRoot}
		onClose={() => (editPath = null)}
		onSaved={(path, name) => void onEdited(path, name)}
	/>
{/if}

{#if renameTarget}
	<div
		class="fixed inset-0 z-[360] flex items-center justify-center bg-black/50 p-4"
		role="dialog"
		aria-modal="true"
		aria-labelledby="rename-title"
	>
		<form
			class="bg-base-100 border-ink-border w-full max-w-sm space-y-3 rounded-lg border p-4 shadow-lg"
			onsubmit={(event) => {
				event.preventDefault();
				void submitRename();
			}}
		>
			<h2 id="rename-title" class="font-display text-secondary text-xl font-bold">
				{m.rename_title()}
			</h2>
			<label class="block text-sm" for="rename-input">
				{m.rename_stem()}<span class="text-error align-top text-sm leading-none" aria-hidden="true"
					>*</span
				>
			</label>
			<div class="flex items-stretch gap-1">
				<input
					id="rename-input"
					class="input min-w-0 flex-1 cursor-text"
					bind:value={renameStem}
					required
					disabled={renaming}
					aria-describedby="rename-ext-help"
				/>
				{#if renameExt}
					<span
						class="bg-base-200 border-base-300 text-base-content/70 inline-flex shrink-0 items-center rounded-lg border px-3 font-mono text-sm"
						title={m.rename_ext_locked()}
						aria-label={m.rename_ext_locked()}
					>
						{renameExt}
					</span>
				{/if}
			</div>
			<p id="rename-ext-help" class="text-base-content/70 text-xs leading-relaxed">
				{m.rename_ext_help()}
			</p>
			<div class="flex justify-end gap-2">
				<button
					type="button"
					class="btn btn-ghost btn-sm cursor-pointer"
					disabled={renaming}
					onclick={() => (renameTarget = null)}
				>
					{m.rename_cancel()}
				</button>
				<button
					type="submit"
					class="btn btn-secondary btn-sm"
					class:cursor-pointer={!renaming && renameStem.trim() !== ''}
					class:cursor-not-allowed={renaming || renameStem.trim() === ''}
					class:loading={renaming}
					class:btn-disabled={renaming || renameStem.trim() === ''}
					disabled={renaming || renameStem.trim() === ''}
					aria-busy={renaming}
				>
					{m.rename_save()}
				</button>
			</div>
		</form>
	</div>
{/if}

{#if deleteTarget}
	<!-- Portal past titlebar backdrop-filter. In-app modal only. Never window.confirm. -->
	<div
		class="fixed inset-0 z-[360] flex items-center justify-center bg-black/50 p-4"
		role="presentation"
		use:portal
		onclick={() => {
			if (!deleting) deleteTarget = null;
		}}
	>
		<div
			class="bg-base-100 border-ink-border w-full max-w-sm space-y-3 rounded-lg border p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="delete-title"
			tabindex="-1"
			onclick={(event) => event.stopPropagation()}
			onkeydown={(event) => event.stopPropagation()}
		>
			<h2 id="delete-title" class="font-display text-error text-xl font-bold">
				{m.delete_title()}
			</h2>
			<p class="text-base-content/80 text-sm leading-relaxed">
				{m.delete_confirm_body({ name: deleteTarget.name })}
			</p>
			<div class="flex justify-end gap-2">
				<button
					type="button"
					class="btn btn-ghost btn-sm"
					class:cursor-pointer={!deleting}
					class:cursor-not-allowed={deleting}
					disabled={deleting}
					onclick={() => (deleteTarget = null)}
				>
					{m.delete_cancel()}
				</button>
				<button
					type="button"
					class="btn btn-error btn-sm"
					class:cursor-pointer={!deleting}
					class:cursor-not-allowed={deleting}
					class:cursor-wait={deleting}
					class:loading={deleting}
					class:btn-disabled={deleting}
					disabled={deleting}
					aria-busy={deleting}
					onclick={() => void confirmDelete()}
				>
					{m.delete_confirm_action()}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if toast}
	<div class="toast toast-bottom toast-end z-[400]" use:portal>
		<div
			class="alert shadow-lg"
			class:alert-success={toast.tone === 'success'}
			class:alert-error={toast.tone === 'error'}
			class:alert-info={toast.tone === 'info'}
		>
			{#if toast.tone === 'success'}
				<WashIcon icon={washIcons['circle-check']} class="size-5 shrink-0" />
			{:else if toast.tone === 'error'}
				<WashIcon icon={washIcons['circle-x']} class="size-5 shrink-0" />
			{/if}
			<span>{toast.text}</span>
		</div>
	</div>
{/if}
