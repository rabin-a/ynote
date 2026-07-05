#!/usr/bin/env bash
#
# papery installer for macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/rabin-a/papery/main/install.sh | bash
#
# Downloads the latest universal .dmg from GitHub Releases, copies papery.app to
# /Applications, clears the Gatekeeper quarantine flag (the app isn't notarized
# yet), and launches it — so you don't have to right-click → Open manually.

set -euo pipefail

REPO="rabin-a/papery"
APP="papery.app"
DEST="/Applications"

info() { printf '\033[1;36m▸\033[0m %s\n' "$1"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "papery's prebuilt app is macOS-only. On other platforms, build the CLI from source: cargo build --release -p papery-cli"

info "Finding the latest release…"
DMG_URL=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep -oE 'https://[^"]+universal\.dmg' | head -1)
[ -n "${DMG_URL}" ] || die "Couldn't find a universal .dmg in the latest release of ${REPO}."

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
DMG="${TMP}/papery.dmg"

info "Downloading $(basename "${DMG_URL}")…"
curl -fSL --progress-bar "${DMG_URL}" -o "${DMG}"

info "Mounting…"
MOUNT="$(hdiutil attach "${DMG}" -nobrowse -readonly | grep -oE '/Volumes/[^[:cntrl:]]+' | tail -1)"
[ -n "${MOUNT}" ] || die "Failed to mount the disk image."
cleanup_mount() { hdiutil detach "${MOUNT}" -quiet 2>/dev/null || true; }
trap 'cleanup_mount; rm -rf "${TMP}"' EXIT

info "Installing to ${DEST}…"
rm -rf "${DEST:?}/${APP}"
cp -R "${MOUNT}/${APP}" "${DEST}/"
cleanup_mount

info "Clearing quarantine (unsigned build)…"
xattr -cr "${DEST}/${APP}" 2>/dev/null || true

printf '\033[1;32m✓ papery installed to %s/%s\033[0m\n' "${DEST}" "${APP}"
info "Launching…"
open "${DEST}/${APP}"
