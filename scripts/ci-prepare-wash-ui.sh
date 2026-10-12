#!/usr/bin/env bash
# Provide the file: Wash UI sibling for CI (and optional local use).
# package.json points at ../../design-lib-menzies/packages/menzies-design-wash-ui.
# On GitHub Actions that path is outside the app checkout and Wash dist is gitignored,
# so this clones design-lib-menzies and builds the library before npm ci.
set -euo pipefail

ROOT="$(pwd)"
DEST="$(node -e "console.log(require('path').resolve(process.cwd(), '..', '..', 'design-lib-menzies'))")"
PKG="$DEST/packages/menzies-design-wash-ui"
# Prefer a PAT that can read design-lib-menzies when that repo is private.
TOKEN="${DESIGN_LIB_CHECKOUT_TOKEN:-}"
if [[ -z "$TOKEN" ]]; then
	TOKEN="${GITHUB_TOKEN:-}"
fi
REPO_URL="${DESIGN_LIB_REPO_URL:-https://github.com/menzies-mariesta-com/design-lib-menzies.git}"

if [[ -n "$TOKEN" ]]; then
	# x-access-token works for classic and fine-grained PATs / Actions GITHUB_TOKEN.
	CLONE_URL="https://x-access-token:${TOKEN}@github.com/menzies-mariesta-com/design-lib-menzies.git"
else
	CLONE_URL="$REPO_URL"
fi

echo "Wash UI sibling target: $DEST"

if [[ ! -f "$PKG/package.json" ]]; then
	rm -rf "$DEST"
	git clone --depth 1 "$CLONE_URL" "$DEST"
fi

cd "$DEST"
if [[ -f package-lock.json ]]; then
	npm ci
else
	npm install
fi
npm run build:lib

if [[ ! -f "$PKG/dist/styles.css" ]]; then
	echo "Wash UI build did not produce dist/styles.css at $PKG/dist" >&2
	exit 1
fi

echo "Wash UI ready at $PKG"
cd "$ROOT"
