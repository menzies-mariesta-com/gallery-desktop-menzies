import { z } from 'zod';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export const updateOutcomeSchema = z.enum([
	'up_to_date',
	'installed',
	'declined',
	'unavailable',
	'error'
]);
export type UpdateOutcome = z.infer<typeof updateOutcomeSchema>;

export interface UpdateFlowResult {
	outcome: UpdateOutcome;
	version?: string;
	detail?: string;
}

function isTauriRuntime(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Check GitHub Releases via tauri-plugin-updater, then download/install if the user confirms.
 * Requires configured pubkey and OWNER/REPO endpoint in tauri.conf.json.
 */
export async function runUpdateCheck(
	confirmInstall: (version: string) => boolean
): Promise<UpdateFlowResult> {
	if (!isTauriRuntime()) {
		return {
			outcome: 'unavailable',
			detail: 'Updates are available only in the desktop application.'
		};
	}

	try {
		const update = await check();
		if (!update) {
			return { outcome: 'up_to_date' };
		}

		const version = update.version;
		if (!confirmInstall(version)) {
			return { outcome: 'declined', version };
		}

		await update.downloadAndInstall();
		await relaunch();
		return { outcome: 'installed', version };
	} catch (err) {
		const detail = err instanceof Error ? err.message : String(err);
		return { outcome: 'error', detail };
	}
}
