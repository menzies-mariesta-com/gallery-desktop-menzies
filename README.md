# Gallery Desktop Menzies

Linux-first image gallery for the Menzies desktop suite. Opens your **Pictures** folder by default. Browse subfolders from the grid. Lightbox viewer for images.

## Identity

Product icon: **Lucide Image** + full music water-block (organic blotches + soft mist, paper `#F7F4EF`). Mineral ink `#1A3A48`, stroke ~2.6 at scale 17.2, no outline. Master: `static/app-icon.svg`.

## Run

```bash
npm install
npm run tauri dev
```

Dev server: `http://localhost:2002` (strict port).

## Stack

SvelteKit (static) + Tauri 2 + Wash UI + Paraglide + Zod. GPLv3.

## Updates (GitHub Releases)

Configured in `src-tauri/tauri.conf.json` under `plugins.updater`:

| Field       | Value                                                         |
| ----------- | ------------------------------------------------------------- |
| `endpoints` | GitHub Releases `latest.json` for this repo                   |
| `pubkey`    | Gallery-only minisign public key (not shared with calculator) |

Signing for release builds (shell or CI only; never commit):

```sh
export TAURI_SIGNING_PRIVATE_KEY="path-or-contents-of-gallery-private-key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""  # if set when generating
npm run tauri build
```

Generate keys once (gallery keypair only; keep private key outside git):

```sh
npm run tauri signer generate -- -w ~/.tauri/gallery-desktop-menzies.key -p "" --ci
```

Typical private key path: `~/.tauri/gallery-desktop-menzies.key`. Do not reuse the calculator private key. See `.env.example` for the same notes.

## Icons

```bash
npm run icon:export
```

Regenerates Tauri rasters and favicon copies from `static/app-icon.svg`.

## Build

```sh
npm run tauri build
```

## CI (manual)

GitHub Actions workflows use **`workflow_dispatch` only** (no auto-run on push).

1. Open **Actions**.
2. Select **CI** (lint, svelte-check, Vitest, rustfmt, clippy, cargo test, gitleaks CLI secret scan) or **Release** (Tauri build + updater artifacts).
3. Click **Run workflow**, choose the branch, then **Run workflow**.

| Workflow | File                                                             | Notes                                                                                                      |
| -------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| CI       | [`.github/workflows/ci.yml`](.github/workflows/ci.yml)           | Matrix: Linux (required), macOS/Windows (`continue-on-error`)                                              |
| Release  | [`.github/workflows/release.yml`](.github/workflows/release.yml) | Needs `TAURI_SIGNING_PRIVATE_KEY` (and optional password) repo secrets holding the **gallery** private key |

Release builds Linux deb/AppImage/rpm first; macOS and Windows are best-effort.
