import { Channel, invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { isTauri } from '$lib/tool/gallery-ipc';

export const extensionStateSchema = z.enum([
	'available',
	'installing',
	'installed',
	'updateAvailable',
	'updating',
	'error',
	'unavailable'
]);

export const extensionViewSchema = z.object({
	id: z.string(),
	appId: z.string(),
	name: z.string(),
	summary: z.string(),
	state: extensionStateSchema,
	updateMode: z.enum(['manual', 'automatic']),
	installedVersion: z.string().nullable(),
	registryVersion: z.string().nullable(),
	experimentalEnabled: z.boolean().nullable(),
	recognitionAvailable: z.boolean(),
	blocker: z.string().nullable(),
	canInstall: z.boolean(),
	canUpdate: z.boolean(),
	offlineFallback: z.boolean(),
	error: z.string().nullable()
});

export type ExtensionView = z.infer<typeof extensionViewSchema>;

export const extensionTransferEventSchema = z.object({
	phase: z.string(),
	done: z.number(),
	total: z.number()
});

export type ExtensionTransferEvent = z.infer<typeof extensionTransferEventSchema>;

const catalogSchema = z.array(extensionViewSchema);

export type ExtensionProgressListener = (event: ExtensionTransferEvent) => void;

function progressChannel(onProgress?: ExtensionProgressListener): Channel<ExtensionTransferEvent> {
	const channel = new Channel<ExtensionTransferEvent>();
	channel.onmessage = (message) => {
		const parsed = extensionTransferEventSchema.safeParse(message);
		if (!parsed.success || !onProgress) return;
		onProgress(parsed.data);
	};
	return channel;
}

async function invokeCatalog(
	command: string,
	args?: Record<string, unknown>
): Promise<ExtensionView[]> {
	return catalogSchema.parse(await invoke(command, args));
}

export async function extensionCatalog(): Promise<ExtensionView[] | null> {
	if (!isTauri()) return null;
	return invokeCatalog('extension_catalog');
}

export async function extensionCheck(force: boolean): Promise<ExtensionView[]> {
	return invokeCatalog('extension_check', { force });
}

export async function extensionInstall(
	id: string,
	onProgress?: ExtensionProgressListener
): Promise<ExtensionView[]> {
	return invokeCatalog('extension_install', {
		id,
		onProgress: progressChannel(onProgress)
	});
}

export async function extensionUpdate(
	id: string,
	onProgress?: ExtensionProgressListener
): Promise<ExtensionView[]> {
	return invokeCatalog('extension_update', {
		id,
		onProgress: progressChannel(onProgress)
	});
}

export async function extensionSetUpdateMode(
	id: string,
	mode: 'manual' | 'automatic'
): Promise<ExtensionView[]> {
	return invokeCatalog('extension_set_update_mode', { id, mode });
}

export async function extensionUninstall(
	id: string,
	onProgress?: ExtensionProgressListener
): Promise<ExtensionView[]> {
	return invokeCatalog('extension_uninstall', {
		id,
		onProgress: progressChannel(onProgress)
	});
}

export async function extensionCancelTransfer(): Promise<ExtensionView[]> {
	return invokeCatalog('extension_cancel_transfer');
}

export function transferPercent(event: ExtensionTransferEvent | null): number {
	if (!event || event.total <= 0) return 0;
	return Math.min(100, Math.round((event.done / event.total) * 100));
}
