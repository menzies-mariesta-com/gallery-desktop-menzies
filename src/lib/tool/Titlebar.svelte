<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		washRecipes,
		watercolorThemes,
		type WatercolorThemeId
	} from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import { m } from '$lib/paraglide/messages.js';
	import { APP_VERSION } from '$lib/tool/app-version';
	import { getAppearance, getPigment, setAppearance, setPigment } from '$lib/tool/theme';
	import { runUpdateCheck } from '$lib/tool/updater';
	import type { AppearanceMode } from '$lib/store/local-storage/settings';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { washIcons } from '$lib/tool/wash-icons';
	import { requestGalleryRefresh } from '$lib/tool/gallery-refresh';

	let maximized = $state(false);
	let appearance = $state<AppearanceMode>(getAppearance());
	let pigment = $state<WatercolorThemeId>(getPigment());
	let themeOpen = $state(false);
	let updating = $state(false);
	let toast = $state<{ tone: 'success' | 'error' | 'info' | 'warning'; text: string } | null>(null);
	let toastTimer: ReturnType<typeof setTimeout> | null = null;

	function showToast(tone: 'success' | 'error' | 'info' | 'warning', text: string) {
		if (toastTimer) clearTimeout(toastTimer);
		toast = { tone, text };
		toastTimer = setTimeout(() => {
			toast = null;
			toastTimer = null;
		}, 5000);
	}

	async function refreshMaximized() {
		try {
			maximized = await getCurrentWindow().isMaximized();
		} catch {
			maximized = false;
		}
	}

	async function minimize() {
		try {
			await getCurrentWindow().minimize();
		} catch {
			/* browser preview */
		}
	}

	async function toggleMaximize() {
		try {
			await getCurrentWindow().toggleMaximize();
			await refreshMaximized();
		} catch {
			/* browser preview */
		}
	}

	async function close() {
		try {
			await getCurrentWindow().close();
		} catch {
			/* browser preview */
		}
	}

	async function startDrag(event: PointerEvent) {
		if (event.button !== 0) return;
		const target = event.target as HTMLElement | null;
		if (target?.closest('[data-no-drag]')) return;
		try {
			await getCurrentWindow().startDragging();
		} catch {
			/* browser preview */
		}
	}

	function chooseAppearance(next: AppearanceMode) {
		appearance = next;
		setAppearance(next);
		themeOpen = false;
	}

	function choosePigment(next: WatercolorThemeId) {
		pigment = next;
		setPigment(next);
		themeOpen = false;
	}

	async function onDownloads() {
		if (updating) return;
		updating = true;
		try {
			const result = await runUpdateCheck((version) =>
				window.confirm(m.update_available_confirm({ version }))
			);
			switch (result.outcome) {
				case 'up_to_date':
					showToast('success', m.update_up_to_date());
					break;
				case 'declined':
					showToast('info', m.update_declined());
					break;
				case 'unavailable':
					showToast('warning', m.update_unavailable());
					break;
				case 'error':
					showToast('error', m.update_error());
					break;
				case 'installed':
					break;
			}
		} finally {
			updating = false;
		}
	}

	function onDocPointerDown(event: PointerEvent) {
		const el = event.target as HTMLElement | null;
		if (!el?.closest('[data-theme-menu]')) themeOpen = false;
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') themeOpen = false;
	}
</script>

<svelte:window onpointerdown={onDocPointerDown} onkeydown={onKeydown} />

<header
	data-titlebar
	class="border-ink-border/15 bg-base-100/90 relative z-[200] flex h-10 shrink-0 items-center overflow-visible border-b px-1 backdrop-blur-sm"
