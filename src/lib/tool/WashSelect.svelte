<script lang="ts">
	import {
		createWashId,
		DROPDOWN_PANEL_Z,
		dropdownPanelStyle,
		dropdownPlacementClassName,
		measureDropdownPlacement,
		sameDropdownPlacement,
		type DropdownPlacement
	} from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { washIcons } from '$lib/tool/wash-icons';

	export type WashSelectOption = {
		value: string;
		label: string;
		disabled?: boolean;
	};

	type MenuWidth = 'trigger' | 'auto' | number | string;

	type Props = {
		options: readonly WashSelectOption[];
		value?: string;
		label?: string;
		placeholder?: string;
		required?: boolean;
		disabled?: boolean;
		id?: string;
		class?: string;
		menuWidth?: MenuWidth;
		menuClass?: string;
		'aria-label'?: string;
		onchange?: (value: string) => void;
	};

	let {
		options,
		value = $bindable(''),
		label,
		placeholder = 'Choose…',
		required = false,
		disabled = false,
		id,
		class: className = '',
		menuWidth = 'trigger',
		menuClass = '',
		'aria-label': ariaLabel,
		onchange
	}: Props = $props();

	const fallbackId = createWashId('select');
	const selectId = $derived(id ?? fallbackId);
	const labelId = $derived(`${selectId}-label`);
	const listId = $derived(`${selectId}-list`);

	let open = $state(false);
	let rootEl = $state<HTMLDivElement | null>(null);
	let triggerEl = $state<HTMLButtonElement | null>(null);
	let triggerWidthPx = $state<number | null>(null);
	let placement = $state<DropdownPlacement>({ end: false, top: false, maxHeight: 320 });

	const menuOpen = $derived(open && !disabled);
	const selected = $derived(options.find((opt) => opt.value === value) ?? null);

	const rootClass = $derived(
		dropdownPlacementClassName(
			placement,
			`dropdown-no-hover w-full ${menuOpen ? 'dropdown-open' : ''}`.trim()
		)
	);

	const triggerClass = $derived(
		[
			'select wash-select--icon inline-flex w-full cursor-pointer items-center justify-between gap-2 border-ink-border text-start font-normal',
			disabled && 'cursor-not-allowed opacity-60',
			className
		]
			.filter(Boolean)
			.join(' ')
	);

	const widthStyle = $derived.by((): Record<string, string | number> | undefined => {
		if (menuWidth === 'auto') return undefined;
		if (menuWidth === 'trigger') {
			if (triggerWidthPx == null || triggerWidthPx <= 0) return undefined;
			return {
				width: triggerWidthPx,
				minWidth: triggerWidthPx,
				maxWidth: triggerWidthPx
			};
		}
		if (typeof menuWidth === 'number') {
			return { width: menuWidth, minWidth: menuWidth, maxWidth: menuWidth };
		}
		return { width: menuWidth, minWidth: menuWidth, maxWidth: menuWidth };
	});

	const menuPanelStyle = $derived.by(() => {
		const panel = dropdownPanelStyle(placement);
		const width = widthStyle;
		const parts = [
			`max-height: ${panel.maxHeight}px`,
			`--wash-dropdown-max-h: ${panel['--wash-dropdown-max-h']}`
		];
		if (width) {
			parts.push(`width: ${typeof width.width === 'number' ? `${width.width}px` : width.width}`);
			parts.push(
				`min-width: ${typeof width.minWidth === 'number' ? `${width.minWidth}px` : width.minWidth}`
			);
			parts.push(
				`max-width: ${typeof width.maxWidth === 'number' ? `${width.maxWidth}px` : width.maxWidth}`
			);
		}
		return parts.join('; ');
	});

	function panelWidthEstimate(): number {
		if (menuWidth === 'trigger' && triggerWidthPx != null) return triggerWidthPx;
		if (typeof menuWidth === 'number') return menuWidth;
		return 320;
	}

	function updatePlacement() {
		if (!rootEl || !menuOpen) return;
		const next = measureDropdownPlacement(rootEl, {
			panelWidth: panelWidthEstimate(),
			panelHeight: 280
		});
		if (!sameDropdownPlacement(placement, next)) placement = next;
	}

	function measureTrigger() {
		if (!triggerEl || menuWidth !== 'trigger') {
			triggerWidthPx = null;
			return;
		}
		const width = triggerEl.getBoundingClientRect().width;
		triggerWidthPx = width > 0 ? Math.round(width) : null;
	}

	$effect(() => {
		if (!menuOpen) return;
		measureTrigger();
		updatePlacement();

		const onPointerDown = (event: PointerEvent) => {
			const el = rootEl;
			if (!el) return;
			if (event.target instanceof Node && !el.contains(event.target)) open = false;
		};
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') open = false;
		};
		const onViewport = () => {
			measureTrigger();
			updatePlacement();
		};

		document.addEventListener('pointerdown', onPointerDown);
		document.addEventListener('keydown', onKeyDown);
		window.addEventListener('resize', onViewport);
		window.addEventListener('scroll', onViewport, true);

		const ro =
			typeof ResizeObserver !== 'undefined' && triggerEl
				? new ResizeObserver(() => {
						measureTrigger();
						updatePlacement();
					})
				: null;
		if (triggerEl) ro?.observe(triggerEl);

		return () => {
			document.removeEventListener('pointerdown', onPointerDown);
			document.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('resize', onViewport);
			window.removeEventListener('scroll', onViewport, true);
			ro?.disconnect();
		};
	});

	function toggle() {
		if (disabled) return;
		open = !open;
	}

	function commit(next: string) {
		value = next;
		onchange?.(next);
		open = false;
	}
