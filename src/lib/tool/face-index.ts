import { Channel, invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

export const faceIndexEventSchema = z.object({
	phase: z.string(),
	done: z.number(),
	total: z.number(),
	faces: z.number(),
	name: z.string()
});

export const faceIndexSummarySchema = z.object({
	cancelled: z.boolean(),
	scanned: z.number(),
	skipped: z.number(),
	faces: z.number(),
	truncated: z.boolean()
});

export const facePreviewSchema = z.object({
	path: z.string(),
	bbox: z.tuple([z.number(), z.number(), z.number(), z.number()]),
	imageWidth: z.number(),
	imageHeight: z.number()
});

export const facePersonSchema = z.object({
	id: z.number(),
	label: z.string().nullable(),
	createdMs: z.number(),
	updatedMs: z.number(),
	photoCount: z.number(),
	preview: facePreviewSchema.nullable()
});

export type FaceIndexEvent = z.infer<typeof faceIndexEventSchema>;
export type FaceIndexSummary = z.infer<typeof faceIndexSummarySchema>;
export type FacePreview = z.infer<typeof facePreviewSchema>;
export type FacePerson = z.infer<typeof facePersonSchema>;

type ProgressListener = (event: FaceIndexEvent) => void;

const progressListeners = new Set<ProgressListener>();
let indexing = false;

export function faceIndexRunning(): boolean {
	return indexing;
}

export function subscribeFaceProgress(listener: ProgressListener): () => void {
	progressListeners.add(listener);
	return () => {
		progressListeners.delete(listener);
	};
}

export async function startFaceIndex(
	folder: string,
	allowedRoot: string
): Promise<FaceIndexSummary> {
	if (indexing) {
		throw new Error('gallery:busy:a face index is already running');
	}
	indexing = true;
	const onProgress = new Channel<FaceIndexEvent>();
	onProgress.onmessage = (message) => {
		const parsed = faceIndexEventSchema.safeParse(message);
		if (!parsed.success) return;
		for (const listener of progressListeners) listener(parsed.data);
	};
	try {
		const raw: unknown = await invoke('face_index_start', {
			folder,
			allowedRoot,
			onProgress
		});
		return faceIndexSummarySchema.parse(raw);
	} finally {
		indexing = false;
	}
}

export async function cancelFaceIndex(): Promise<void> {
	await invoke('face_index_cancel');
}

export async function listFacePeople(): Promise<FacePerson[]> {
	return z.array(facePersonSchema).parse(await invoke('face_people'));
}

export async function listFacePersonImages(id: number): Promise<string[]> {
	return z.array(z.string()).parse(await invoke('face_person_images', { id }));
}

export async function renameFacePerson(id: number, label: string): Promise<void> {
	await invoke('face_person_rename', { id, label });
}
