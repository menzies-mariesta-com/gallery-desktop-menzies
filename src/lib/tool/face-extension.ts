import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { z } from 'zod';
import { isTauri } from '$lib/tool/gallery-ipc';

export const faceExtensionStatusSchema = z.object({
	experimentalEnabled: z.boolean(),
	installed: z.boolean(),
	recognitionAvailable: z.boolean(),
	inference: z.string(),
	hostedArtifact: z.string(),
	installDir: z.string(),
	manifestVersion: z.string().nullable(),
	detectorSha256: z.string().nullable(),
	embedderSha256: z.string().nullable(),
	blocker: z.string()
});

export type FaceExtensionStatus = z.infer<typeof faceExtensionStatusSchema>;

export async function faceExtensionStatus(): Promise<FaceExtensionStatus | null> {
	if (!isTauri()) return null;
	return faceExtensionStatusSchema.parse(await invoke('face_extension_status'));
}

export async function setFaceRecognitionExperimental(
	enabled: boolean
): Promise<FaceExtensionStatus> {
	return faceExtensionStatusSchema.parse(
		await invoke('set_face_recognition_experimental', { enabled })
	);
}

export async function installFaceExtension(packageDir: string): Promise<FaceExtensionStatus> {
	return faceExtensionStatusSchema.parse(await invoke('install_face_extension', { packageDir }));
}

export async function uninstallFaceExtension(): Promise<FaceExtensionStatus> {
	return faceExtensionStatusSchema.parse(await invoke('uninstall_face_extension'));
}

export async function pickFacePackageDir(title: string): Promise<string | null> {
	const selected = await openDialog({ directory: true, multiple: false, title });
	return typeof selected === 'string' ? selected : null;
}
