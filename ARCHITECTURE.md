# Architecture

## Shell

- Static SvelteKit SPA in Tauri webview (port **2002** in development).
- Custom titlebar, Wash UI vermilion default pigment, light/dark/system modes.
- Business logic for filesystem listing lives in Rust commands.
- Titlebar Refresh requests a listing reload via `gallery-refresh` pub/sub (no GalleryPad import).

## Paths

```text
{config_dir}/menzies/com.mariesta.menzies.gallery-desktop-menzies/   # settings.json
{data_local_dir}/menzies/com.mariesta.menzies.gallery-desktop-menzies/ # logs, caches
```

Default browse root: OS Pictures (`dirs::picture_dir()`). Navigation stays under Pictures (or a legacy allowed root). Listing is non-recursive: immediate subfolders and images only.

## IPC

| Command                                           | Purpose                                              |
| ------------------------------------------------- | ---------------------------------------------------- |
| `get_default_pictures_dir`                        | Resolve / create Pictures path                       |
| `list_gallery_entries`                            | Immediate children (folders then images), path-gated |
| `load_gallery_settings` / `save_gallery_settings` | Persist last folder, sort, type filter, onboarded    |
| `app_paths`                                       | Config and data directories                          |

Thumbnails and lightbox use `convertFileSrc` with Tauri asset protocol scopes under `$HOME/Pictures` and `$HOME/**`.

## Capabilities

Default-deny. Granted: window chrome, updater, process restart. Asset protocol scopes documented in `tauri.conf.json`.
