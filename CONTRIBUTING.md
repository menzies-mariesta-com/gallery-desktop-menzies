# Contributing

1. Use npm only. Node 22+.
2. Conventional Commits. Do not add Cursor co-author trailers.
3. Before claiming done: `npm run lint`, `npm run check`, `npm run lint` again, unit tests; in `src-tauri` run `cargo fmt --all -- --check` and `cargo check`.
4. Do not commit `.env`, private keys (including `*.key`), or `.cursor/`.
5. UI: Wash UI + Tailwind. Product icons: Lucide + mineral splash (see menzies-os rule 10).

## Updater signing

Gallery uses its **own** Tauri updater keypair, separate from calculator-desktop-menzies.

- Public key: embedded in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
- Private key: keep outside the repo (typical path `~/.tauri/gallery-desktop-menzies.key`). Never commit it.
- Local or CI signing env: `TAURI_SIGNING_PRIVATE_KEY` (path or contents) and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- GitHub Actions secret names match calculator (`TAURI_SIGNING_PRIVATE_KEY`), but this repo's secret value must be the **gallery** private key, not calculator's.

See `.env.example` and the Updates section in `README.md`.
