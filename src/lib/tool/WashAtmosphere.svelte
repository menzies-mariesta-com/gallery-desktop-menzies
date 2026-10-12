<script lang="ts">
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import { THEME_CHANGE_EVENT } from '@menzies-mariesta-com/menzies-design-wash-ui/theme';
	import {
		MAX_ATMOSPHERE_SPLASHES,
		nextSplashTick,
		planAtmosphereSplashes,
		refreshAtmosphereThemeColors,
		type AtmospherePlan
	} from '@menzies-mariesta-com/menzies-design-wash-ui/splash';
	import { APP_ATMOSPHERE_SEED } from '$lib/tool/wash-atmosphere-seed';
	import { splashMarkupForSlot } from '$lib/tool/wash-atmosphere-markup';

	let plan = $state<AtmospherePlan>(
		planAtmosphereSplashes({ baseSeed: APP_ATMOSPHERE_SEED, count: MAX_ATMOSPHERE_SPLASHES })
	);
	let reducedMotion = $state(false);
	let animate = $state(true);
	let mountId = $state('atmo');

	let tickCounter = 0;
	const slotTimers: Array<ReturnType<typeof setTimeout> | undefined> = [];

	function slotMarkup(slotIndex: number, seed: number) {
		const slot = plan.slots[slotIndex];
		if (!slot) return '';
		return splashMarkupForSlot(slot.config, `${mountId}-s${slotIndex}-${seed}`);
	}

	function scheduleSlot(slotIndex: number) {
		if (reducedMotion || !animate) return;
		const delayMs = 6000 + ((slotIndex * 1597 + APP_ATMOSPHERE_SEED) % 8000);
		const timer = setTimeout(() => {
			if (slotTimers[slotIndex] === timer) slotTimers[slotIndex] = undefined;
			tickCounter += 1;
			plan = nextSplashTick(plan, slotIndex, tickCounter);
			scheduleSlot(slotIndex);
		}, delayMs);
		slotTimers[slotIndex] = timer;
	}

	function startSchedulers() {
		clearSchedulers();
		if (reducedMotion || !animate) return;
		for (let i = 0; i < plan.slots.length; i += 1) {
			scheduleSlot(i);
		}
	}

	function clearSchedulers() {
		for (let i = 0; i < slotTimers.length; i += 1) {
			const timer = slotTimers[i];
			if (timer) clearTimeout(timer);
			slotTimers[i] = undefined;
		}
	}

	function onThemeChange() {
		plan = refreshAtmosphereThemeColors(plan);
	}

	function onVisibilityChange() {
		animate = !document.hidden;
		if (animate) startSchedulers();
		else clearSchedulers();
	}

	onMount(() => {
		mountId = `atmo-${Math.random().toString(36).slice(2, 9)}`;
		reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		if (reducedMotion) {
			plan = planAtmosphereSplashes({ baseSeed: APP_ATMOSPHERE_SEED, count: 3 });
		}
		startSchedulers();
		window.addEventListener(THEME_CHANGE_EVENT, onThemeChange);
		document.addEventListener('visibilitychange', onVisibilityChange);

		return () => {
			clearSchedulers();
			window.removeEventListener(THEME_CHANGE_EVENT, onThemeChange);
			document.removeEventListener('visibilitychange', onVisibilityChange);
		};
	});
</script>

<div
	class="wash-atmo-layer pointer-events-none fixed inset-0 z-0 overflow-hidden"
	aria-hidden="true"
>
	{#each plan.slots as slot (slot.slotIndex)}
		{#key slot.config.seed}
			<div
				class="wash-atmo-slot absolute"
				class:wash-atmo-slot--drift={!reducedMotion}
				style="
					top: {slot.topPct}%;
					left: {slot.leftPct}%;
					width: {slot.sizePx}px;
					height: {slot.sizePx}px;
					z-index: {slot.zIndex};
					--wash-atmo-drift-delay: {slot.driftPhase}s;
					--wash-atmo-rot: {slot.rotation}deg;
				"
				in:fade={{ duration: reducedMotion ? 0 : 900 }}
				out:fade={{ duration: reducedMotion ? 0 : 700 }}
			>
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- Wash UI generated SVG only -->
				{@html slotMarkup(slot.slotIndex, slot.config.seed)}
			</div>
		{/key}
	{/each}
</div>
