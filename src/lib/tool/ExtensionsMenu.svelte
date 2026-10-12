<script lang="ts">
	import { onMount } from 'svelte';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import {
		extensionCancelTransfer,
		extensionCatalog,
		extensionCheck,
		extensionInstall,
		extensionUninstall,
		transferPercent,
		type ExtensionTransferEvent,
		type ExtensionView
	} from '$lib/tool/extensions';
	import {
		installFaceExtension,
		pickFacePackageDir,
		setFaceRecognitionExperimental
	} from '$lib/tool/face-extension';
	import { subscribeGalleryRefresh } from '$lib/tool/gallery-refresh';
	import { isTauri } from '$lib/tool/gallery-ipc';
	import { publishRecognitionAvailable } from '$lib/store/state/recognition';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { washIcons } from '$lib/tool/wash-icons';
	import { portal } from '$lib/util/portal';

	type DialogKind = 'install' | 'uninstall';
	type DialogPhase = 'confirm' | 'running' | 'error';

	let open = $state(false);
	let items = $state<ExtensionView[] | null>(null);
	let busy = $state<'face' | 'remove' | null>(null);
	let dialog = $state<DialogKind | null>(null);
	let dialogPhase = $state<DialogPhase>('confirm');
	let progress = $state<ExtensionTransferEvent | null>(null);
	let dialogError = $state<string | null>(null);
	let toast = $state<{ tone: 'success' | 'error' | 'info'; text: string } | null>(null);
	let toastTimer: ReturnType<typeof setTimeout> | null = null;

	const face = $derived(items?.find((item) => item.id === 'face-recognition') ?? null);
	const faceInstalled = $derived(Boolean(face?.installedVersion));
	const faceBusy = $derived(busy === 'face' || busy === 'remove');
	const progressPct = $derived(transferPercent(progress));
	const progressLabel = $derived(labelForPhase(progress?.phase ?? null));

	function applyCatalog(next: ExtensionView[] | null) {
		items = next;
		publishRecognitionAvailable(Boolean(next?.some((item) => item.recognitionAvailable)));
	}

	function showToast(tone: 'success' | 'error' | 'info', text: string) {
		if (toastTimer) clearTimeout(toastTimer);
		toast = { tone, text };
		toastTimer = setTimeout(() => {
			toast = null;
			toastTimer = null;
		}, 4000);
	}

	function labelForPhase(phase: string | null): string {
		switch (phase) {
			case 'download':
				return m.ext_progress_download();
			case 'verify':
				return m.ext_progress_verify();
			case 'extract':
				return m.ext_progress_extract();
			case 'install':
				return m.ext_progress_install();
			case 'remove':
				return m.ext_progress_remove();
			case 'complete':
				return m.ext_progress_complete();
			default:
				return dialog === 'uninstall' ? m.ext_removing() : m.ext_installing();
		}
	}

	function closeDialog() {
		if (dialogPhase === 'running') return;
		dialog = null;
		dialogPhase = 'confirm';
		progress = null;
		dialogError = null;
	}

	async function loadLocal() {
		if (!isTauri()) {
			applyCatalog(null);
			return;
		}
		applyCatalog(await extensionCatalog());
	}

	async function refreshCatalog() {
		if (!isTauri()) return;
		try {
			applyCatalog(await extensionCheck(false));
		} catch {
			applyCatalog(await extensionCatalog());
		}
	}

	function toggle() {
		open = !open;
		if (open) {
			void loadLocal()
				.then(() => refreshCatalog())
				.catch(() => {
					/* silent: unavailable catalog stays click-noop */
				});
		}
	}

	function onDocPointerDown(event: PointerEvent) {
		const el = event.target as HTMLElement | null;
		if (dialog) return;
		if (!el?.closest('[data-extensions-menu]')) open = false;
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		if (dialog) {
			if (dialogPhase !== 'running') closeDialog();
			return;
		}
		open = false;
	}

	onMount(() => {
		let alive = true;
		const unsubscribe = subscribeGalleryRefresh(() => {
			if (!alive || busy) return;
			void refreshCatalog();
		});
		return () => {
			alive = false;
			unsubscribe();
			if (toastTimer) clearTimeout(toastTimer);
		};
	});

	function onFacePrimary() {
		if (faceBusy || !isTauri() || !face || faceInstalled) return;

		if (face.canInstall || face.offlineFallback) {
			dialog = 'install';
			dialogPhase = 'confirm';
			progress = null;
			dialogError = null;
			open = false;
			return;
		}

		showToast('info', m.ext_unavailable_toast());
	}

	function onFaceDelete() {
		if (faceBusy || !isTauri() || !face?.installedVersion) return;
		dialog = 'uninstall';
		dialogPhase = 'confirm';
		progress = null;
		dialogError = null;
		open = false;
	}

	async function confirmInstall() {
		if (!face || faceBusy || !isTauri()) return;
		busy = 'face';
		dialogPhase = 'running';
		progress = { phase: 'download', done: 0, total: 0 };
		dialogError = null;
		try {
			if (face.canInstall) {
				applyCatalog(
					await extensionInstall(face.id, (event) => {
						progress = event;
					})
				);
			} else if (face.offlineFallback) {
				progress = { phase: 'install', done: 0, total: 1 };
				const dir = await pickFacePackageDir(m.ext_install());
				if (!dir) {
					dialogPhase = 'confirm';
					progress = null;
					busy = null;
					return;
				}
				progress = { phase: 'install', done: 0, total: 1 };
				await installFaceExtension(dir);
				progress = { phase: 'complete', done: 1, total: 1 };
			} else {
				showToast('info', m.ext_unavailable_toast());
				closeDialog();
				busy = null;
				return;
			}
			await setFaceRecognitionExperimental(true);
			applyCatalog(await extensionCatalog());
			dialog = null;
			dialogPhase = 'confirm';
			progress = null;
			showToast('success', m.ext_install_success());
		} catch {
			dialogPhase = 'error';
			dialogError = m.ext_install_error();
			showToast('error', m.ext_install_error());
		} finally {
			busy = null;
		}
	}

	async function confirmUninstall() {
		if (!face?.installedVersion || faceBusy || !isTauri()) return;
		busy = 'remove';
		dialogPhase = 'running';
		progress = { phase: 'remove', done: 0, total: 1 };
		dialogError = null;
		try {
			applyCatalog(
				await extensionUninstall(face.id, (event) => {
					progress = event;
				})
			);
			try {
				await setFaceRecognitionExperimental(false);
			} catch {
				/* flag clear is best-effort after uninstall */
			}
			applyCatalog(await extensionCatalog());
			dialog = null;
			dialogPhase = 'confirm';
			progress = null;
			showToast('success', m.ext_uninstall_success());
		} catch {
			dialogPhase = 'error';
			dialogError = m.ext_remove_error();
			showToast('error', m.ext_remove_error());
		} finally {
			busy = null;
		}
	}

	async function cancelRunning() {
		if (dialogPhase !== 'running' || dialog !== 'install') return;
		try {
			await extensionCancelTransfer();
		} catch {
			/* cancel is best-effort */
		}
	}
