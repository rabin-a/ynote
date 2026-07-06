Also use ANGENTS.md
# CLAUDE.md — ynote

Project-based markdown editor, previewer, and exporter. Single self-contained Rust binary. Zero runtime dependencies. Fast to load, minimal UI, first-class agent interface (CLI + MCP).

Working name: **ynote** (rename freely; keep crate paths consistent).

---

## 1. Product Summary

ynote lets a user (or an AI agent) open a folder of markdown files as a **project**, edit them with live preview, and export to **HTML, PDF, and DOCX** — all rendered by one shared pipeline so preview output and export output are identical.

Three consumers of the same core:

1. **Human** — Tauri 2 desktop app: file tree, editor, live preview, export.
2. **Agent (structured)** — MCP stdio server with a small, typed tool surface.
3. **Agent/scripts (shell)** — CLI with the same operations.

## 2. Non-Negotiable Principles

- **One renderer.** Preview HTML and exported HTML come from the same Rust function. Never render markdown in JavaScript.
- **Zero runtime deps.** No Pandoc, no LaTeX, no headless Chrome. PDF via embedded Typst. Fonts bundled or system-discovered.
- **Core owns all logic.** `cli`, `mcp`, and `app` are thin adapters over `core`. If a feature requires logic in an adapter, it belongs in core.
- **Project = folder.** No database, no hidden state. A project is a directory containing `.md` files, assets, and an optional `ynote.toml`. Everything is plain files, git-friendly, agent-editable.
- **Fast.** Cold start of the desktop app < 1s. Preview re-render after keystroke < 50ms for typical documents (debounce 100ms). Export of a 100-page document < 5s.
- **Minimal UI.** File tree + editor + preview + export dialog. No settings screens: all configuration lives in `ynote.toml`.

## 3. Repository Layout

```
ynote/
├── Cargo.toml                 # workspace
├── crates/
│   ├── core/                  # ynote-core: all logic
│   │   └── src/
│   │       ├── project.rs     # workspace model, config, asset resolution
│   │       ├── parse.rs       # markdown → AST (comrak)
│   │       ├── render_html.rs # AST → HTML string (+ theme CSS)
│   │       ├── export_pdf.rs  # AST → Typst markup → PDF bytes
│   │       ├── export_docx.rs # AST → .docx bytes
│   │       ├── outline.rs     # AST → heading tree
│   │       └── error.rs       # single error enum (thiserror)
│   ├── cli/                   # ynote binary (clap)
│   ├── mcp/                   # ynote-mcp binary (rmcp, stdio transport)
│   └── app/                   # Tauri 2 shell (src-tauri)
├── ui/                        # frontend: vanilla TS or Svelte + CodeMirror 6
├── assets/
│   ├── fonts/                 # bundled fonts for Typst PDF output
│   ├── themes/                # HTML preview/export CSS themes
│   └── typst/                 # default.typ export template
└── tests/                     # integration + golden-file tests
```

## 4. Core Requirements (crates/core)

### 4.1 Project model (`project.rs`)
- `Project::open(root: &Path)` — validates directory, loads `ynote.toml` if present, applies defaults otherwise.
- Enumerate documents: all `*.md` under root, respecting `.gitignore` and an optional `exclude` list in config. Return relative paths, sorted.
- Resolve relative asset paths (images, includes) against the document's own directory.
- Path safety: **reject any path that escapes the project root** (canonicalize + prefix check). This is a hard security requirement — the MCP server exposes these operations to agents.

### 4.2 Config (`ynote.toml`)
```toml
[project]
name = "My Docs"
exclude = ["drafts/**"]

[render]
theme = "default"          # maps to assets/themes/<name>.css
syntax_theme = "github"    # code-block highlighting theme
math = true                # KaTeX-compatible math rendering

[export.pdf]
template = "default"       # maps to a Typst template
paper = "a4"
margin = "2.5cm"
font = "Inter"
toc = true

[export.docx]
template = "default"
```
All fields optional; sensible defaults for everything. Unknown keys → warning, not error.

### 4.3 Parsing (`parse.rs`)
- Use **comrak** with GFM extensions enabled: tables, strikethrough, autolinks, task lists, footnotes.
- Front matter: parse YAML front matter into a `serde_yaml::Value`; expose it, strip it from rendered output. Front matter keys `title`, `author`, `date` feed export templates.
- Expose the comrak AST (or a thin wrapper) — exporters walk the AST, never re-parse strings.

