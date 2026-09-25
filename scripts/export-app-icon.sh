#!/usr/bin/env bash
# Regenerate app icon rasters from static/app-icon.svg (Lucide Image + water-block circle).
# Canonical backdrop: music-player-mobile-menzies soft mist / water-block circles (paper #F7F4EF).
# Rule: menzies-os/.cursor/rules/10-app-product-icons.mdc
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SVG="$ROOT/static/app-icon.svg"
OUT_PNG="$ROOT/resources/icon.png"
ICONS_DIR="$ROOT/src-tauri/icons"

if [[ ! -f "$SVG" ]]; then
  echo "Missing $SVG" >&2
  exit 1
fi

mkdir -p "$ROOT/resources" "$ROOT/static" "$ROOT/src/lib/asset/image"

if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 1024 -h 1024 "$SVG" -o "$OUT_PNG"
elif command -v inkscape >/dev/null 2>&1; then
  inkscape "$SVG" -w 1024 -h 1024 -o "$OUT_PNG"
elif command -v convert >/dev/null 2>&1; then
  convert -background none -resize 1024x1024 "$SVG" "$OUT_PNG"
else
  echo "Need rsvg-convert, inkscape, or ImageMagick convert" >&2
  exit 1
fi

cp "$SVG" "$ROOT/static/favicon.svg"
cp "$SVG" "$ROOT/src/lib/asset/image/app-icon.svg"
cp "$SVG" "$ROOT/src/lib/asset/image/favicon.svg"
cp "$OUT_PNG" "$ROOT/static/icon-1024.png"

# Tauri platform set (32, 128, 128@2x, icns, ico, Square*, StoreLogo, icon.png)
if [[ -x "$ROOT/node_modules/.bin/tauri" ]] || command -v npx >/dev/null 2>&1; then
  (
    cd "$ROOT"
    npx tauri icon "$SVG" -o "$ICONS_DIR"
  )
else
  echo "Warn: tauri CLI missing; wrote $OUT_PNG only. Install @tauri-apps/cli and re-run." >&2
fi

echo "Wrote $OUT_PNG, static favicon copies, and $ICONS_DIR (when tauri icon ran)"
