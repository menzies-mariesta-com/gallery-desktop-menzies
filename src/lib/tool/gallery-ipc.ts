import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
	galleryDiskSettingsSchema,
	galleryEntrySchema,
	type GalleryDiskSettings,
	type GalleryEntry
} from '$lib/store/local-storage/settings';
import { z } from 'zod';

const galleryListSchema = z.array(galleryEntrySchema);

export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function getDefaultPicturesDir(): Promise<string> {
	if (!isTauri()) {
		return '';
	}
	return invoke<string>('get_default_pictures_dir');
}

export async function listGalleryEntries(
	dir: string,
	allowedRoot?: string | null
): Promise<GalleryEntry[]> {
	if (!isTauri()) {
		return [];
	}
	const raw = await invoke<unknown>('list_gallery_entries', {
		dir,
		allowedRoot: allowedRoot ?? null
	});
	return galleryListSchema.parse(raw);
}

export async function loadGallerySettings(): Promise<GalleryDiskSettings> {
	if (!isTauri()) {
		return galleryDiskSettingsSchema.parse({});
	}
	const raw = await invoke<unknown>('load_gallery_settings');
	return galleryDiskSettingsSchema.parse(raw);
}

export async function saveGallerySettings(settings: GalleryDiskSettings): Promise<void> {
	if (!isTauri()) return;
	await invoke('save_gallery_settings', { settings });
}

export function imageSrc(path: string): string {
	if (!path) return '';
	if (!isTauri()) return path;
	return convertFileSrc(path);
}
