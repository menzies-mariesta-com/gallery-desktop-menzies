<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { m } from '$lib/paraglide/messages.js';
	import FaceCropPreview from '$lib/tool/FaceCropPreview.svelte';
	import {
		cancelFaceIndex,
		faceIndexRunning,
		listFacePeople,
		listFacePersonImages,
		renameFacePerson,
		startFaceIndex,
		subscribeFaceProgress,
		type FaceIndexEvent,
		type FacePerson
	} from '$lib/tool/face-index';
	import { galleryErrorCode } from '$lib/tool/image-edit';
	import { imageSrc } from '$lib/tool/gallery-ipc';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { washIcons } from '$lib/tool/wash-icons';

	let { folder, allowedRoot }: { folder: string; allowedRoot: string } = $props();

	let people = $state<FacePerson[]>([]);
	let loading = $state(true);
	let indexing = $state(false);
	let ownedIndex = false;
	let renamingId = $state<number | null>(null);
	let renameDraft = $state('');
	let renameOpenId = $state<number | null>(null);
	let progress = $state<FaceIndexEvent | null>(null);
	let notice = $state<string | null>(null);
	let error = $state<string | null>(null);
	let selectedId = $state<number | null>(null);
	let detailImages = $state<string[]>([]);
	let detailLoading = $state(false);
	let refreshTimer: ReturnType<typeof setTimeout> | null = null;
	let indexSeq = 0;

	const selected = $derived(people.find((person) => person.id === selectedId) ?? null);
	const useContain = $derived(people.length > 80 || detailImages.length > 80);

	function fileName(path: string): string {
		const parts = path.split(/[/\\]/);
		return parts[parts.length - 1] || path;
	}

	function displayName(person: FacePerson): string {
		const label = person.label?.trim();
		return label ? label : m.people_unnamed();
	}

	function indexError(err: unknown): string {
		const code = galleryErrorCode(err);
		if (code === 'busy') return m.people_busy();
		if (code === 'experimental_off') return m.ext_experimental_required();
		if (code === 'inference_not_linked') return m.ext_runtime_missing();
		if (code === 'not_installed') return m.ext_not_installed();
		return m.people_error();
	}

	function clearRefreshTimer() {
		if (refreshTimer === null) return;
		clearTimeout(refreshTimer);
		refreshTimer = null;
	}

	function schedulePeopleRefresh() {
		if (refreshTimer !== null) return;
		refreshTimer = setTimeout(() => {
			refreshTimer = null;
			void loadPeople(false);
		}, 900);
	}

	async function loadPeople(showSpinner: boolean) {
		if (showSpinner) loading = true;
		try {
			const next = await listFacePeople();
			people = next;
			if (selectedId !== null && !next.some((person) => person.id === selectedId)) {
				selectedId = null;
				detailImages = [];
			}
		} catch (err) {
			error = indexError(err);
		} finally {
			loading = false;
		}
	}

	async function runAutoIndex(targetFolder: string, root: string, seq: number) {
		if (!targetFolder) return;
		if (faceIndexRunning()) {
			indexing = true;
			return;
		}
		indexing = true;
		ownedIndex = true;
		error = null;
		notice = null;
		progress = null;
		try {
			const summary = await startFaceIndex(targetFolder, root);
			if (seq !== indexSeq) return;
			progress = null;
			if (summary.truncated) notice = m.people_truncated();
			else if (!summary.cancelled) {
				notice = m.people_done({
					scanned: summary.scanned,
					skipped: summary.skipped,
					faces: summary.faces
				});
			}
			await loadPeople(false);
		} catch (err) {
			if (seq !== indexSeq) return;
			if (galleryErrorCode(err) !== 'busy') error = indexError(err);
		} finally {
			if (seq === indexSeq) {
				indexing = faceIndexRunning();
				ownedIndex = false;
			}
		}
	}

	async function cancel() {
		if (!indexing) return;
		try {
			await cancelFaceIndex();
		} catch (err) {
			error = indexError(err);
		}
	}

	async function openPerson(person: FacePerson) {
		selectedId = person.id;
		renameOpenId = null;
		detailLoading = true;
		detailImages = [];
		error = null;
		try {
			detailImages = await listFacePersonImages(person.id);
		} catch (err) {
			error = indexError(err);
		} finally {
			detailLoading = false;
		}
	}

	function backToPeople() {
		selectedId = null;
		detailImages = [];
		renameOpenId = null;
	}

	function beginRename(person: FacePerson) {
		renameOpenId = person.id;
		renameDraft = person.label ?? '';
	}

	async function saveName(id: number) {
		if (renamingId !== null) return;
		renamingId = id;
		error = null;
		try {
			await renameFacePerson(id, renameDraft);
			renameOpenId = null;
			await loadPeople(false);
		} catch (err) {
			error = indexError(err);
		} finally {
			renamingId = null;
		}
	}

	$effect(() => {
		const dir = folder;
		const root = allowedRoot;
		const seq = ++indexSeq;
		untrack(() => {
			selectedId = null;
			detailImages = [];
			renameOpenId = null;
			notice = null;
			error = null;
		});
		void loadPeople(true);
		void runAutoIndex(dir, root, seq);
		return () => {
			clearRefreshTimer();
			indexSeq += 1;
			if (ownedIndex || faceIndexRunning()) {
				ownedIndex = false;
				void cancelFaceIndex();
			}
		};
	});

	onMount(() => {
		indexing = faceIndexRunning();
		const unsubscribe = subscribeFaceProgress((event) => {
			progress = event;
			indexing = true;
			schedulePeopleRefresh();
		});
		return () => {
			unsubscribe();
			clearRefreshTimer();
		};
	});
