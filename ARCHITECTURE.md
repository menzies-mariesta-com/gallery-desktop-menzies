# Architecture

## Shell

- Static SvelteKit SPA in Tauri webview (port **2002** in development).
- Wash UI comes from `file:../../design-lib-menzies/packages/menzies-design-wash-ui`. GitHub Actions prepares that sibling with `scripts/ci-prepare-wash-ui.sh` before `npm ci` (clone + `npm run build:lib`).
- Custom titlebar, Wash UI vermilion default pigment, light/dark/system modes.
- App shell uses `washShell` paper-grain plus animated watercolor splash atmosphere (`WashAtmosphere`, seed from bundle id).
- Business logic for filesystem listing, image edits, copies, renames, order, and extension install lives in Rust commands.
- Titlebar Refresh requests a listing reload via `gallery-refresh` pub/sub (no GalleryPad import).
- Titlebar **Extensions** sits after the title and version, before Refresh. It lists signed catalog entries and can install a local face-recognition folder.

## Paths

```text
{config_dir}/menzies/com.mariesta.menzies.gallery-desktop-menzies/
  settings.json
  experimental.json          # { "faceRecognition": false } by default
  extensions.json            # per-extension update mode, consent, catalog sequence
{data_local_dir}/menzies/com.mariesta.menzies.gallery-desktop-menzies/
  logs/
  cache/edit-preview/        # at most 8 preview files
  cache/order/               # at most 200 folder-order files, 2000 paths each
  cache/extension-downloads/ # temp parts, capped at 250 MB, removed after verify
  cache/extension-registry.json
  extensions/<id>/
  extensions/<id>.previous/  # one prior install, replaced on the next successful install
  faces.sqlite               # images, faces, and people for the installed model version
```

Default browse root: OS Pictures (`dirs::picture_dir()`). Navigation stays under Pictures (or a legacy allowed root). Listing is non-recursive: immediate subfolders and images only.

## IPC

| Command                                               | Purpose                                                                                                                         |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `get_default_pictures_dir`                            | Resolve / create Pictures path                                                                                                  |
| `list_gallery_entries`                                | Immediate children (folders then images), path-gated                                                                            |
| `load_gallery_settings` / `save_gallery_settings`     | Persist last folder, sort, type filter, onboarded                                                                               |
| `app_paths`                                           | Config and data directories                                                                                                     |
| `image_file_info`                                     | Width, height, bytes, raster flag                                                                                               |
| `preview_image_edit`                                  | Render edits into the bounded preview cache                                                                                     |
| `commit_image_edit`                                   | Write only with `replace` or `saveAsNew`                                                                                        |
| `suggest_edited_name`                                 | Sibling name for save as new                                                                                                    |
| `discard_edit_preview`                                | Delete one file inside the preview cache                                                                                        |
| `duplicate_gallery_image`                             | Byte copy to a free `name-copy` sibling                                                                                         |
| `rename_gallery_image`                                | Rename in place. Refuses an existing target. Same extension only                                                                |
| `load_image_order` / `save_image_order`               | Manual order for images in one folder                                                                                           |
| `face_extension_status`                               | Experimental flag, install state, blocker                                                                                       |
| `set_face_recognition_experimental`                   | Write `experimental.json`                                                                                                       |
| `install_face_extension` / `uninstall_face_extension` | Verify and copy, or remove, a local package                                                                                     |
| `extension_catalog`                                   | Signed catalog plus disk state, no network                                                                                      |
| `extension_check`                                     | Fetch the registry. Backs off 12 hours unless `force` is true                                                                   |
| `extension_install` / `extension_update`              | Download one published package after confirm; `onProgress` channel (`download` / `verify` / `extract` / `install` / `complete`) |
| `extension_set_update_mode`                           | `manual` or `automatic` after that extension is installed                                                                       |
| `extension_uninstall`                                 | Remove the install after confirm; `onProgress` (`remove` / `complete`)                                                          |
| `extension_cancel_transfer`                           | Stop the in-flight download between retries or chunks                                                                           |
| `face_index_start` / `face_index_cancel`              | Index the current folder. Cancel on request or window close                                                                     |
| `face_people` / `face_person_rename`                  | List grouped people and set or clear a name                                                                                     |

Edit payloads use camelCase Zod and serde. Errors are `gallery:{code}:{detail}`.

Pipeline order: rotate, crop, resize, enhance, filters, encode. Raster inputs are JPEG, PNG, WebP, GIF (first frame), and BMP. SVG is listed but not edited. JPEG quality is real. PNG, GIF, and BMP ignore quality. WebP output uses the lossless encoder in `image` 0.25, so a quality number is not applied (`webp_lossless_only`).

**Enhance** is a Lanczos upscale (auto: 1.25 when the long side is under 2000, capped at 4096), an unsharp mask, and a contrast adjustment. It is not a generative model.

**Replace** writes the same path only when the output format kind matches the source (`jpg` and `jpeg` match). Format conversion requires **Save as new**. Save as new refuses when the destination exists. Both choices are explicit in the editor. Preview never writes the original.

Files over 80 MB, or images over 16384 px or 40 megapixels, are rejected.

## Extension registry

Gallery reads a signed catalog, schema `menzies-extension-registry/1`:

`https://menzies.mariesta.com/extensions/registry.json`

A developer may set `registryUrl` in `extensions.json` to `http://127.0.0.1:{port}/extensions/registry.json` or the same path on `localhost`. The port is required. That override is not shown in the app. `https://store.menzies.design` is not an extension host.

