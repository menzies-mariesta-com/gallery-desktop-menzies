# Threat model

## Assets

- Local image files under Pictures (and legacy allowed roots). Listing is read-only. Edits, copies, and renames write only after an explicit action. Replace never runs for a format change, and save-as-new refuses an existing name.
- Settings JSON (last folder, sort, type filter, onboarding).
- `experimental.json` feature flag for face recognition (default off).
- Local extension files under the data dir (model bytes and licenses). No photo upload path exists.
- `extensions.json` (update mode, install consent, last accepted catalog sequence). `allowDowngrade` and `registryUrl` are developer overrides in that file. They are not shown in the app.
- The pinned Ed25519 publisher public key in `src-tauri/extensions/publisher-key.json`. The private key is not in this repo.
- Edit previews and manual order JSON under the data cache, both size-capped.
- Updater signatures (gallery-only pubkey embedded; private key only in CI secrets / local `~/.tauri/`, never in git).

## Trust boundaries

- Webview is untrusted relative to Rust. No direct Node FS from the UI.
- Rust commands canonicalize paths and reject paths outside the allowed root (Pictures, or a legacy root).
- Preview discard only deletes a file inside `cache/edit-preview`.
- Extension install only copies files whose manifest hashes match, and only into the app data dir. A local folder may omit a signature. If a signature is present, it must verify with the pinned publisher key. Registry downloads are separate: the catalog and each package must verify. The production host is `menzies.mariesta.com`. `registryUrl` may use `http://127.0.0.1:{port}` or `http://localhost:{port}` for a local test. No other host is accepted.
- The webview does not fetch extension bytes. Rust uses the Tauri HTTP plugin client, with a 30 second connect timeout and a 30 second read timeout, three attempts, and exponential backoff. Downloads land in the data-dir cache, are hash-checked, then replace the install. The previous install is kept once for recovery. Catalog sequence cannot move backward unless `allowDowngrade` is set by hand in `extensions.json`.
- Automatic updates run only after that extension was installed on purpose and its saved mode is `automatic`. A new extension is never installed by a check.
- Asset protocol allows `$HOME/Pictures/**` and `$HOME/**` for gallery originals via `convertFileSrc`. Edit previews are served only from the app data edit-preview cache: `$LOCALDATA/menzies/com.mariesta.menzies.gallery-desktop-menzies/cache/edit-preview/**`, plus an explicit `$HOME/.local/share/.../cache/edit-preview/**` entry because Unix `$HOME/**` does not match hidden `.local` segments.

## Capabilities

- Updater + process restart for downloads control.
- Folder picker (`dialog:default`) for the local face-recognition package.
- HTTP fetch in the webview capability is `https://menzies.mariesta.com/*`. Rust repeats the host allowlist and adds loopback only when `registryUrl` is set. Face embeddings stay in `faces.sqlite` under the data dir. Inference does not run unless the signed runtime is installed and `experimental.json` enables recognition.
- No shell execution. No photo upload. Model bytes are downloaded only after Install or an automatic update of an extension the user already installed.

## Update channel

GitHub Releases `latest.json` for `gallery-desktop-menzies`. Trust requires the gallery minisign pubkey in `tauri.conf.json` (keypair is separate from calculator).
