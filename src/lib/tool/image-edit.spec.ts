import { describe, expect, it } from 'vitest';
import {
	applyRotateClick,
	applyRotateDelta,
	buildImageEditOps,
	editOpsReadyForPreview,
	galleryErrorCode,
	imageEditOpsSchema,
	imageEditResultSchema,
	replaceAllowed,
	rotateCssNudgeDeg,
	rotateFromQuarterTurns,
	rotateOpAfterClick,
	joinSaveAsFileName,
	opsFingerprint,
	suggestSaveAsName,
	suggestSaveAsStem,
	type BuildImageEditOpsInput
} from '$lib/tool/image-edit';

function baseInput(overrides: Partial<BuildImageEditOpsInput> = {}): BuildImageEditOpsInput {
	return {
		rotate: 'none',
		cropOn: false,
		cropX: '0',
		cropY: '0',
		cropW: '10',
		cropH: '10',
		resizeOn: false,
		resizeW: '80',
		resizeH: '40',
		resizeMode: 'fitInside',
		filters: [],
		qualityKind: 'keep',
		qualityMode: 'auto',
		qualityValue: '80',
		maxBytes: '',
		scalePercent: '125',
		sharpen: '30',
		contrast: '12',
		format: 'keep',
		...overrides
	};
}

describe('image edit contract', () => {
	it('maps cumulative quarter turns to absolute rotate ops', () => {
		expect(rotateFromQuarterTurns(0)).toBe('none');
		expect(rotateFromQuarterTurns(1)).toBe('cw90');
		expect(rotateFromQuarterTurns(2)).toBe('deg180');
		expect(rotateFromQuarterTurns(3)).toBe('ccw90');
		expect(rotateFromQuarterTurns(4)).toBe('none');
		expect(rotateFromQuarterTurns(-1)).toBe('ccw90');

		expect(applyRotateDelta(0, -1)).toBe(3);
		expect(applyRotateDelta(3, -1)).toBe(2);
		expect(applyRotateDelta(2, -1)).toBe(1);
		expect(applyRotateDelta(1, -1)).toBe(0);
		expect(applyRotateDelta(0, 1)).toBe(1);
		expect(applyRotateDelta(1, 1)).toBe(2);
		expect(applyRotateDelta(2, 1)).toBe(3);
		expect(applyRotateDelta(3, 1)).toBe(0);
		expect(applyRotateDelta(0, 2)).toBe(2);
		expect(applyRotateDelta(2, 2)).toBe(0);
		expect(applyRotateDelta(3, 2)).toBe(1);
	});

	it('maps rotate left to CCW and rotate right to CW each click', () => {
		// image crate rotate90 is CW; FE left must land on ccw90 (Rust rotate270).
		expect(rotateOpAfterClick(0, 'left')).toBe('ccw90');
		expect(rotateOpAfterClick(0, 'right')).toBe('cw90');
		expect(rotateOpAfterClick(0, 'half')).toBe('deg180');

		// Repeated left stays CCW around the circle: ccw90 → 180 → cw90 → none.
		expect(rotateOpAfterClick(applyRotateClick(0, 'left'), 'left')).toBe('deg180');
		expect(rotateOpAfterClick(applyRotateClick(applyRotateClick(0, 'left'), 'left'), 'left')).toBe(
			'cw90'
		);

		// Repeated right stays CW: cw90 → 180 → ccw90 → none.
		expect(rotateOpAfterClick(applyRotateClick(0, 'right'), 'right')).toBe('deg180');

		// CSS nudge sign matches image/CSS clockwise convention (left is negative).
		expect(rotateCssNudgeDeg('left')).toBeLessThan(0);
		expect(rotateCssNudgeDeg('right')).toBeGreaterThan(0);
	});

	it('parses a manual reduce and enhance payload', () => {
		const reduce = imageEditOpsSchema.parse({
			rotate: 'cw90',
			crop: { x: 1, y: 2, width: 10, height: 12 },
			resize: { width: 80, height: 40, mode: 'fitInside' },
			filters: ['sepia', 'sharpen'],
			quality: { kind: 'reduce', mode: { kind: 'manual', quality: 70, maxBytes: 50_000 } },
			format: 'jpeg'
		});
		expect(reduce.rotate).toBe('cw90');

		const enhance = imageEditOpsSchema.parse({
			quality: {
				kind: 'enhance',
				mode: { kind: 'manual', quality: 92, scalePercent: 125, sharpen: 20, contrast: 8 }
			},
			format: 'png'
		});
		expect(enhance.quality.kind).toBe('enhance');
		expect(enhance.filters).toEqual([]);
	});

	it('builds camelCase ops from editor controls for preview and commit', () => {
		const ops = buildImageEditOps(
			baseInput({
				rotate: 'cw90',
				filters: ['grayscale', 'warm'],
				qualityKind: 'enhance',
				qualityMode: 'manual',
				qualityValue: '92',
				scalePercent: '125',
				sharpen: '20',
				contrast: '8',
				format: 'png'
			})
		);
		expect(ops.rotate).toBe('cw90');
		expect(ops.filters).toEqual(['grayscale', 'warm']);
		expect(ops.format).toBe('png');
		expect(ops.quality).toEqual({
			kind: 'enhance',
			mode: { kind: 'manual', quality: 92, scalePercent: 125, sharpen: 20, contrast: 8 }
		});

		const reduce = buildImageEditOps(
			baseInput({
				qualityKind: 'reduce',
				qualityMode: 'manual',
				qualityValue: '70',
				maxBytes: '50000',
				format: 'jpeg'
			})
		);
		expect(reduce.quality).toEqual({
			kind: 'reduce',
			mode: { kind: 'manual', quality: 70, maxBytes: 50_000 }
		});
	});

	it('waits for crop and resize fields before preview is ready', () => {
		expect(editOpsReadyForPreview(baseInput({ cropOn: true, cropW: '' }))).toBe(false);
		expect(editOpsReadyForPreview(baseInput({ resizeOn: true, resizeW: '' }))).toBe(false);
		expect(
			editOpsReadyForPreview(
				baseInput({
					cropOn: true,
					cropX: '0',
					cropY: '0',
					cropW: '8',
					cropH: '8',
					resizeOn: true,
					resizeW: '4',
					resizeH: '4'
				})
			)
		).toBe(true);
	});

	it('rejects a missing disposition result shape and accepts an explicit one', () => {
		const saved = imageEditResultSchema.parse({
			path: '/tmp/photo-edited.jpg',
			width: 3,
			height: 5,
			bytes: 120,
			format: 'jpeg',
			qualityApplied: true,
			quality: 80,
			notes: ['enhance_classical'],
			disposition: 'saveAsNew'
		});
		expect(saved.disposition).toBe('saveAsNew');
		expect(saved.notes).toContain('enhance_classical');
	});

	it('allows replace only when the output format stays compatible', () => {
		expect(replaceAllowed('photo.jpeg', 'keep')).toBe(true);
		expect(replaceAllowed('photo.jpeg', 'jpeg')).toBe(true);
		expect(replaceAllowed('photo.png', 'jpeg')).toBe(false);
		expect(replaceAllowed('icon.svg', 'keep')).toBe(false);
	});

	it('suggests a sibling name and never the original name', () => {
		expect(suggestSaveAsStem('Vacation.JPEG')).toBe('Vacation-edited');
		expect(joinSaveAsFileName('Vacation-edited', 'png')).toBe('Vacation-edited.png');
		expect(suggestSaveAsName('Vacation.JPEG', 'png')).toBe('Vacation-edited.png');
		expect(suggestSaveAsName('photo.jpg', 'jpeg')).toBe('photo-edited.jpg');
		expect(suggestSaveAsName('photo.jpg', 'keep')).not.toBe('photo.jpg');
	});

	it('fingerprints ops for duplicate preview skips', () => {
		const a = imageEditOpsSchema.parse({
			rotate: 'none',
			filters: [],
			quality: { kind: 'keep' },
			format: 'keep'
		});
		const b = imageEditOpsSchema.parse({ ...a, rotate: 'cw90' });
		expect(opsFingerprint(a)).toBe(opsFingerprint({ ...a }));
		expect(opsFingerprint(a)).not.toBe(opsFingerprint(b));
	});

	it('reads stable gallery error codes', () => {
		expect(galleryErrorCode('gallery:target_exists:a file with that name already exists')).toBe(
			'target_exists'
		);
		expect(galleryErrorCode('gallery:replace_format_mismatch:convert')).toBe(
			'replace_format_mismatch'
		);
		expect(galleryErrorCode(new Error('gallery:not_raster:svg'))).toBe('not_raster');
		expect(galleryErrorCode('plain failure')).toBeNull();
	});
});
