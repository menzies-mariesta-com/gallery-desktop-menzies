import { z } from 'zod';

/** Mirrors the Rust edit pipeline. Enhancement is upscale, sharpen, and contrast. */

export const rotateSchema = z.enum(['none', 'cw90', 'ccw90', 'deg180']);
export type Rotate = z.infer<typeof rotateSchema>;

/**
 * Cumulative quarter turns from identity (0..3), counted clockwise.
 * image crate: rotate90 = CW, rotate270 = 90° CCW. Rust maps cw90→rotate90, ccw90→rotate270.
 */
export function normalizeQuarterTurns(turns: number): number {
	return ((Math.trunc(turns) % 4) + 4) % 4;
}

/** Map cumulative clockwise quarter turns to the absolute rotate op the pipeline accepts. */
export function rotateFromQuarterTurns(turns: number): Rotate {
	switch (normalizeQuarterTurns(turns)) {
		case 1:
			return 'cw90';
		case 2:
			return 'deg180';
		case 3:
			return 'ccw90';
		default:
			return 'none';
	}
}

/** Rotate-left (CCW) / rotate-right (CW) / 180 deltas in the clockwise quarter-turn model. */
export type RotateClick = 'left' | 'right' | 'half';

const ROTATE_CLICK_DELTA: Record<RotateClick, -1 | 1 | 2> = {
	// Left = CCW = minus one clockwise quarter turn.
	left: -1,
	// Right = CW = plus one clockwise quarter turn.
	right: 1,
	half: 2
};

export function rotateClickDelta(click: RotateClick): -1 | 1 | 2 {
	return ROTATE_CLICK_DELTA[click];
}

/** Apply a rotate-left (CCW), rotate-right (CW), or 180 click to quarter turns. */
export function applyRotateClick(turns: number, click: RotateClick): number {
	return normalizeQuarterTurns(turns + rotateClickDelta(click));
}

/** @deprecated Prefer applyRotateClick. Kept for call sites that pass a raw delta. */
export function applyRotateDelta(turns: number, delta: -1 | 1 | 2): number {
	return normalizeQuarterTurns(turns + delta);
}

/** Absolute rotate op after one UI click from the given cumulative turns. */
export function rotateOpAfterClick(turns: number, click: RotateClick): Rotate {
	return rotateFromQuarterTurns(applyRotateClick(turns, click));
}

/**
 * CSS stage nudge degrees for a click. CSS rotate() positive angles are clockwise,
 * matching image crate rotate90. Left must nudge negative (CCW).
 */
export function rotateCssNudgeDeg(click: RotateClick): number {
	if (click === 'half') return 10;
	return rotateClickDelta(click) * 8;
}

export const filterIdSchema = z.enum([
	'grayscale',
	'invert',
	'sepia',
	'blur',
	'sharpen',
	'brighten',
	'darken',
	'warm',
	'cool'
]);
export type FilterId = z.infer<typeof filterIdSchema>;

export const outputFormatSchema = z.enum(['keep', 'jpeg', 'png', 'webp', 'gif', 'bmp']);
export type OutputFormat = z.infer<typeof outputFormatSchema>;

export const resizeModeSchema = z.enum(['exact', 'fitInside']);
export type ResizeMode = z.infer<typeof resizeModeSchema>;

export const dispositionSchema = z.enum(['replace', 'saveAsNew']);
export type Disposition = z.infer<typeof dispositionSchema>;

export const cropRectSchema = z.object({
	x: z.number().int().nonnegative(),
	y: z.number().int().nonnegative(),
	width: z.number().int().positive(),
	height: z.number().int().positive()
});

export const resizeSpecSchema = z.object({
	width: z.number().int().positive().max(16384),
	height: z.number().int().positive().max(16384),
	mode: resizeModeSchema
});

