import { describe, expect, it } from 'vitest';
import {
	galleryEntrySchema,
	galleryDiskSettingsSchema,
	imageEntrySchema
} from '$lib/store/local-storage/settings';

describe('gallery schemas', () => {
	it('parses gallery folder and image entries', () => {
		const folder = galleryEntrySchema.parse({
			kind: 'folder',
			path: '/home/user/Pictures/Vacation',
			name: 'Vacation',
			modifiedMs: 1
		});
		expect(folder.kind).toBe('folder');

		const image = galleryEntrySchema.parse({
			kind: 'image',
			path: '/home/user/Pictures/a.png',
			name: 'a.png',
			size: 12,
			modifiedMs: 1
		});
		expect(image.name).toBe('a.png');
	});

	it('still parses legacy image entries', () => {
		const entry = imageEntrySchema.parse({
			path: '/home/user/Pictures/a.png',
			name: 'a.png',
			size: 12,
			modifiedMs: 1
		});
		expect(entry.name).toBe('a.png');
	});

	it('defaults disk settings with sort and filter prefs', () => {
		const settings = galleryDiskSettingsSchema.parse({});
		expect(settings.onboarded).toBe(false);
		expect(settings.recentFolders).toEqual([]);
		expect(settings.sortBy).toBe('name');
		expect(settings.sortDir).toBe('asc');
		expect(settings.typeFilter).toBe('all');
	});
});
