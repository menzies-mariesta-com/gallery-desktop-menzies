# Changelog

## Unreleased

## 0.1.3

- Image editor controls are icon-only with tooltips (rotate, crop, resize, filters, preview, save, close).
- Copy image puts pixels on the system clipboard for paste in other apps.
- Rename edits the file stem only. The extension stays locked; convert format with Edit and Save as new.
- Edit raster images in Rust: rotate, crop, resize, filters, JPEG quality, and format conversion. Preview stays in a capped cache. Saving asks Replace original or Save as new and does not overwrite silently.
- Manual image order is drag-and-drop plus Alt arrow keys.
- Extensions menu, after the title and version, installs the signed face-recognition package from menzies.mariesta.com only after Install. Enable recognition is a separate step and stays off until you choose it. After install, each extension can be Manual or Automatic. Manual is the default. Automatic never installs a new extension by itself.
- Face recognition 1.0.0 for Linux x86_64 groups faces on this computer with YuNet and SFace. Photos are not uploaded. The People view appears only after the package is installed and recognition is enabled.

## 0.1.2

- Titlebar version subscript no longer clips: keep in-flow with padding and overflow-visible.
- Product icon aligned to Music water-block recipe: organic pigment blotches + mist glow, mineral ink `#1A3A48`, thick Lucide Image stroke.

## 0.1.1

- Product icon: transitional water-block mist with black Lucide Image stroke (superseded; see 0.1.2 for full music recipe).

## 0.1.0

- Gallery desktop app: Pictures default root, non-recursive folder navigation, search/sort/type filter, lightbox, titlebar refresh, Lucide Image + water-block circle icon, Wash UI shell, Tauri updater placeholders.
