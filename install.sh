#!/usr/bin/env bash
#
# papery installer.
#
#   curl -fsSL https://raw.githubusercontent.com/rabin-a/papery/main/install.sh | bash
#
# Detects your OS and downloads the matching build from the latest GitHub
# Release, then installs it:
#   macOS  — universal .dmg  → /Applications (quarantine cleared, then launched)
#   Linux  — .AppImage       → ~/.local/bin/papery (made executable)
#   Windows— points you at the .msi installer (run install from PowerShell/Explorer)

set -euo pipefail

REPO="rabin-a/papery"
API="https://api.github.com/repos/${REPO}/releases/latest"

info() { printf '\033[1;36m▸\033[0m %s\n' "$1"; }
ok()   { printf '\033[1;32m✓ %s\033[0m\n' "$1"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$1" >&2; exit 1; }

# Return the first release asset download URL whose name matches the given
# extended-regex suffix (e.g. 'universal\.dmg', '\.AppImage', '\.msi').
asset_url() {
  curl -fsSL "${API}" | grep -oE "https://[^\"]+$1" | head -1
}

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
  Darwin) # ---------------------------------------------------------- macOS ---
    URL="$(asset_url 'universal\.dmg')"
    [ -n "${URL}" ] || die "No macOS .dmg found in the latest release of ${REPO}."
    TMP="$(mktemp -d)"; trap 'rm -rf "${TMP}"' EXIT
    info "Downloading $(basename "${URL}")…"
    curl -fSL --progress-bar "${URL}" -o "${TMP}/papery.dmg"
    info "Mounting…"
    MOUNT="$(hdiutil attach "${TMP}/papery.dmg" -nobrowse -readonly | grep -oE '/Volumes/[^[:cntrl:]]+' | tail -1)"
    [ -n "${MOUNT}" ] || die "Failed to mount the disk image."
    trap 'hdiutil detach "${MOUNT}" -quiet 2>/dev/null || true; rm -rf "${TMP}"' EXIT
    info "Installing to /Applications…"
    rm -rf "/Applications/papery.app"
    cp -R "${MOUNT}/papery.app" /Applications/
    hdiutil detach "${MOUNT}" -quiet 2>/dev/null || true
    info "Clearing quarantine (unsigned build)…"
    xattr -cr "/Applications/papery.app" 2>/dev/null || true
    ok "papery installed to /Applications."
    info "Launching…"; open "/Applications/papery.app"
    ;;

  Linux) # ---------------------------------------------------------- Linux ---
    if [ "${ARCH}" != "x86_64" ] && [ "${ARCH}" != "amd64" ]; then
      die "Only x86_64 Linux builds are published. On ${ARCH}, build from source: cargo build --release -p papery-cli"
    fi
    URL="$(asset_url '\.AppImage')"
    [ -n "${URL}" ] || die "No Linux .AppImage found in the latest release of ${REPO}."
    DEST="${HOME}/.local/bin"
    mkdir -p "${DEST}"
    BIN="${DEST}/papery"
    info "Downloading $(basename "${URL}")…"
    curl -fSL --progress-bar "${URL}" -o "${BIN}"
    chmod +x "${BIN}"
    ok "papery installed to ${BIN}."
    case ":${PATH}:" in
      *":${DEST}:"*) info "Run it with: papery" ;;
      *) info "Add ${DEST} to your PATH, then run: papery"
         info "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc" ;;
    esac
    info "(AppImages need FUSE; on Debian/Ubuntu: sudo apt install libfuse2)"
    ;;

  MINGW*|MSYS*|CYGWIN*|Windows_NT) # ------------------------------- Windows ---
    URL="$(asset_url '\.msi')"
    [ -z "${URL}" ] && URL="$(asset_url 'setup\.exe')"
    [ -n "${URL}" ] || die "No Windows installer found in the latest release of ${REPO}."
    info "Windows: download and run the installer:"
    printf '  %s\n' "${URL}"
    ;;

  *)
    die "Unsupported OS: ${OS}. Build the CLI from source: cargo build --release -p papery-cli"
    ;;
esac