export const qualityIntentSchema = z.discriminatedUnion('kind', [
	z.object({ kind: z.literal('keep') }),
	z.object({
		kind: z.literal('reduce'),
		mode: z.discriminatedUnion('kind', [
			z.object({ kind: z.literal('auto') }),
			z.object({
				kind: z.literal('manual'),
				quality: z.number().int().min(1).max(100),
				maxBytes: z.number().int().min(1024).optional()
			})
		])
	}),
	z.object({
		kind: z.literal('enhance'),
		mode: z.discriminatedUnion('kind', [
			z.object({ kind: z.literal('auto') }),
			z.object({
				kind: z.literal('manual'),
				quality: z.number().int().min(1).max(100),
				scalePercent: z.number().int().min(100).max(200),
				sharpen: z.number().int().min(0).max(100),
				contrast: z.number().int().min(-50).max(50)
			})
		])
	})
]);

export const imageEditOpsSchema = z.object({
	rotate: rotateSchema.default('none'),
	crop: cropRectSchema.optional(),
	resize: resizeSpecSchema.optional(),
	filters: z.array(filterIdSchema).max(8).default([]),
	quality: qualityIntentSchema.default({ kind: 'keep' }),
	format: outputFormatSchema.default('keep')
});

export type ImageEditOps = z.infer<typeof imageEditOpsSchema>;

export const imageFileInfoSchema = z.object({
	width: z.number().int().nonnegative(),
	height: z.number().int().nonnegative(),
	bytes: z.number().int().nonnegative(),
	format: z.string(),
	raster: z.boolean()
});

export type ImageFileInfo = z.infer<typeof imageFileInfoSchema>;

export const imageEditResultSchema = z.object({
	path: z.string(),
	width: z.number().int().nonnegative(),
	height: z.number().int().nonnegative(),
	bytes: z.number().int().nonnegative(),
	format: z.string(),
	qualityApplied: z.boolean(),
	quality: z.number().int().min(1).max(100).nullable(),
	notes: z.array(z.string()),
	disposition: z.string()
});

export type ImageEditResult = z.infer<typeof imageEditResultSchema>;

export const storedImageSchema = z.object({
	path: z.string(),
	name: z.string()
});

export type StoredImage = z.infer<typeof storedImageSchema>;

const RASTER = new Set(['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp']);

export function fileExtension(name: string): string {
	const index = name.lastIndexOf('.');
	if (index < 0 || index === name.length - 1) return '';
	return name.slice(index + 1).toLowerCase();
}

export function isRasterName(name: string): boolean {
	return RASTER.has(fileExtension(name));
}

/** Keep the Rust `suggest_edited_name` rules in sync with this helper. */
export function outputExtension(sourceName: string, format: OutputFormat): string {
	if (format === 'keep') return fileExtension(sourceName);
	if (format === 'jpeg') return 'jpg';
	return format;
}

export function replaceAllowed(sourceName: string, format: OutputFormat): boolean {
	if (format === 'keep') return isRasterName(sourceName);
	const ext = fileExtension(sourceName);
	if (format === 'jpeg') return ext === 'jpg' || ext === 'jpeg';
	return ext === format;
}

/** Suggested stem only (no extension). Pair with `outputExtension` in the save-as UI. */
export function suggestSaveAsStem(sourceName: string): string {
	const stem = sourceName.replace(/\.[^.]+$/, '') || 'image';
	return `${stem}-edited`;
}

/** Join a trimmed stem with an extension (with or without a leading dot). */
export function joinSaveAsFileName(stem: string, extension: string): string {
	const cleanStem = stem.trim();
	const ext = extension.replace(/^\./, '').toLowerCase();
	return ext ? `${cleanStem}.${ext}` : cleanStem;
}

export function suggestSaveAsName(sourceName: string, format: OutputFormat): string {
	return joinSaveAsFileName(suggestSaveAsStem(sourceName), outputExtension(sourceName, format));
}

/** Stable key for skipping duplicate live-preview IPC. */
export function opsFingerprint(ops: ImageEditOps): string {
	return JSON.stringify(ops);
}