The publisher public key is pinned in `src-tauri/extensions/publisher-key.json` (key id `menzies-extensions-ed25519-1`). Gallery verifies the catalog with Ed25519 over `menzies-extension-registry/1\n` plus canonical JSON (sorted keys, no whitespace). Package bytes, when one is published, are signed over `menzies-extension-artifact/1\n` plus the file. A catalog sequence lower than the last accepted sequence is rejected. Same sequence with a different signature is rejected. `allowDowngrade: true` in `extensions.json` is the only override, and the app has no control for it.

Each entry has `manual` or `automatic` updates. New installs default to manual. Automatic runs on Check for updates, on opening Extensions after the 12 hour backoff, and on titlebar Refresh. It never installs an extension that has not been installed once.

The catalog is cached for at most 60 seconds on the website so a revocation can publish quickly. Package URLs are immutable and versioned under `/extensions/packages/{appId}/{id}/{version}/{os}-{arch}.zip`.

Checks are not on a timer. Opening the menu uses the backoff. Refresh and Check for updates pass `force`. Listeners are removed when the titlebar is destroyed.

## Face recognition extension

This is a local face detector plus an embedding model, then clustering and identity labels. It is not a general language model.

The experimental flag defaults to off (`experimental.json`). Install does not require that flag. After install, Enable recognition writes the flag. Inference, indexing, and the People view run only when the signed package is installed and the flag is on.

The published package is version 1.0.0 for Linux x86_64:

`https://menzies.mariesta.com/extensions/packages/gallery-desktop-menzies/face-recognition/1.0.0/linux-x86_64.zip`

| Piece    | File                                        | Bytes    | License                      |
| -------- | ------------------------------------------- | -------- | ---------------------------- |
| Detector | YuNet `face_detection_yunet_2023mar.onnx`   | 232589   | MIT, Copyright 2020 Shiqi Yu |
| Embedder | SFace `face_recognition_sface_2021dec.onnx` | 38696353 | Apache-2.0                   |
| Runtime  | ONNX Runtime 1.28 `libonnxruntime.so`       | 24268848 | MIT, Microsoft Corporation   |
| Zip      | `linux-x86_64.zip`                          | 45009296 | Signed catalog artifact      |

Gallery links `ort` 2.0.0-rc.13 with `load-dynamic` (Cargo feature `face-runtime`, on by default). The shared library stays in the signed zip and is loaded only after install. `init_from` disables telemetry. Replacing that library needs a Gallery restart because ONNX Runtime initializes once per process. OpenMP inside the Microsoft build may ignore the intra-thread cap. The app still asks for 2 intra-op threads and indexes one image at a time. macOS and Windows package slots are not published.

Both model licenses allow redistribution, including commercial use, when the notices stay with the files. Notices are `licenses/yunet-MIT.txt`, `licenses/sface-Apache-2.0.txt`, and `licenses/onnxruntime-MIT.txt`.

Package format `menzies-gallery-face-extension/1`:

```json
{
	"format": "menzies-gallery-face-extension/1",
	"id": "face-recognition",
	"version": "0.1.0",
	"models": [
		{
			"role": "detector",
			"file": "detector.onnx",
			"sha256": "<64 hex chars>",
			"license": "license.txt"
		},
		{
			"role": "embedder",
			"file": "embedder.onnx",
			"sha256": "<64 hex chars>"
		}
	]
}
```

Install canonicalizes each file, rejects `..`, checks SHA-256 before copy, and stages into the data dir. Limits: 180 MB per model or runtime file, 200 MB total, 32 zip entries. A local folder may omit a signature. A signature that is present must verify. The registry zip signature is the trust boundary for downloads.

YuNet's published graph takes a fixed 640 by 640 input, so Gallery stretches the working image to that size and scales the boxes back. SFace alignment uses the OpenCV five-point template and an Umeyama similarity. Embeddings are L2-normalized. Grouping uses cosine similarity 0.55 and refuses a match when two people are within 0.05. Unknown clusters stay unlabeled until you name them.

`faces.sqlite` (sqlx migration `001_faces.sql`) stores image path, fingerprint, model version, box, landmarks, embedding, and person assignment. The index command walks only the current allowed folder, depth 4, at most 2000 raster images, 40 MB each. It skips unchanged files by size, mtime, and fingerprint. One image runs at a time. Cancel is an atomic flag, also set when the window closes. Progress uses a Tauri channel. There is no timer and no photo upload.

## Dependency order for the rest

1. Image edit, copy, rename, and manual order (this tree).
2. Signed face package, dynamic ONNX Runtime, and the People index (this tree).

Thumbnails and lightbox use `convertFileSrc` with Tauri asset protocol scopes under `$HOME/Pictures` and `$HOME/**`. Edit live previews are loaded from the data-dir cache only: `$LOCALDATA/menzies/com.mariesta.menzies.gallery-desktop-menzies/cache/edit-preview/**` (plus an explicit `$HOME/.local/share/.../edit-preview/**` scope on Linux, because `$HOME/**` does not match hidden `.local` segments).

## Capabilities

Default-deny. Granted: window chrome, updater, process restart, and HTTP fetch for `https://menzies.mariesta.com`. Rust also accepts a loopback `registryUrl` for a local catalog test. Asset protocol scopes documented in `tauri.conf.json`.
