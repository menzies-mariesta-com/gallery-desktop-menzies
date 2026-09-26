/**
 * App semver from package.json. Synced by vite.config.ts on config load.
 * Do not import package.json from client code (Vite fs.allow 404).
 * Vite 8 does not text-replace bare `define` keys in client modules.
 */
export const APP_VERSION = '0.1.2';
