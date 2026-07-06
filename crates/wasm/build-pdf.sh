#!/usr/bin/env bash
# Build the size-optimized browser PDF engine bundle → web/vendor-pdf/.
#
# Two-step because wasm-pack applies its `wasm-opt` metadata only for the
# built-in profiles (dev/release/profiling), NOT custom ones — and its default
# `-O` rejects Typst's post-MVP opcodes. So we build with the size-first
# `release-wasm` cargo profile (opt-level="z", fat LTO, panic=abort), let
# wasm-pack's own wasm-opt step fail harmlessly, then run wasm-opt ourselves
# with the correct feature flags.
#
# The light editor bundle (web/vendor/) is built separately with the normal
# `wasm-pack build --target web` (no pdf feature) and is unaffected by this.
set -uo pipefail
cd "$(dirname "$0")"

echo "==> cargo build (release-wasm, --features pdf)"
wasm-pack build --target web --out-dir pkg-pdf --profile release-wasm -- --features pdf || true

WASM=pkg-pdf/ynote_wasm_bg.wasm
[ -f "$WASM" ] || { echo "error: $WASM not produced"; exit 1; }

# Locate the wasm-opt binary wasm-pack downloaded (macOS + Linux cache paths).
WO=$(ls -t "$HOME"/Library/Caches/.wasm-pack/wasm-opt-*/bin/wasm-opt \
        "$HOME"/.cache/.wasm-pack/wasm-opt-*/bin/wasm-opt 2>/dev/null | head -1)
[ -x "$WO" ] || { echo "error: wasm-opt not found (install binaryen)"; exit 1; }

echo "==> wasm-opt -Oz (with post-MVP features enabled)"
"$WO" "$WASM" -o "$WASM.opt" \
  -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
  --enable-mutable-globals --enable-multivalue --enable-reference-types
mv "$WASM.opt" "$WASM"

echo "==> deploy → web/vendor-pdf/"
mkdir -p ../../web/vendor-pdf
cp pkg-pdf/ynote_wasm.js pkg-pdf/ynote_wasm_bg.wasm pkg-pdf/ynote_wasm.d.ts ../../web/vendor-pdf/

RAW=$(wc -c < "$WASM")
printf "done — %.1f MB raw / %.1f MB gzip\n" \
  "$(echo "$RAW/1048576"|bc -l)" "$(gzip -c "$WASM" | wc -c | awk '{print $1/1048576}')"