</script>

<svelte:window onpointerdown={onDocPointerDown} onkeydown={onKeydown} />

<div class="relative shrink-0" data-no-drag data-extensions-menu>
	<button
		type="button"
		class="btn btn-ghost btn-sm cursor-pointer px-2"
		aria-label={m.menu_extensions()}
		aria-haspopup="menu"
		aria-expanded={open}
		onclick={toggle}
	>
		{m.menu_extensions()}
	</button>
	{#if open}
		<div
			class="rounded-box border-ink-border bg-base-100 absolute top-full left-0 z-[230] mt-1 w-72 border p-2 shadow-md"
			role="menu"
		>
			{#if !isTauri()}
				<p class="text-base-content/70 px-1 py-1 text-xs">{m.ext_desktop_only()}</p>
			{:else if face === null && items === null}
				<p class="px-1 py-1 text-xs">{m.ext_checking()}</p>
			{:else if face}
				<div class="flex items-center gap-1 px-0.5 py-0.5">
					<button
						type="button"
						class="btn btn-sm btn-primary min-w-0 flex-1 justify-start gap-2"
						class:cursor-pointer={!faceBusy &&
							!faceInstalled &&
							(face.canInstall || face.offlineFallback)}
						class:cursor-default={faceInstalled || (!face.canInstall && !face.offlineFallback)}
						class:cursor-not-allowed={faceBusy}
						class:btn-disabled={faceBusy}
						disabled={faceBusy || faceInstalled}
						aria-label={m.ext_face_title()}
						role="menuitem"
						onclick={() => onFacePrimary()}
					>
						{#if faceInstalled}
							<WashIcon icon={washIcons.check} class="size-4 shrink-0" />
						{/if}
						<span class="truncate">{m.ext_face_title()}</span>
					</button>
					{#if faceInstalled}
						<div
							class="{washRecipes.tooltipIcon('error', 'left')} shrink-0"
							data-tip={m.ext_delete()}
						>
							<button
								type="button"
								class="btn btn-ghost btn-square btn-sm btn-error"
								class:cursor-pointer={busy === null}
								class:cursor-not-allowed={busy !== null}
								class:btn-disabled={busy !== null}
								disabled={busy !== null}
								aria-label={m.ext_delete()}
								role="menuitem"
								onclick={() => onFaceDelete()}
							>
								<WashIcon icon={washIcons.trash} class="size-4" />
							</button>
						</div>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

{#if dialog}
	<!-- Portal past titlebar backdrop-filter. In-app modal only. Never window.confirm / alert. -->
	<div
		class="fixed inset-0 z-[360] flex items-center justify-center bg-black/50 p-4"
		role="presentation"
		use:portal
		onclick={() => {
			if (dialogPhase !== 'running') closeDialog();
		}}
	>
		<div
			class="bg-base-100 border-ink-border w-full max-w-sm space-y-3 rounded-lg border p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="ext-dialog-title"
			tabindex="-1"
			onclick={(event) => event.stopPropagation()}
			onkeydown={(event) => event.stopPropagation()}
		>
			{#if dialog === 'install'}
				<h2 id="ext-dialog-title" class="font-display text-primary text-xl font-bold">
					{m.ext_install_confirm_title()}
				</h2>
				{#if dialogPhase === 'confirm'}
					<p class="text-base-content/80 text-sm leading-relaxed">
						{m.ext_install_confirm_body()}
					</p>
				{:else if dialogPhase === 'running'}
					<p class="text-sm">{progressLabel}</p>
					{#if progress && progress.total > 0}
						<progress class="progress progress-primary w-full" value={progressPct} max="100"
						></progress>
						<p class="text-base-content/60 font-mono text-xs">{progressPct}%</p>
					{:else}
						<progress class="progress progress-primary w-full"></progress>
					{/if}
				{:else if dialogError}
					<p class="text-error text-sm" role="status">{dialogError}</p>
				{/if}
				<div class="flex justify-end gap-2">
					{#if dialogPhase === 'running'}
						<button
							type="button"
							class="btn btn-ghost btn-sm cursor-pointer"
							onclick={() => void cancelRunning()}
						>
							{m.ext_cancel()}
						</button>
						<button
							type="button"
							class="btn btn-primary btn-sm loading btn-disabled cursor-wait"
							disabled
							aria-busy="true"
						>
							{m.ext_install_confirm_action()}
						</button>
					{:else}
						<button type="button" class="btn btn-ghost btn-sm cursor-pointer" onclick={closeDialog}>
							{m.ext_cancel()}
						</button>
						<button
							type="button"
							class="btn btn-primary btn-sm cursor-pointer"
							onclick={() => void confirmInstall()}
						>
							{m.ext_install_confirm_action()}
						</button>
					{/if}
				</div>
			{:else}
				<h2 id="ext-dialog-title" class="font-display text-error text-xl font-bold">
					{m.ext_uninstall_confirm_title()}
				</h2>
				{#if dialogPhase === 'confirm'}
					<p class="text-base-content/80 text-sm leading-relaxed">
						{m.ext_uninstall_confirm_body()}
					</p>
				{:else if dialogPhase === 'running'}
					<p class="text-sm">{progressLabel}</p>
					{#if progress && progress.total > 0}
						<progress class="progress progress-error w-full" value={progressPct} max="100"
						></progress>
						<p class="text-base-content/60 font-mono text-xs">{progressPct}%</p>
					{:else}
						<progress class="progress progress-error w-full"></progress>
					{/if}
				{:else if dialogError}
					<p class="text-error text-sm" role="status">{dialogError}</p>
				{/if}
				<div class="flex justify-end gap-2">
					{#if dialogPhase === 'running'}
						<button type="button" class="btn btn-ghost btn-sm cursor-not-allowed" disabled>
							{m.ext_cancel()}
						</button>
						<button
							type="button"
							class="btn btn-error btn-sm loading btn-disabled cursor-wait"
							disabled
							aria-busy="true"
						>
							{m.ext_uninstall_confirm_action()}
						</button>
					{:else}
						<button type="button" class="btn btn-ghost btn-sm cursor-pointer" onclick={closeDialog}>
							{m.ext_cancel()}
						</button>
						<button
							type="button"
							class="btn btn-error btn-sm cursor-pointer"
							onclick={() => void confirmUninstall()}
						>
							{m.ext_uninstall_confirm_action()}
						</button>
					{/if}
				</div>
			{/if}
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