### 4.4 HTML rendering (`render_html.rs`)
- `render_html(doc, opts) -> String` — full standalone HTML (embedded CSS) or body-only fragment (for the preview webview), selected by option.
- Syntax highlighting: **syntect**, server-side, at render time (no JS highlighter).
- Math: render `$...$` / `$$...$$` via KaTeX-compatible markup when `render.math = true` (client-side KaTeX in preview is acceptable ONLY if exported HTML embeds KaTeX assets so output is self-contained).
- Heading anchors: stable slug IDs (GitHub algorithm) — used by outline and intra-doc links.
- Local images: preview uses Tauri asset protocol; HTML export inlines images as base64 by default (`--no-inline` flag to keep relative paths).

### 4.5 PDF export (`export_pdf.rs`) — highest-risk component, build early
- Pipeline: comrak AST → **Typst markup** → compile in-process with `typst` + `typst-pdf` crates → PDF bytes.
- The AST→Typst lowering must cover: headings (→ Typst headings, feeding native TOC), paragraphs, emphasis/strong/strikethrough, inline code, fenced code blocks **with syntax highlighting**, block quotes, ordered/unordered/task lists (nested), tables (GFM alignment), images (resolve relative paths, embed), links, footnotes, horizontal rules, math (pass through to Typst math or typeset raw).
- Escape Typst-significant characters in text nodes correctly (`#`, `*`, `_`, `@`, `$`, `[`, `]`, `\`, `<`, `>`). Write property-style tests for this.
- Template: `assets/typst/default.typ` defines page setup, fonts, heading styles, header/footer, optional TOC. Document content is injected as a variable. Config keys (paper, margin, font, toc) are passed as template inputs.
- Fonts: bundle a default set (e.g. Inter + JetBrains Mono + a serif) in `assets/fonts`, embed via Typst's font loading; also search system fonts. Missing font → fall back with a warning, never fail the export.

### 4.6 DOCX export (`export_docx.rs`)
- AST → `docx-rs`. Map to Word built-in styles (Heading 1–6, Quote, code as monospaced style, real Word tables and lists) so downstream editing in Word/Google Docs behaves correctly.
- Acceptable v1 gaps: math as plain text, footnotes as endnote-style text. Document gaps in code comments.

### 4.7 Outline (`outline.rs`)
- `outline(doc) -> Vec<Heading { level, text, slug, line }>` — used by the UI sidebar and the MCP `get_outline` tool (agents need this to navigate long documents without reading them fully).

### 4.8 Errors
- Single `thiserror` enum in core. Adapters map it: CLI → exit codes + stderr message; MCP → tool error result with human-readable message; Tauri → serialized error to frontend. Never `panic!` on user input.

## 5. CLI Requirements (crates/cli)

Binary name: `ynote`. Built with clap (derive). All commands accept `--project <dir>` (default: cwd, walking up to find `ynote.toml`).

```
ynote list                              # relative paths of all project documents
ynote outline <file.md>                 # heading tree (text or --json)
ynote render <file.md> [-o out.html]    # standalone HTML (stdout if no -o)
ynote export <file.md> --format pdf|docx|html [-o out]
ynote export --all --format pdf -o dist/    # batch export
ynote watch <file.md>                   # re-export on change (notify crate)
ynote check                             # lint: broken relative links/images, exit 1 on findings
```

- `--json` flag on `list`, `outline`, `check` for machine-readable output.
- Exit codes: 0 ok, 1 lint findings, 2 usage error, 3 IO/render error.

## 6. MCP Server Requirements (crates/mcp)

Binary: `ynote-mcp`, stdio transport, built on **rmcp** (official Rust MCP SDK). Started with `--project <dir>`. Registration in Claude Code:

```json
{ "mcpServers": { "ynote": { "command": "ynote-mcp", "args": ["--project", "."] } } }
```

Tools (keep this surface small and stable):

| Tool | Input | Output |
|---|---|---|
| `list_documents` | — | relative paths + titles (front matter or first H1) |
| `read_document` | `path`, optional `heading_slug` | full text, or just that section |
| `write_document` | `path`, `content` | ok; creates parent dirs; refuses paths outside root |
| `edit_section` | `path`, `heading_slug`, `content` | replaces the section under that heading — preferred over full-file writes for long docs |
| `get_outline` | `path` | heading tree with slugs and line numbers |
| `render_html` | `path` | standalone HTML string |
| `export` | `path`, `format` (`pdf`/`docx`/`html`), optional `out` | absolute output path |
| `check_project` | — | lint findings (broken links/images) |

- Every tool description must be written for an LLM consumer: state what it does, when to prefer it (e.g. "use `edit_section` instead of `write_document` for targeted changes"), and input constraints.
- All file operations go through core's path-safety check. No tool may read or write outside the project root.

## 7. Desktop App Requirements (crates/app + ui/)

- **Tauri 2.** Frontend: vanilla TypeScript or Svelte (no React; keep the bundle tiny). Editor: **CodeMirror 6** with markdown mode.
- Layout: left file tree (collapsible, Cmd+B) · center editor · right preview. Preview toggle: side-by-side / editor-only / preview-only (Cmd+\).
- Live preview: on change, debounce 100ms, call Tauri command `render_preview(path, content)` → core `render_html` (fragment mode) → set innerHTML. Preserve scroll position; sync scroll editor↔preview by nearest heading (best-effort, don't over-engineer).
- File operations: open project (folder picker), create/rename/delete file via tree context menu, autosave on idle (1s) + Cmd+S.
- Export: Cmd+E opens a dialog — format (PDF/DOCX/HTML), scope (current file / whole project), output location. Progress + "Reveal in folder" on completion.
- Outline sidebar (from core `outline`), click-to-jump.
- Unsaved-changes indicator per tab/file; confirm on close with dirty buffers.
- No settings UI. A "Edit project config" menu item just opens `ynote.toml` in the editor.
- Dark/light theme follows OS; preview theme comes from project config.

## 8. Quality & Testing

- **Golden-file tests**: `tests/fixtures/*.md` → committed expected HTML and Typst output. Cover every markdown construct listed in 4.5.
- PDF: assert compilation succeeds + basic invariants (page count > 0, text extractable) rather than byte-exact output.
- Property tests for Typst text escaping (any Unicode string must round-trip without breaking compilation).
- Path-traversal tests for project root enforcement (`../`, absolute paths, symlinks).
- CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, plus a smoke export of the fixtures on Linux/macOS/Windows.
- Benchmark guard: parsing + HTML render of a 1MB fixture must stay under 50ms (criterion, informational).

## 9. Build Order (do in this sequence)

1. **Workspace + core parse/render_html** with golden tests. CLI `list`, `outline`, `render`. → usable + agent-usable immediately.
2. **PDF export** (Typst lowering, template, fonts). Highest risk; validate early with real documents containing tables, code, images.
3. **Tauri app**: tree + editor + live preview + export dialog.
4. **MCP server** (thin — core already does everything).
5. **DOCX export**, `check` lint, `watch`, batch export, polish.

## 10. Conventions for Agents Working in This Repo

- Rust 2021+, `cargo fmt` defaults, clippy clean (`-D warnings`).
- Commits: conventional commits (`feat(core): ...`, `fix(pdf): ...`).
- Any new markdown feature must land in this order: parse (if needed) → HTML render + golden test → PDF lowering → DOCX lowering → UI (if applicable). Never ship a feature that previews but doesn't export.
- Do not add dependencies to `app`/`cli`/`mcp` that duplicate core functionality.
- Update this file when the tool surface (CLI commands, MCP tools, config keys) changes — it is the single source of truth for the spec.













<!-- evoride:edits:start -->
## Edit tracking (EvorIDE)
After you create or modify a file, append ONE json line to the file at the path in the `$EVORIDE_EDITS` env var, recording what you changed:
`echo '{"file":"<repo-relative path>","info":"<short what/why>"}' >> "$EVORIDE_EDITS"`
This lets EvorIDE show which files you changed in this session. Do it for every edit.
<!-- evoride:edits:end -->

<!-- evoride:tasks:start -->
## Tasks (EvorIDE)
You have an `evor` CLI for THIS project's task board. Use it instead of guessing — it keeps the board in sync with what you're actually doing.
- `evor task list` — what's open (add `--status todo` / `--json`). Run this first if the user asks what to work on.
- `evor task new "<short title>" [--desc "<what/why>"]` — start NEW work that isn't already listed. Creates the task, marks it in progress, and binds it to THIS terminal. Add `--todo` to just queue it. Do this once per distinct piece of work, before you start changing code; don't recreate an existing task.
- `evor task done` — finished the current task. `evor task start` — back to in progress. `evor task block --note "why"` — stuck.
- `evor task note "<text>"` — progress note. `evor task step done "<step title>"` — tick a breakdown step.
Report honestly and promptly. Do NOT create Jira (or other external) tickets unless the user explicitly asks. Run `evor --help` for the full list.
(Fallback if `evor` is unavailable: append one JSON line to `$EVORIDE_TASKS`, e.g. `echo '{"new_task":"…"}' >> "$EVORIDE_TASKS"`; `{"status":"doing|done"}`; read `$EVORIDE_PROJECT_TASKS` to list.)
<!-- evoride:tasks:end -->
