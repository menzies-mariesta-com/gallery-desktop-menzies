import {
	applyTheme,
	initWash,
	type ThemeMode,
	type WatercolorThemeId,
	type WashRuntime
} from '@menzies-mariesta-com/menzies-design-wash-ui/core';
import {
	loadSettings,
	saveSettings,
	type AppearanceMode,
	type AppSettings
} from '$lib/store/local-storage/settings';

let wash: WashRuntime | null = null;
let media: MediaQueryList | null = null;
let appearance: AppearanceMode = 'system';
let pigment: WatercolorThemeId = 'vermilion';

function resolvedMode(pref: AppearanceMode): ThemeMode {
	if (pref === 'system') {
		if (typeof window === 'undefined') return 'light';
		return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
	}
	return pref;
}

function onSystemChange() {
	if (appearance !== 'system') return;
	applyTheme(pigment, resolvedMode('system'));
}

export function bootTheme(): AppSettings {
	const settings = loadSettings();
	appearance = settings.appearance;
	pigment = settings.pigment as WatercolorThemeId;
	const mode = resolvedMode(appearance);

	wash = initWash({
		defaultPigment: pigment,
		defaultMode: mode,
		enableEffects: true
	});

	if (typeof window !== 'undefined') {
		media = window.matchMedia('(prefers-color-scheme: dark)');
		media.addEventListener('change', onSystemChange);
	}

	return settings;
}

export function setAppearance(next: AppearanceMode): void {
	appearance = next;
	const settings = loadSettings();
	settings.appearance = next;
	saveSettings(settings);
	applyTheme(pigment, resolvedMode(next));
}

export function setPigment(id: WatercolorThemeId): void {
	pigment = id;
	const settings = loadSettings();
	settings.pigment = id;
	saveSettings(settings);
	applyTheme(id, resolvedMode(appearance));
	wash?.setPigment(id);
}

export function getAppearance(): AppearanceMode {
	return appearance;
}

export function getPigment(): WatercolorThemeId {
	return pigment;
}

export function destroyTheme(): void {
	media?.removeEventListener('change', onSystemChange);
	media = null;
	wash?.destroy();
	wash = null;
}
