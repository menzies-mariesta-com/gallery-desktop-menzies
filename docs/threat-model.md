# Threat model

## Assets

- Local image files under Pictures (and legacy allowed roots), read-only for v0.1.
- Settings JSON (last folder, sort, type filter, onboarding).
- Updater signatures (gallery-only pubkey embedded; private key only in CI secrets / local `~/.tauri/`, never in git).

## Trust boundaries

- Webview is untrusted relative to Rust. No direct Node FS from the UI.
- Rust commands canonicalize directories and reject paths outside the allowed root (Pictures, or a legacy root).
- Asset protocol allows `$HOME/Pictures/**` and `$HOME/**` for `convertFileSrc` (document and revisit if tightening scopes).

## Capabilities

- Updater + process restart for downloads control.
- No folder picker dialog. No shell execution.

## Update channel

GitHub Releases `latest.json` for `gallery-desktop-menzies`. Trust requires the gallery minisign pubkey in `tauri.conf.json` (keypair is separate from calculator).
