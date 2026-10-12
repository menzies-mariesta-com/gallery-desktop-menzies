import { describe, expect, it } from 'vitest';
import { faceIndexSummarySchema, facePersonSchema } from './face-index';

describe('face index schemas', () => {
	it('accepts a cancelled summary and an unnamed person with preview', () => {
		const summary = faceIndexSummarySchema.parse({
			cancelled: true,
			scanned: 2,
			skipped: 1,
			faces: 0,
			truncated: false
		});
		expect(summary.cancelled).toBe(true);
		const person = facePersonSchema.parse({
			id: 1,
			label: null,
			createdMs: 1,
			updatedMs: 2,
			photoCount: 3,
			preview: {
				path: '/tmp/gallery-face-fixture.png',
				bbox: [10, 20, 40, 40],
				imageWidth: 200,
				imageHeight: 200
			}
		});
		expect(person.label).toBeNull();
		expect(person.photoCount).toBe(3);
		expect(person.preview?.bbox[2]).toBe(40);
	});
});
