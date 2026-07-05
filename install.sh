#!/usr/bin/env bash
#
# papery installer.
#
#   # desktop app (default):
#   curl -fsSL https://raw.githubusercontent.com/rabin-a/papery/main/install.sh | bash
#
#   # pick what to install: app | cli | mcp | all
#   curl -fsSL https://raw.githubusercontent.com/rabin-a/papery/main/install.sh | bash -s -- cli
#   curl -fsSL https://raw.githubusercontent.com/rabin-a/papery/main/install.sh | bash -s -- all
#
#   app  desktop application  (macOS .dmg / Linux .AppImage / Windows .msi)
#   cli  the `papery` command-line tool
#   mcp  the `papery-mcp` server (prints the MCP registration snippet)
#   all  app + cli + mcp

set -euo pipefail

REPO="rabin-a/papery"
API="https://api.github.com/repos/${REPO}/releases/latest"
COMPONENT="${1:-app}"

info() { printf '\033[1;36m▸\033[0m %s\n' "$1"; }
ok()   { printf '\033[1;32m✓ %s\033[0m\n' "$1"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$1" >&2; exit 1; }

# First release asset URL whose name matches the given extended-regex suffix.
asset_url() { curl -fsSL "${API}" | grep -oE "https://[^\"]+$1" | head -1; }

OS="$(uname -s)"
ARCH="$(uname -m)"

# --------------------------------------------------------------- desktop app ---
install_app() {
  case "${OS}" in
    Darwin)
      local url; url="$(asset_url 'universal\.dmg')"
      [ -n "${url}" ] || die "No macOS .dmg in the latest release."
      local tmp; tmp="$(mktemp -d)"
      info "Downloading $(basename "${url}")…"
      curl -fSL --progress-bar "${url}" -o "${tmp}/papery.dmg"
      info "Mounting…"
      local mount; mount="$(hdiutil attach "${tmp}/papery.dmg" -nobrowse -readonly | grep -oE '/Volumes/[^[:cntrl:]]+' | tail -1)"
      [ -n "${mount}" ] || { rm -rf "${tmp}"; die "Failed to mount the disk image."; }
      info "Installing to /Applications…"
      rm -rf "/Applications/papery.app"
      cp -R "${mount}/papery.app" /Applications/
      hdiutil detach "${mount}" -quiet 2>/dev/null || true
      rm -rf "${tmp}"
      info "Clearing quarantine (unsigned build)…"
      xattr -cr "/Applications/papery.app" 2>/dev/null || true
      ok "papery.app installed to /Applications."
      info "Launching…"; open "/Applications/papery.app"
      ;;
    Linux)
      [ "${ARCH}" = "x86_64" ] || [ "${ARCH}" = "amd64" ] || die "Prebuilt app is x86_64 only. Build from source on ${ARCH}."
      local url; url="$(asset_url '\.AppImage')"
      [ -n "${url}" ] || die "No Linux .AppImage in the latest release."
      local dest="${HOME}/.local/bin"; mkdir -p "${dest}"
      info "Downloading $(basename "${url}")…"
      curl -fSL --progress-bar "${url}" -o "${dest}/papery-app"
      chmod +x "${dest}/papery-app"
      ok "papery installed to ${dest}/papery-app (AppImages need FUSE: sudo apt install libfuse2)."
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      local url; url="$(asset_url '\.msi')"; [ -z "${url}" ] && url="$(asset_url 'setup\.exe')"
      [ -n "${url}" ] || die "No Windows installer in the latest release."
      info "Windows: download and run the installer:"; printf '  %s\n' "${url}"
      ;;
    *) die "Unsupported OS for the desktop app: ${OS}." ;;
  esac
}

# --------------------------------------------------- CLI / MCP command-line ---
# Downloads the tools archive (papery + papery-mcp) and installs the requested
# binaries. $1 = cli | mcp | both
install_tools() {
  local which="$1" suffix ext
  case "${OS}" in
    Darwin) suffix="macos-universal"; ext="tar.gz" ;;
    Linux)
      [ "${ARCH}" = "x86_64" ] || [ "${ARCH}" = "amd64" ] || die "CLI/MCP prebuilt for x86_64 only. Build from source: cargo build --release -p papery-cli -p papery-mcp"
      suffix="linux-x86_64"; ext="tar.gz" ;;
    *) die "CLI/MCP auto-install supports macOS and Linux. On Windows, download papery-tools-windows-x86_64.zip from the Releases page." ;;
  esac
  local url; url="$(asset_url "papery-tools-${suffix}\\.${ext}")"
  [ -n "${url}" ] || die "No papery-tools-${suffix} archive in the latest release."
  local tmp; tmp="$(mktemp -d)"
  info "Downloading $(basename "${url}")…"
  curl -fSL --progress-bar "${url}" -o "${tmp}/tools.tgz"
  tar -xzf "${tmp}/tools.tgz" -C "${tmp}"

  local bindir="/usr/local/bin"
  [ -w "${bindir}" ] || bindir="${HOME}/.local/bin"
  mkdir -p "${bindir}"
  _put() { chmod +x "${tmp}/$1"; mv -f "${tmp}/$1" "${bindir}/$1"; ok "installed $1 → ${bindir}/$1"; }

  case "${which}" in
    cli)  _put papery ;;
    mcp)  _put papery-mcp ;;
    both) _put papery; _put papery-mcp ;;
  esac
  rm -rf "${tmp}"

  case ":${PATH}:" in *":${bindir}:"*) : ;; *) info "Add ${bindir} to your PATH (e.g. echo 'export PATH=\"${bindir}:\$PATH\"' >> ~/.bashrc)";; esac
  if [ "${which}" = "mcp" ] || [ "${which}" = "both" ]; then
    printf '\n\033[1;36mRegister the MCP server with your client (e.g. Claude Code):\033[0m\n'
    printf '  { "mcpServers": { "papery": { "command": "%s/papery-mcp", "args": ["--project", "."] } } }\n\n' "${bindir}"
  fi
}

case "${COMPONENT}" in
  app) install_app ;;
  cli) install_tools cli ;;
  mcp) install_tools mcp ;;
  all) install_app; install_tools both ;;
  *)   die "Unknown component '${COMPONENT}'. Use one of: app | cli | mcp | all" ;;
esac
