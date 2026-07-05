---
title: On Writing Tools
author: A. Reader
date: 2026-07-05
---
# On Writing Tools

Every writing tool makes an argument about how thinking should feel. The blinking cursor in an empty document is not neutral — it is an *invitation shaped like a demand*.

> The best tool is the one that disappears. You should be arguing with your ideas, not your software.

What follows is a short field guide to the tools I keep coming back to, and the single quality they share.

## The one quality that matters

Durability. A note written today should open, unchanged, in ten years. That rules out most of what the industry sells and leaves a small, stubborn category:

- **Plain text** — the format that outlives its editors
- **Markdown** — structure without ceremony
- **Files and folders** — a database everyone already knows how to use

If your words live inside someone else's product, you are renting your own thoughts.

## What a good editor does

A good editor gets three things right and refuses to add a fourth.

1. It renders exactly what it exports — no surprises at the finish line.
2. It stays fast enough to feel like paper.
3. It leaves your files where you put them.

### Speed is a feature

Latency is the tax you pay on every keystroke. Below fifty milliseconds a tool feels like an extension of the hand; above it, like a conversation with a slow clerk.

```toml
[render]
theme = "default"
math = true

[export.pdf]
paper = "a4"
toc = true
```

### One renderer, three destinations

| Destination | Use              | Fidelity |
| ----------- | ---------------- | -------- |
| HTML        | web, email       | exact    |
| PDF         | print, archive   | exact    |
| DOCX        | collaborators    | close    |

The promise is simple: the preview *is* the document. There is no second rendering path waiting to betray you.

## A short checklist

- [x] Words stored as plain files
- [x] Preview matches export
- [ ] Zero runtime dependencies
- [ ] Exports a 100-page book in under five seconds

---

Write in something you can still open when the company is gone. Everything else is decoration. Read more in [the quiet web](the-quiet-web.md).
