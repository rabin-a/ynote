---
title: Kitchen Sink
author: papery
date: 2026-07-05
---

# Kitchen Sink

A demo document exercising **every** markdown construct papery supports, so
preview, HTML, PDF, and DOCX output can be compared side by side.

Inline styles: **bold**, *italic*, ~~strikethrough~~, `inline code`, a
[link](https://example.com "with title"), an autolink <https://rust-lang.org>,
and 2^nd^ superscript. Tricky Typst characters must survive: `# * _ @ $ [ ] \ < >`.

## Lists

- Bullet one
- Bullet two
  - Nested bullet
  - Another nested
- Bullet three

1. First ordered
2. Second ordered
   1. Nested ordered
3. Third ordered

Task list:

- [x] Completed task
- [ ] Pending task

## Code

```rust
fn main() {
    let msg = "hello, world";
    println!("{msg}");
}
```

Plain fenced block:

```
no language here
line two
```

## Table

| Feature   | Preview | Export |
|:----------|:-------:|-------:|
| Headings  |   yes   |    yes |
| Tables    |   yes   |    yes |
| Footnotes |   yes   |    yes |

## Quote

> The best way to predict the future is to invent it.
>
> — Alan Kay

## Image

![A red dot](img/dot.png)

## Math

Inline math $a^2 + b^2 = c^2$ within a sentence, and a display block:

$$\int_0^\infty e^{-x} \, dx = 1$$

## Footnotes

Here is a statement needing a citation.[^src]

[^src]: This is the footnote body with a [link](https://example.com).

---

The end.