</script>

<div class={rootClass} bind:this={rootEl}>
	<div class="form-control w-full min-w-0">
		{#if label}
			<span class="label">
				<span class="label-text text-base font-medium" id={labelId}>
					{label}{#if required}<span
							class="text-error align-top text-sm leading-none"
							aria-hidden="true">*</span
						>{/if}
				</span>
			</span>
		{/if}

		<button
			bind:this={triggerEl}
			type="button"
			id={selectId}
			role="combobox"
			aria-expanded={menuOpen}
			aria-controls={listId}
			aria-haspopup="listbox"
			aria-required={required || undefined}
			aria-labelledby={label ? labelId : undefined}
			aria-label={ariaLabel}
			{disabled}
			class={triggerClass}
			onclick={toggle}
		>
			<span
				class={selected
					? 'min-w-0 flex-1 truncate'
					: 'text-base-content/50 min-w-0 flex-1 truncate'}
			>
				{selected?.label ?? placeholder}
			</span>
			<WashIcon
				icon={washIcons['chevron-down']}
				class="size-5 shrink-0 opacity-60 {disabled ? 'opacity-40' : ''}"
			/>
		</button>
	</div>

	{#if menuOpen}
		<div
			class={[
				'dropdown-content',
				DROPDOWN_PANEL_Z,
				placement.top ? 'mb-1' : 'mt-1',
				menuWidth === 'auto' ? 'w-full max-w-[min(100vw-1rem,24rem)]' : '',
				'rounded-box border-ink-border bg-base-100 overflow-x-hidden overflow-y-auto border p-2 shadow-[var(--shadow-paper-md)]',
				menuClass
			]
				.filter(Boolean)
				.join(' ')}
			style={menuPanelStyle}
		>
			<ul
				id={listId}
				role="listbox"
				class="menu rounded-box flex w-full flex-col flex-nowrap overflow-x-hidden overflow-y-auto p-0 text-base"
				tabindex="-1"
			>
				{#if options.length === 0}
					<li class="text-ink-muted px-3 py-2 text-sm">No options</li>
				{:else}
					{#each options as opt (opt.value)}
						{@const active = value === opt.value}
						<li role="option" aria-selected={active}>
							<button
								type="button"
								disabled={opt.disabled}
								class={[
									'cursor-pointer',
									active && 'active',
									opt.disabled && 'cursor-not-allowed opacity-50'
								]
									.filter(Boolean)
									.join(' ')}
								onmousedown={(event) => event.preventDefault()}
								onclick={() => {
									if (!opt.disabled) commit(opt.value);
								}}
							>
								<span class="truncate">{opt.label}</span>
								{#if active}
									<WashIcon icon={washIcons.check} class="size-4 opacity-70" />
								{/if}
							</button>
						</li>
					{/each}
				{/if}
			</ul>
		</div>
	{/if}
</div>