</script>

<div class="flex min-h-0 flex-col gap-3 p-2">
	{#if selected}
		<div class="flex flex-wrap items-center gap-2">
			<button
				type="button"
				class="btn btn-ghost btn-sm btn-secondary cursor-pointer gap-1"
				onclick={() => backToPeople()}
			>
				<WashIcon icon={washIcons['chevron-left']} class="size-4" />
				{m.people_back()}
			</button>
		</div>
		<div class="flex flex-wrap items-center gap-3">
			<FaceCropPreview
				preview={selected.preview}
				alt={displayName(selected)}
				shape="circle"
				class="size-16 shrink-0 sm:size-20"
			/>
			<div class="min-w-0 flex-1">
				{#if renameOpenId === selected.id}
					<div class="flex flex-wrap items-end gap-2">
						<label class="min-w-[10rem] flex-1" for="person-name-{selected.id}">
							<span class="text-base-content/70 mb-1 block text-xs">{m.people_name()}</span>
							<input
								id="person-name-{selected.id}"
								class="input input-sm border-ink-border w-full cursor-text"
								maxlength="80"
								placeholder={m.people_unnamed()}
								value={renameDraft}
								oninput={(event) => {
									renameDraft = event.currentTarget.value;
								}}
							/>
						</label>
						<button
							type="button"
							class="btn btn-secondary btn-sm"
							class:cursor-pointer={renamingId === null}
							class:cursor-not-allowed={renamingId !== null}
							class:cursor-wait={renamingId === selected.id}
							class:loading={renamingId === selected.id}
							class:btn-disabled={renamingId !== null}
							disabled={renamingId !== null}
							aria-busy={renamingId === selected.id}
							onclick={() => void saveName(selected.id)}
						>
							{m.people_rename_save()}
						</button>
						<button
							type="button"
							class="btn btn-ghost btn-sm cursor-pointer"
							disabled={renamingId !== null}
							onclick={() => (renameOpenId = null)}
						>
							{m.people_rename_cancel()}
						</button>
					</div>
					<p class="text-base-content/60 mt-1 text-xs">{m.people_name_hint()}</p>
				{:else}
					<div class="flex flex-wrap items-center gap-2">
						<h2 class="font-display text-lg font-semibold">{displayName(selected)}</h2>
						<div class="tooltip tooltip-secondary" data-tip={m.people_rename()}>
							<button
								type="button"
								class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
								aria-label={m.people_rename()}
								onclick={() => beginRename(selected)}
							>
								<WashIcon icon={washIcons.pencil} class="size-4" />
							</button>
						</div>
					</div>
					<p class="text-base-content/70 text-xs">
						{m.people_photo_count({ count: selected.photoCount })}
					</p>
				{/if}
			</div>
		</div>
		{#if detailLoading}
			<p class="text-base-content/70 text-sm" aria-live="polite">{m.people_detail_loading()}</p>
		{:else if detailImages.length === 0}
			<p class="text-base-content/70 text-sm">{m.people_detail_empty()}</p>
		{:else}
			<ul
				class="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
			>
				{#each detailImages as image (image)}
					<li
						class="border-ink-border/40 bg-base-200/40 rounded-box aspect-square overflow-hidden border"
						style:content-visibility={useContain ? 'auto' : undefined}
						style:contain-intrinsic-size={useContain ? '8rem' : undefined}
					>
						<img
							src={imageSrc(image)}
							alt={fileName(image)}
							loading="lazy"
							class="size-full object-cover"
							draggable="false"
						/>
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<div class="flex flex-wrap items-center gap-2">
			{#if indexing}
				<span class="loading loading-spinner loading-sm text-primary" aria-hidden="true"></span>
				<span class="text-base-content/80 text-xs">{m.people_indexing()}</span>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-secondary cursor-pointer"
					onclick={() => void cancel()}
				>
					{m.people_cancel()}
				</button>
			{/if}
		</div>
		{#if progress && indexing}
			<p class="text-base-content/70 text-xs" aria-live="polite">
				{m.people_progress({ done: progress.done, total: progress.total, faces: progress.faces })}
				{#if progress.name}
					{m.people_progress_file({ name: progress.name })}
				{/if}
			</p>
		{:else}
			<p class="text-base-content/60 text-xs">{m.people_hint()}</p>
		{/if}
		{#if notice}
			<p class="text-xs" role="status">{notice}</p>
		{/if}
		{#if error}
			<p class="text-error text-xs" role="alert">{error}</p>
		{/if}
		{#if loading && people.length === 0}
			<p class="text-base-content/70 text-sm">{m.people_loading()}</p>
		{:else if people.length === 0}
			<div class="flex min-h-[10rem] flex-col items-center justify-center text-center">
				<WashIcon icon={washIcons.users} class="text-primary/60 mb-3 size-10" />
				<p class="text-base-content/70 max-w-md text-sm">
					{indexing ? m.people_empty_indexing() : m.people_empty()}
				</p>
			</div>
		{:else}
			<ul
				class="grid grid-cols-3 gap-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-7"
				role="list"
			>
				{#each people as person (person.id)}
					<li
						style:content-visibility={useContain ? 'auto' : undefined}
						style:contain-intrinsic-size={useContain ? '10rem' : undefined}
					>
						<button
							type="button"
							class="hover:bg-primary/10 focus-visible:ring-primary rounded-box flex w-full cursor-pointer flex-col items-center gap-2 p-2 text-center focus-visible:ring-2 focus-visible:outline-none"
							aria-label={m.people_open({ name: displayName(person) })}
							onclick={() => void openPerson(person)}
						>
							<FaceCropPreview
								preview={person.preview}
								alt=""
								shape="circle"
								class="aspect-square w-full max-w-[7.5rem]"
							/>
							<span class="w-full truncate text-sm font-medium">{displayName(person)}</span>
							<span class="text-base-content/60 w-full truncate text-xs">
								{m.people_photo_count({ count: person.photoCount })}
							</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</div>
