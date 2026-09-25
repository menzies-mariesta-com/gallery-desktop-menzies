<script lang="ts">
	import { onMount } from 'svelte';
	import { washRecipes } from '@menzies-mariesta-com/menzies-design-wash-ui/core';
	import favicon from '$lib/asset/image/favicon.svg';
	import '$lib/asset/style/app.css';
	import Titlebar from '$lib/tool/Titlebar.svelte';
	import { bootTheme, destroyTheme } from '$lib/tool/theme';
	import { m } from '$lib/paraglide/messages.js';

	let { children } = $props();

	onMount(() => {
		bootTheme();
		return () => destroyTheme();
	});
</script>

<svelte:head>
	<title>{m.app_title()}</title>
	<link rel="icon" href={favicon} />
</svelte:head>

<div
	class="{washRecipes.washShellFlat} flex h-dvh max-h-dvh min-h-0 w-full flex-col overflow-hidden"
>
	<Titlebar />
	<main class="relative z-0 flex min-h-0 w-full flex-1 flex-col overflow-hidden p-2 sm:p-3 md:p-4">
		{@render children()}
	</main>
</div>