>
	<div
		class="flex min-w-[8rem] flex-1 cursor-grab items-center gap-2 px-2 active:cursor-grabbing"
		onpointerdown={startDrag}
		role="presentation"
	>
		<span class="inline-flex max-w-full items-baseline gap-1">
			<span
				class="font-display text-base-content text-sm font-semibold tracking-wide whitespace-nowrap"
				>{m.app_title()}</span
			>
			<span class="relative top-[0.15em] shrink-0 text-[0.65em] leading-none italic opacity-50"
				>{APP_VERSION}</span
			>
		</span>
	</div>

	<div
		class="relative z-[210] flex shrink-0 items-center gap-0.5 overflow-visible"
		data-no-drag
		data-theme-menu
	>
		<div
			class="{washRecipes.tooltipIcon('secondary', 'bottom')} relative z-[220]"
			data-tip={m.refresh()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
				aria-label={m.refresh()}
				onclick={() => requestGalleryRefresh()}
			>
				<WashIcon icon={washIcons['refresh-cw']} class="size-4" />
			</button>
		</div>

		<div
			class="{washRecipes.tooltipIcon('secondary', 'bottom')} relative z-[220]"
			data-tip={m.theme_menu()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
				aria-label={m.theme_menu()}
				aria-expanded={themeOpen}
				aria-haspopup="menu"
				onclick={() => (themeOpen = !themeOpen)}
			>
				<WashIcon icon={washIcons.sun} class="size-4" />
			</button>
		</div>

		{#if themeOpen}
			<div
				class="rounded-box border-ink-border bg-base-100 absolute top-full right-0 z-[230] mt-1 flex max-h-[min(70vh,28rem)] w-56 flex-col overflow-x-hidden overflow-y-hidden border p-1 shadow-md"
				role="menu"
			>
				<div class="shrink-0">
					<p class="text-base-content/60 px-2 py-1 text-xs font-medium">{m.theme_mode_section()}</p>
					<ul class="menu menu-sm flex w-full flex-col flex-nowrap p-0">
						<li role="none">
							<button
								type="button"
								class="cursor-pointer"
								class:active={appearance === 'light'}
								role="menuitem"
								onclick={() => chooseAppearance('light')}>{m.theme_light()}</button
							>
						</li>
						<li role="none">
							<button
								type="button"
								class="cursor-pointer"
								class:active={appearance === 'dark'}
								role="menuitem"
								onclick={() => chooseAppearance('dark')}>{m.theme_dark()}</button
							>
						</li>
						<li role="none">
							<button
								type="button"
								class="cursor-pointer"
								class:active={appearance === 'system'}
								role="menuitem"
								onclick={() => chooseAppearance('system')}>{m.theme_system()}</button
							>
						</li>
					</ul>
					<div class="border-ink-border/40 my-1 border-t"></div>
					<p class="text-base-content/60 px-2 py-1 text-xs font-medium">
						{m.theme_pigment_section()}
					</p>
				</div>
				<ul
					class="menu menu-sm flex min-h-0 w-full flex-1 flex-col flex-nowrap overflow-x-hidden overflow-y-auto overscroll-contain p-0"
				>
					{#each watercolorThemes as item (item.id)}
						<li role="none" class="w-full">
							<button
								type="button"
								class="w-full cursor-pointer"
								class:active={pigment === item.id}
								role="menuitem"
								onclick={() => choosePigment(item.id)}
							>
								<span
									class="border-ink-border size-3.5 shrink-0 rounded-full border"
									style="background: radial-gradient(circle at 35% 30%, color-mix(in oklab, white 70%, transparent) 0%, {item.swatch} 60%, color-mix(in oklab, {item.swatch} 70%, black) 100%)"
									aria-hidden="true"
								></span>
								<span class="min-w-0 truncate">{item.label}</span>
							</button>
						</li>
					{/each}
				</ul>
			</div>
		{/if}

		<div
			class="{washRecipes.tooltipIcon('primary', 'bottom')} relative z-[220]"
			data-tip={updating ? m.update_checking() : m.downloads()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-primary"
				class:cursor-pointer={!updating}
				class:cursor-not-allowed={updating}
				class:cursor-wait={updating}
				class:loading={updating}
				class:btn-disabled={updating}
				disabled={updating}
				aria-busy={updating}
				aria-label={updating ? m.update_checking() : m.downloads()}
				onclick={onDownloads}
			>
				<WashIcon icon={washIcons.download} class="size-4" />
			</button>
		</div>

		<div
			class="{washRecipes.tooltipIcon('secondary', 'bottom')} relative z-[220]"
			data-tip={m.window_minimize()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
				aria-label={m.window_minimize()}
				onclick={minimize}
			>
				<WashIcon icon={washIcons.minus} class="size-3.5" />
			</button>
		</div>

		<div
			class="{washRecipes.tooltipIcon('secondary', 'bottom')} relative z-[220]"
			data-tip={maximized ? m.window_restore() : m.window_maximize()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-secondary cursor-pointer"
				aria-label={maximized ? m.window_restore() : m.window_maximize()}
				onclick={toggleMaximize}
			>
				{#if maximized}
					<WashIcon icon={washIcons.copy} class="size-3.5" />
				{:else}
					<WashIcon icon={washIcons.square} class="size-3.5" />
				{/if}
			</button>
		</div>

		<div
			class="{washRecipes.tooltipIcon('error', 'bottom')} relative z-[220]"
			data-tip={m.window_close()}
		>
			<button
				type="button"
				class="btn btn-ghost btn-square btn-sm btn-error cursor-pointer"
				aria-label={m.window_close()}
				onclick={close}
			>
				<WashIcon icon={washIcons.x} class="size-3.5" />
			</button>
		</div>
	</div>
</header>

{#if toast}
	<div class="toast toast-bottom toast-end z-[300]">
		<div class="alert alert-{toast.tone} shadow-lg">
			<span>{toast.text}</span>
		</div>
	</div>
{/if}
