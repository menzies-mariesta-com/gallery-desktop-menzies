import { paraglideVitePlugin } from '@inlang/paraglide-js';
import tailwindcss from '@tailwindcss/vite';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';

const port = Number(process.env.VITE_DEV_PORT ?? 2002);

const rootUrl = import.meta.url;
const pkg = JSON.parse(
	readFileSync(fileURLToPath(new URL('./package.json', rootUrl)), 'utf-8')
) as { version: string };

// Vite 8 client transforms skip bare `define` replacement (globals only; ES modules
// cannot see them). Write a tiny constant module instead of importing package.json.
// Emit Prettier-compliant output (singleQuote: true). JSON.stringify uses double quotes
// and would fail `prettier --check` after every Vite config load.
const appVersionPath = fileURLToPath(new URL('./src/lib/tool/app-version.ts', rootUrl));
const appVersionLiteral = `'${pkg.version.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
writeFileSync(
	appVersionPath,
	`/**
 * App semver from package.json. Synced by vite.config.ts on config load.
 * Do not import package.json from client code (Vite fs.allow 404).
 * Vite 8 does not text-replace bare \`define\` keys in client modules.
 */
export const APP_VERSION = ${appVersionLiteral};
`
);

export default defineConfig({
	server: {
		port,
		strictPort: true
	},
	preview: {
		port,
		strictPort: true
	},
	plugins: [
		paraglideVitePlugin({
			project: './project.inlang',
			outdir: './src/lib/paraglide',
			strategy: ['baseLocale']
		}),
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({
				fallback: 'index.html'
			})
		})
	],
	test: {
		expect: { requireAssertions: true },
		projects: [
			{
				extends: './vite.config.ts',
				test: {
					name: 'client',
					browser: {
						enabled: true,
						provider: playwright(),
						instances: [{ browser: 'chromium', headless: true }]
					},
					include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
					exclude: ['src/lib/server/**', 'src/lib/paraglide/**']
				}
			},
			{
				extends: './vite.config.ts',
				test: {
					name: 'server',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}', 'src/lib/paraglide/**']
				}
			}
		]
	}
});
