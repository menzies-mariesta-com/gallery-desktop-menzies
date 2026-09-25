import { z } from 'zod';
import { isWatercolorTheme } from '@menzies-mariesta-com/menzies-design-wash-ui/core';

/** Appearance preference: Wash paper mode or follow OS. */
export const appearanceModeSchema = z.enum(['light', 'dark', 'system']);
export type AppearanceMode = z.infer<typeof appearanceModeSchema>;

/** Default Wash pigment (brand theme). Id is `vermilion` in Wash UI. */
export const DEFAULT_PIGMENT = 'vermilion' as const;

export const settingsSchema = z.object({
	appearance: appearanceModeSchema.default('system'),
	pigment: z
		.string()
		.min(1)
		.default(DEFAULT_PIGMENT)
		.transform((value) => (isWatercolorTheme(value) ? value : DEFAULT_PIGMENT)),
	onboarded: z.boolean().default(false)
});

export type AppSettings = z.infer<typeof settingsSchema>;

export const SETTINGS_STORAGE_KEY = 'com.mariesta.menzies.gallery-desktop-menzies.settings';

export function loadSettings(): AppSettings {
	if (typeof localStorage === 'undefined') {
		return settingsSchema.parse({});
	}
	try {
		const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
		if (!raw) return settingsSchema.parse({});
		return settingsSchema.parse(JSON.parse(raw));
	} catch {
		return settingsSchema.parse({});
	}
}

export function saveSettings(settings: AppSettings): void {
	if (typeof localStorage === 'undefined') return;
	localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

export const galleryEntryKindSchema = z.enum(['folder', 'image']);

export const galleryEntrySchema = z.object({
	kind: galleryEntryKindSchema,
	path: z.string(),
	name: z.string(),
	size: z.number().nonnegative().optional().nullable(),
	modifiedMs: z.number().nonnegative().nullable().optional()
});

export type GalleryEntry = z.infer<typeof galleryEntrySchema>;

/** @deprecated Prefer GalleryEntry; kept for tests migrating from image-only DTOs. */
export const imageEntrySchema = z.object({
	path: z.string(),
	name: z.string(),
	size: z.number().nonnegative(),
	modifiedMs: z.number().nonnegative().nullable().optional()
});

export type ImageEntry = z.infer<typeof imageEntrySchema>;

export const sortBySchema = z.enum(['name', 'size', 'modified']);
export type SortBy = z.infer<typeof sortBySchema>;

export const sortDirSchema = z.enum(['asc', 'desc']);
export type SortDir = z.infer<typeof sortDirSchema>;

export const typeFilterSchema = z.enum(['all', 'jpg', 'png', 'webp', 'gif', 'bmp', 'svg']);
export type TypeFilter = z.infer<typeof typeFilterSchema>;

export const galleryDiskSettingsSchema = z.object({
	lastFolder: z.string().nullable().optional(),
	recentFolders: z.array(z.string()).default([]),
	onboarded: z.boolean().default(false),
	sortBy: sortBySchema.default('name'),
	sortDir: sortDirSchema.default('asc'),
	typeFilter: typeFilterSchema.default('all')
});

export type GalleryDiskSettings = z.infer<typeof galleryDiskSettingsSchema>;