export function galleryErrorCode(err: unknown): string | null {
	const text = errorText(err);
	return /^gallery:([a-z0-9_]+):/.exec(text)?.[1] ?? null;
}

export function errorText(err: unknown): string {
	if (typeof err === 'string') return err;
	if (err instanceof Error) return err.message;
	if (err && typeof err === 'object' && 'message' in err && typeof err.message === 'string') {
		return err.message;
	}
	return 'unknown';
}

export type BuildImageEditOpsInput = {
	rotate: Rotate;
	cropOn: boolean;
	cropX: string;
	cropY: string;
	cropW: string;
	cropH: string;
	resizeOn: boolean;
	resizeW: string;
	resizeH: string;
	resizeMode: ResizeMode;
	filters: FilterId[];
	qualityKind: 'keep' | 'reduce' | 'enhance';
	qualityMode: 'auto' | 'manual';
	qualityValue: string;
	maxBytes: string;
	scalePercent: string;
	sharpen: string;
	contrast: string;
	format: OutputFormat;
};

function intField(value: string): number {
	const trimmed = value.trim();
	if (!/^-?\d+$/.test(trimmed)) return Number.NaN;
	return Number(trimmed);
}

/** Build and Zod-validate ops from editor control state (camelCase IPC contract). */
export function buildImageEditOps(input: BuildImageEditOpsInput): ImageEditOps {
	let quality: ImageEditOps['quality'] = { kind: 'keep' };
	if (input.qualityKind === 'reduce') {
		quality =
			input.qualityMode === 'auto'
				? { kind: 'reduce', mode: { kind: 'auto' } }
				: {
						kind: 'reduce',
						mode: {
							kind: 'manual',
							quality: intField(input.qualityValue),
							...(input.maxBytes.trim() ? { maxBytes: intField(input.maxBytes) } : {})
						}
					};
	} else if (input.qualityKind === 'enhance') {
		quality =
			input.qualityMode === 'auto'
				? { kind: 'enhance', mode: { kind: 'auto' } }
				: {
						kind: 'enhance',
						mode: {
							kind: 'manual',
							quality: intField(input.qualityValue),
							scalePercent: intField(input.scalePercent),
							sharpen: intField(input.sharpen),
							contrast: intField(input.contrast)
						}
					};
	}
	return imageEditOpsSchema.parse({
		rotate: input.rotate,
		crop: input.cropOn
			? {
					x: intField(input.cropX),
					y: intField(input.cropY),
					width: intField(input.cropW),
					height: intField(input.cropH)
				}
			: undefined,
		resize: input.resizeOn
			? {
					width: intField(input.resizeW),
					height: intField(input.resizeH),
					mode: input.resizeMode
				}
			: undefined,
		filters: input.filters,
		quality,
		format: input.format
	});
}

/** True when crop/resize toggles need numeric fields before preview can run. */
export function editOpsReadyForPreview(input: BuildImageEditOpsInput): boolean {
	if (input.cropOn) {
		const fields = [input.cropX, input.cropY, input.cropW, input.cropH];
		if (fields.some((value) => value.trim() === '' || Number.isNaN(intField(value)))) {
			return false;
		}
	}
	if (input.resizeOn) {
		if (
			input.resizeW.trim() === '' ||
			input.resizeH.trim() === '' ||
			Number.isNaN(intField(input.resizeW)) ||
			Number.isNaN(intField(input.resizeH))
		) {
			return false;
		}
	}
	if (input.qualityKind !== 'keep' && input.qualityMode === 'manual') {
		if (input.qualityValue.trim() === '' || Number.isNaN(intField(input.qualityValue))) {
			return false;
		}
		if (input.qualityKind === 'enhance') {
			const fields = [input.scalePercent, input.sharpen, input.contrast];
			if (fields.some((value) => value.trim() === '' || Number.isNaN(intField(value)))) {
				return false;
			}
		}
		if (input.qualityKind === 'reduce' && input.maxBytes.trim() !== '') {
			if (Number.isNaN(intField(input.maxBytes))) return false;
		}
	}
	return true;
}
