#!/usr/bin/env bash
# Build BOTH browser wasm bundles the ynote.onl web app serves, and place them
# under web/. Run this before deploying to Vercel — Vercel does no build; it
# just serves web/ (see vercel.json + .vercelignore at the repo root).
#
#   crates/wasm/build-web.sh
#
# Produces:
#   web/vendor/      light editor engine (HTML pipeline) — loaded on startup
#   web/vendor-pdf/  Typst PDF engine (size-optimized)   — lazy-loaded on export
set -uo pipefail
cd "$(dirname "$0")"

echo "==> light editor bundle → web/vendor/"
wasm-pack build --target web --out-dir pkg 2>&1 | tail -2
mkdir -p ../../web/vendor
cp pkg/ynote_wasm.js pkg/ynote_wasm_bg.wasm pkg/ynote_wasm.d.ts ../../web/vendor/

echo "==> PDF engine bundle → web/vendor-pdf/"
./build-pdf.sh

echo "==> done. web/ is ready to deploy."
du -h ../../web/vendor/ynote_wasm_bg.wasm ../../web/vendor-pdf/ynote_wasm_bg.wasm
