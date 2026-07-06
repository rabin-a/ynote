// The Welcome note. This is plain markdown rendered by ynote's own engine —
// the landing "page" is literally a note. It's seeded into your browser on the
// first visit and, like every note, saves automatically as you type.
export const WELCOME_MD = `---
title: Welcome to ynote
---

# Welcome to ynote 👋

**ynote** is a markdown editor, live preview, and exporter that runs entirely
in your browser. Notes save **automatically in this browser** — no sign-in, no
server, nothing uploaded. What you're reading right now is a note, rendered by
the same Rust engine that powers the desktop app and the agent tools.

> Edit this note in the left pane and watch the preview update. Your changes
> are saved to this browser as you type — come back later and they'll be here.

---

## Get going in seconds

- Press **new note** (the **+** in the sidebar, or **⌘⏎ / Ctrl+Enter**) to start writing.
- Every note autosaves. The list on the left is newest-first, like a notebook.
- Switch **Source · Split · Reading** in the top-right (**⌘\\\\ / Ctrl+\\\\** cycles).
- Export the current note to HTML with **⌘E / Ctrl+E**.

Safe shortcuts only — ynote never uses ⌘N/⌘T/⌘W, since browsers reserve those.

## Also available

| Where | What you get |
|---|---|
| **Desktop app** | Native window, system fonts, offline **PDF & DOCX** export |
| **Your AI agent (MCP)** | Let Claude read, write, and export your notes directly |

Point the MCP server at a folder and your agent gets a small, typed tool
surface — \`write_document\`, \`edit_section\`, \`export\`:

\`\`\`json
{
  "mcpServers": {
    "ynote": { "command": "ynote-mcp", "args": ["--project", "."] }
  }
}
\`\`\`

---

## Everything renders

- [x] Read this welcome note
- [ ] Edit this line — the preview updates as you type
- [ ] Make a new note and write something

Inline styles all work: **bold**, *italic*, ~~strikethrough~~, \`inline code\`,
and [links](https://ynote.onl). Code blocks are highlighted in WASM, never by a
JavaScript library:

\`\`\`rust
// The one renderer — preview and export come from this same function.
pub fn render_html(source: &str, opts: &RenderOptions) -> Result<String> {
    let arena = Arena::new();
    let (root, fm) = parse::parse(&arena, source);
    // …walk the AST once, emit HTML.
}
\`\`\`

Happy writing.
`;
