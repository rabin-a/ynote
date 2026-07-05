<h1 align="center">papery</h1>

<p align="center">
  A project-based markdown editor, previewer, and exporter.<br>
  One shared Rust renderer feeds a desktop app, a CLI, and an MCP server — so preview and export are always identical.
</p>

<p align="center">
  <a href="https://rabin-a.github.io/papery/">Website</a> ·
  <a href="https://github.com/rabin-a/papery/releases/latest">Download for macOS</a> ·
  <a href="#cli">CLI</a> ·
  <a href="#mcp-server">MCP</a>
</p>

---

- **One renderer.** Preview HTML and exported HTML come from the same Rust function. Markdown is never rendered in JavaScript.
- **Zero runtime deps.** PDF is produced in-process with embedded Typst — no Pandoc, LaTeX, or headless Chrome. Themes, template, and fonts are baked into the binary.
- **Project = folder.** A project is a directory of `.md` files plus an optional `papery.toml`. No database, no lock-in — git-friendly and agent-editable.

## Download (macOS)

Grab the latest `.dmg` from the [**Releases**](https://github.com/rabin-a/papery/releases/latest) page. It's a **universal** build (Apple Silicon + Intel).

> The app isn't code-signed yet, so on first launch macOS Gatekeeper will block it. Right-click **papery.app → Open** and confirm, or run `xattr -cr /Applications/papery.app` after moving it to Applications.

The desktop app opens straight into a local workspace (defaults to a cloud-synced folder — iCloud Drive `papery`, else `~/Documents/papery`) — no setup, no accounts. `Cmd+N` starts a new file, `⊞` a new group, double-click a filename to rename, and everything autosaves.

## CLI

The same engine is a single binary, `papery`, so scripts and agents get identical output.

```sh
# Build it (see "Build from source"), then:
papery list                                   # all documents (--json for machine output)
papery outline file.md                        # heading tree (--json)
papery render file.md -o out.html             # standalone HTML (stdout if no -o)
papery export file.md --format pdf -o out.pdf # pdf | docx | html
papery export --all --format pdf -o dist/     # batch export
papery watch file.md --format pdf -o out.pdf  # re-export on change
papery check                                  # lint broken links/images (exit 1 on findings)
```

All commands accept `--project <dir>` (defaults to the cwd, walking up to find `papery.toml`). Exit codes: `0` ok · `1` lint findings · `2` usage · `3` IO/render.

To put `papery` on your `PATH` after building:

```sh
cargo build --release -p papery-cli
cp target/release/papery /usr/local/bin/   # or ~/.local/bin, anywhere on PATH
```

## MCP server

`papery-mcp` exposes the same engine to AI agents over stdio. Register it with any MCP client (e.g. Claude Code):

```json
{ "mcpServers": { "papery": { "command": "papery-mcp", "args": ["--project", "."] } } }
```

Tools: `list_documents`, `read_document` (whole file or one section via `heading_slug`), `write_document`, `edit_section`, `get_outline`, `render_html`, `export`, `check_project`. Every path is confined to the project root.

## Build from source

Rust stable is all you need for the CLI and MCP server:

```sh
cargo build --release -p papery-cli    # the `papery` CLI
cargo build --release -p papery-mcp    # the MCP server
```

The desktop app uses [Tauri 2](https://tauri.app) and platform webview libraries:

```sh
# needs the Tauri CLI: npm i -g @tauri-apps/cli
cd crates/app && tauri build --target universal-apple-darwin   # -> .app + .dmg
```

## Configuration (`papery.toml`)

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
crates/core   papery-core — parse, render, export (HTML/PDF/DOCX)
crates/cli    papery       — command-line interface
crates/mcp    papery-mcp   — MCP stdio server (rmcp)
crates/app    papery-app   — Tauri 2 desktop app
ui/           desktop frontend (vanilla JS + bundled fonts)
assets/       themes (CSS) + the Typst PDF template
```

## License

MIT — see [LICENSE](LICENSE). Bundled fonts (IBM Plex Sans/Mono, Newsreader) are licensed under the SIL Open Font License.
