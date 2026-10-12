import {
	buildSplashSvgMarkup,
	type WatercolorSplashConfig
} from '@menzies-mariesta-com/menzies-design-wash-ui/splash';

export function splashMarkupForSlot(config: WatercolorSplashConfig, idPrefix: string): string {
	const raw = buildSplashSvgMarkup(config, { inline: true });
	return raw.replace(/wash-splash-(filter|gradient)-/g, `wash-splash-$1-${idPrefix}-`);
}
