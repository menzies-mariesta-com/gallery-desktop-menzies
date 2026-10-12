import { describe, expect, it } from 'vitest';
import { extensionTransferEventSchema, extensionViewSchema, transferPercent } from './extensions';

describe('extension catalog schema', () => {
	it('accepts an unavailable face extension', () => {
		const parsed = extensionViewSchema.parse({
			id: 'face-recognition',
			appId: 'gallery-desktop-menzies',
			name: 'Face recognition',
			summary: 'Optional local add-on.',
			state: 'unavailable',
			updateMode: 'manual',
			installedVersion: null,
			registryVersion: null,
			experimentalEnabled: false,
			recognitionAvailable: false,
			blocker: 'artifact_not_published',
			canInstall: false,
			canUpdate: false,
			offlineFallback: true,
			error: null
		});
		expect(parsed.state).toBe('unavailable');
		expect(parsed.updateMode).toBe('manual');
		expect(parsed.canInstall).toBe(false);
	});

	it('parses transfer progress and percent', () => {
		const event = extensionTransferEventSchema.parse({
			phase: 'download',
			done: 25,
			total: 100
		});
		expect(transferPercent(event)).toBe(25);
		expect(transferPercent({ phase: 'verify', done: 0, total: 0 })).toBe(0);
	});
});
