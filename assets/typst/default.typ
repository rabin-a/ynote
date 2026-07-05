// papery default PDF template.
//
// Defines a single template function, `papery-doc`, applied to the lowered
// document body via `#show: papery-doc.with(...)`. The exporter composes that
// show-line from `papery.toml` (paper/margin/font/toc) and front matter
// (title/author/date), then appends the lowered markdown body.
//
// Keep all page setup, fonts, and heading styles here — the exporter only
// emits content, never layout.

#let papery-doc(
  title: none,
  author: none,
  date: none,
  paper: "a4",
  margin: 2.5cm,
  body-font: ("Inter", "Helvetica Neue", "Arial"),
  mono-font: ("JetBrains Mono", "DejaVu Sans Mono", "Menlo", "Consolas"),
  toc: true,
  body,
) = {
  set document(
    title: if title != none { title } else { "" },
    author: if author != none { (author,) } else { () },
  )

  set page(
    paper: paper,
    margin: margin,
    numbering: "1",
    footer: context {
      let n = counter(page).at(here()).first()
      // Suppress the number on the title page.
      if n > 1 or title == none {
        align(center)[#text(size: 9pt, fill: luma(120))[#n]]
      }
    },
  )

  set text(font: body-font, size: 10.5pt, lang: "en")
  set par(justify: true, leading: 0.68em, spacing: 1.1em)

  // Links: subtle blue, no underline.
  show link: set text(fill: rgb("#0969da"))

  // Inline + block code use the mono font.
  show raw: set text(font: mono-font, size: 9.2pt)
  show raw.where(block: true): block.with(
    fill: luma(245),
    inset: 8pt,
    radius: 4pt,
    width: 100%,
  )
  show raw.where(block: false): box.with(
    fill: luma(240),
    inset: (x: 3pt, y: 0pt),
    outset: (y: 3pt),
    radius: 2pt,
  )

  // Block quotes.
  show quote.where(block: true): it => {
    set text(fill: luma(90))
    block(
      inset: (left: 12pt),
      stroke: (left: 2.5pt + luma(200)),
      it.body,
    )
  }

  // Tables: light header, thin rules.
  set table(stroke: 0.5pt + luma(200))

  // Headings.
  set heading(numbering: none)
  show heading: set block(above: 1.4em, below: 0.7em)
  show heading.where(level: 1): set text(size: 1.7em, weight: 700)
  show heading.where(level: 2): set text(size: 1.4em, weight: 700)
  show heading.where(level: 3): set text(size: 1.2em, weight: 600)
  show heading.where(level: 4): set text(size: 1.05em, weight: 600)

  // Title block on the first page.
  if title != none {
    align(center)[
      #text(size: 2.1em, weight: 700)[#title]
      #if author != none [ \ #v(0.4em) #text(size: 1.1em, fill: luma(90))[#author] ]
      #if date != none [ \ #v(0.2em) #text(size: 0.95em, fill: luma(120))[#date] ]
    ]
    v(1.2em)
  }

  if toc {
    outline(title: "Contents", depth: 3, indent: auto)
    pagebreak()
  }

  body
}
