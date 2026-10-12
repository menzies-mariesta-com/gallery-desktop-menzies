import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
	galleryDiskSettingsSchema,
	galleryEntrySchema,
	type GalleryDiskSettings,
	type GalleryEntry
} from '$lib/store/local-storage/settings';
import {
	imageEditOpsSchema,
	imageEditResultSchema,
	imageFileInfoSchema,
	storedImageSchema,
	type Disposition,
	type ImageEditOps,
	type ImageEditResult,
	type ImageFileInfo,
	type OutputFormat,
	type StoredImage
} from '$lib/tool/image-edit';
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

/** Cap convertFileSrc work; paths are stable asset URLs for the session. */
const imageSrcCache = new Map<string, string>();
const IMAGE_SRC_CACHE_MAX = 512;

export function imageSrc(path: string): string {
	if (!path) return '';
	if (!isTauri()) return path;
	const hit = imageSrcCache.get(path);
	if (hit !== undefined) return hit;
	const src = convertFileSrc(path);
	if (imageSrcCache.size >= IMAGE_SRC_CACHE_MAX) {
		const oldest = imageSrcCache.keys().next().value;
		if (oldest !== undefined) imageSrcCache.delete(oldest);
	}
	imageSrcCache.set(path, src);
	return src;
}

/** Drop a path from the URL cache (e.g. after discarding an edit preview file). */
export function forgetImageSrc(path: string): void {
	if (path) imageSrcCache.delete(path);
}

export async function imageFileInfo(
	path: string,
	allowedRoot?: string | null
): Promise<ImageFileInfo> {
	const raw = await invoke<unknown>('image_file_info', {
		path,
		allowedRoot: allowedRoot ?? null
	});
	return imageFileInfoSchema.parse(raw);
}

export async function suggestEditedName(sourceName: string, format: OutputFormat): Promise<string> {
	const raw = await invoke<unknown>('suggest_edited_name', { sourceName, format });
	return z.string().parse(raw);
}

export async function previewImageEdit(
	sourcePath: string,
	allowedRoot: string | null,
	ops: ImageEditOps
): Promise<ImageEditResult> {
	const parsed = imageEditOpsSchema.parse(ops);
	const raw = await invoke<unknown>('preview_image_edit', {
		sourcePath,
		allowedRoot,
		ops: parsed
	});
	return imageEditResultSchema.parse(raw);
}

export async function commitImageEdit(
	sourcePath: string,
	allowedRoot: string | null,
	ops: ImageEditOps,
	disposition: Disposition,
	saveAsName?: string
): Promise<ImageEditResult> {
	const parsed = imageEditOpsSchema.parse(ops);
	const raw = await invoke<unknown>('commit_image_edit', {
		sourcePath,
		allowedRoot,
		ops: parsed,
		disposition,
		saveAsName: saveAsName ?? null
	});
	return imageEditResultSchema.parse(raw);
}

export async function discardEditPreview(path: string): Promise<void> {
	if (!isTauri() || !path) return;
	await invoke('discard_edit_preview', { path });
}

export async function duplicateGalleryImage(
	sourcePath: string,
	allowedRoot?: string | null
): Promise<StoredImage> {
	const raw = await invoke<unknown>('duplicate_gallery_image', {
		sourcePath,
		allowedRoot: allowedRoot ?? null
	});
	return storedImageSchema.parse(raw);
}

/** Copy image pixels to the OS clipboard for paste in other apps. */
export async function copyImageToClipboard(
	sourcePath: string,
	allowedRoot?: string | null
): Promise<void> {
	await invoke('copy_image_to_clipboard', {
		sourcePath,
		allowedRoot: allowedRoot ?? null
	});
}

export async function renameGalleryImage(
	sourcePath: string,
	allowedRoot: string | null,
	newName: string
): Promise<StoredImage> {
	const raw = await invoke<unknown>('rename_gallery_image', {
		sourcePath,
		allowedRoot,
		newName
	});
	return storedImageSchema.parse(raw);
}

/** Permanently delete a gallery image under the allowed root. */
export async function deleteGalleryImage(
	sourcePath: string,
	allowedRoot?: string | null
): Promise<void> {
	await invoke('delete_gallery_image', {
		sourcePath,
		allowedRoot: allowedRoot ?? null
	});
}

export async function loadImageOrder(dir: string, allowedRoot?: string | null): Promise<string[]> {
	if (!isTauri()) return [];
	const raw = await invoke<unknown>('load_image_order', {
		dir,
		allowedRoot: allowedRoot ?? null
	});
	return z.array(z.string()).parse(raw);
}

export async function saveImageOrder(
	dir: string,
	allowedRoot: string | null,
	paths: string[]
): Promise<void> {
	if (!isTauri()) return;
	await invoke('save_image_order', { dir, allowedRoot, paths });
}
