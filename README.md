<h1 align="center">YNote</h1>

<p align="center">
  A project-based markdown editor, previewer, and exporter.<br>
  One shared Rust renderer feeds a desktop app, a CLI, and an MCP server — so preview and export are always identical.
</p>

<p align="center">
  <a href="https://ynote.onl/">Website</a> ·
  <a href="https://github.com/rabin-a/ynote/releases/latest">Download</a> ·
  <a href="#cli">CLI</a> ·
  <a href="#mcp-server">MCP</a>
</p>

---

- **One renderer.** Preview HTML and exported HTML come from the same Rust function. Markdown is never rendered in JavaScript.
- **Zero runtime deps.** PDF is produced in-process with embedded Typst — no Pandoc, LaTeX, or headless Chrome. Themes, template, and fonts are baked into the binary.
- **Project = folder.** A project is a directory of `.md` files plus an optional `ynote.toml`. No database, no lock-in — git-friendly and agent-editable.

## Install

One line in the Terminal — detects your OS and installs the matching build:

```sh
curl -fsSL https://raw.githubusercontent.com/rabin-a/ynote/main/install.sh | bash
```

- **macOS** — universal `.dmg` (Apple Silicon + Intel) → `/Applications`, quarantine cleared, launched.
- **Linux** — `.AppImage` → `~/.local/bin/ynote` (needs FUSE: `sudo apt install libfuse2`).
- **Windows** — download the `.msi` from [**Releases**](https://github.com/rabin-a/ynote/releases/latest) and run it.

Install the **CLI** or **MCP server** instead of (or alongside) the app by passing a component:

```sh
curl -fsSL https://raw.githubusercontent.com/rabin-a/ynote/main/install.sh | bash -s -- cli   # the `ynote` CLI
curl -fsSL https://raw.githubusercontent.com/rabin-a/ynote/main/install.sh | bash -s -- mcp   # ynote-mcp (prints the MCP config)
curl -fsSL https://raw.githubusercontent.com/rabin-a/ynote/main/install.sh | bash -s -- all   # app + cli + mcp
```

Or grab any installer directly from the [Releases](https://github.com/rabin-a/ynote/releases/latest) page.

> The macOS and Windows builds aren't code-signed/notarized yet. The install script clears the macOS Gatekeeper quarantine for you; if you install the `.dmg` manually, right-click **YNote.app → Open** on first launch (or `xattr -cr /Applications/YNote.app`).

The desktop app opens straight into a local workspace (defaults to a cloud-synced folder — iCloud Drive `ynote`, else `~/Documents/ynote`) — no setup, no accounts. `Cmd+N` starts a new file, `⊞` a new group, double-click a filename to rename, and everything autosaves.

## CLI

The same engine is a single binary, `ynote`, so scripts and agents get identical output.

```sh
# Build it (see "Build from source"), then:
ynote list                                   # all documents (--json for machine output)
ynote outline file.md                        # heading tree (--json)
ynote render file.md -o out.html             # standalone HTML (stdout if no -o)
ynote export file.md --format pdf -o out.pdf # pdf | docx | html
ynote export --all --format pdf -o dist/     # batch export
ynote watch file.md --format pdf -o out.pdf  # re-export on change
ynote check                                  # lint broken links/images (exit 1 on findings)
```

All commands accept `--project <dir>` (defaults to the cwd, walking up to find `ynote.toml`). Exit codes: `0` ok · `1` lint findings · `2` usage · `3` IO/render.

To put `ynote` on your `PATH` after building:

```sh
cargo build --release -p ynote-cli
cp target/release/ynote /usr/local/bin/   # or ~/.local/bin, anywhere on PATH
```

## MCP server

`ynote-mcp` exposes the same engine to AI agents over stdio. Register it with any MCP client (e.g. Claude Code):

```json
{ "mcpServers": { "ynote": { "command": "ynote-mcp", "args": ["--project", "."] } } }
```

Tools: `list_documents`, `read_document` (whole file or one section via `heading_slug`), `write_document`, `edit_section`, `get_outline`, `render_html`, `export`, `check_project`. Every path is confined to the project root.

## Build from source

Rust stable is all you need for the CLI and MCP server:

```sh
cargo build --release -p ynote-cli    # the `ynote` CLI
cargo build --release -p ynote-mcp    # the MCP server
```

The desktop app uses [Tauri 2](https://tauri.app) and platform webview libraries:

```sh
# needs the Tauri CLI: npm i -g @tauri-apps/cli
cd crates/app && tauri build --target universal-apple-darwin   # -> .app + .dmg
```

## Configuration (`ynote.toml`)

```toml
[project]
name = "My Docs"
exclude = ["drafts/**"]

[render]
theme = "default"
syntax_theme = "github"
math = true

[export.pdf]
template = "default"
paper = "a4"
margin = "2.5cm"
font = "Inter"
toc = true

[export.docx]
template = "default"
```

Every key is optional; unknown keys warn rather than error.

## Workspace layout

```
crates/core   ynote-core — parse, render, export (HTML/PDF/DOCX)
crates/cli    ynote       — command-line interface
crates/mcp    ynote-mcp   — MCP stdio server (rmcp)
crates/app    ynote-app   — Tauri 2 desktop app
ui/           desktop frontend (vanilla JS, system font)
assets/       themes (CSS) + the Typst PDF template
```

## License

MIT — see [LICENSE](LICENSE).
