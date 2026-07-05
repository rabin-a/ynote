//! PDF export: comrak AST → Typst markup → in-process Typst compile → PDF bytes.
//!
//! Highest-risk component, so it is built defensively:
//! - Text runs are emitted as Typst **string literals in code position**
//!   (`#("...")`), not markup. A string value shown as content is never
//!   re-interpreted as markup, so arbitrary Unicode text can never break
//!   compilation — only string-literal escaping (`\`, `"`, control chars) is
//!   needed. This is what makes the escaping property tests pass.
//! - Structure (headings, lists, tables, quotes, code) uses Typst element
//!   functions (`#list`, `#table`, `#raw`, `#quote`) for the same reason.
//! - Images are only emitted when the file resolves inside the project;
//!   otherwise the alt text is shown, so a missing image never fails export.
//! - Math is typeset as raw source (v1): markdown math is LaTeX-flavoured and
//!   would frequently fail Typst's math parser, and a compile error aborts the
//!   whole document. A LaTeX→Typst math converter is future work.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::Arena;
use typst::diag::{FileError, FileResult, SourceResult};
use typst::foundations::{Bytes, Datetime, Dict};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use crate::error::{Error, Result};
use crate::parse;
use crate::project::Project;

/// Render a document to PDF bytes (TOC per `ynote.toml`).
pub fn render_pdf(project: &Project, rel: &Path, source: &str) -> Result<Vec<u8>> {
    render_pdf_with(project, rel, source, None)
}

/// Render a document to PDF bytes, optionally overriding the table-of-contents
/// setting for this one export (`None` = use the project config value).
pub fn render_pdf_with(
    project: &Project,
    rel: &Path,
    source: &str,
    toc: Option<bool>,
) -> Result<Vec<u8>> {
    let world = TypstWorld::new(
        build_typst_source_with(project, rel, source, toc),
        project.root().to_path_buf(),
    );
    compile(&world)
}

/// Build the full Typst source (template + config + lowered body) for a
/// document. Exposed so tests can inspect the lowering without compiling.
pub fn build_typst_source(project: &Project, rel: &Path, source: &str) -> String {
    build_typst_source_with(project, rel, source, None)
}

fn build_typst_source_with(
    project: &Project,
    rel: &Path,
    source: &str,
    toc: Option<bool>,
) -> String {
    let arena = Arena::new();
    let (root, fm) = parse::parse(&arena, source);

    let cfg = &project.config().export.pdf;
    let lowerer = Lowerer::new(project, rel, root);
    let body = lowerer.document(root);

    let title = parse::document_title(root, fm.as_ref());
    let author = fm.as_ref().and_then(|f| f.author());
    let date = fm.as_ref().and_then(|f| f.date());

    let template = crate::assets::typst_template(&cfg.template);
    let show_line = build_show_line(
        cfg,
        title.as_deref(),
        author.as_deref(),
        date.as_deref(),
        toc.unwrap_or(cfg.toc),
    );
    format!("{template}\n{show_line}\n\n{body}\n")
}

/// Compose the `#show: ynote-doc.with(...)` line from config + front matter.
fn build_show_line(
    cfg: &crate::config::PdfConfig,
    title: Option<&str>,
    author: Option<&str>,
    date: Option<&str>,
    toc: bool,
) -> String {
    let opt = |v: Option<&str>| match v {
        Some(s) => ts_str(s),
        None => "none".to_string(),
    };
    format!(
        "#show: ynote-doc.with(\n  \
title: {title},\n  \
author: {author},\n  \
date: {date},\n  \
paper: {paper},\n  \
margin: {margin},\n  \
body-font: ({font}, \"Libertinus Serif\", \"New Computer Modern\"),\n  \
toc: {toc},\n)",
        title = opt(title),
        author = opt(author),
        date = opt(date),
        paper = ts_str(&valid_paper(&cfg.paper)),
        margin = valid_margin(&cfg.margin), // validated raw length literal
        font = ts_str(&cfg.font),
        toc = toc,
    )
}

/// Validate a margin value before splicing it into Typst *as raw code*. Rejects
/// anything that isn't a plain length literal (guards against both a compile
/// break and Typst code injection via config). Falls back to the default.
fn valid_margin(s: &str) -> String {
    let t = s.trim();
    if is_length_literal(t) {
        t.to_string()
    } else {
        "2.5cm".to_string()
    }
}

fn is_length_literal(s: &str) -> bool {
    // <number><unit>, e.g. `2.5cm`, `18pt`, `10%`, `0`.
    let units = ["cm", "mm", "in", "pt", "pc", "em", "%"];
    let num_part = if let Some(u) = units.iter().find(|u| s.ends_with(**u)) {
        &s[..s.len() - u.len()]
    } else {
        s // bare number (points)
    };
    let num_part = num_part.trim();
    !num_part.is_empty()
        && num_part.parse::<f64>().is_ok()
        && num_part
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
}

/// Validate the paper name against Typst's common set; unknown -> `a4` so an
/// export never aborts on a typo'd paper size.
fn valid_paper(s: &str) -> String {
    const KNOWN: &[&str] = &[
        "a0",
        "a1",
        "a2",
        "a3",
        "a4",
        "a5",
        "a6",
        "a7",
        "a8",
        "a9",
        "a10",
        "iso-b1",
        "iso-b2",
        "iso-b3",
        "iso-b4",
        "iso-b5",
        "iso-b6",
        "us-letter",
        "us-legal",
        "us-tabloid",
        "us-executive",
        "us-statement",
        "presentation-16-9",
        "presentation-4-3",
    ];
    let t = s.trim().to_ascii_lowercase();
    if KNOWN.contains(&t.as_str()) {
        t
    } else {
        "a4".to_string()
    }
}

// ---------------------------------------------------------------------------
// AST -> Typst lowering
// ---------------------------------------------------------------------------

struct Lowerer<'a> {
    project: &'a Project,
    rel: PathBuf,
    /// footnote name -> definition node, so references inline their content.
    footnotes: HashMap<String, &'a AstNode<'a>>,
    /// footnote names currently being expanded — guards against self- or
    /// mutually-referential footnotes recursing until the stack overflows.
    active_footnotes: std::cell::RefCell<std::collections::HashSet<String>>,
}

impl<'a> Lowerer<'a> {
    fn new(project: &'a Project, rel: &Path, root: &'a AstNode<'a>) -> Self {
        let mut footnotes = HashMap::new();
        for node in root.descendants() {
            if let NodeValue::FootnoteDefinition(def) = &node.data.borrow().value {
                footnotes.insert(def.name.clone(), node);
            }
        }
        Lowerer {
            project,
            rel: rel.to_path_buf(),
            footnotes,
            active_footnotes: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Whole document body = its block children, separated by parbreaks.
    fn document(&self, root: &'a AstNode<'a>) -> String {
        self.blocks(root)
    }

    /// Render all block children of `node`, separated by blank lines.
    fn blocks(&self, node: &'a AstNode<'a>) -> String {
        let mut parts = Vec::new();
        for child in node.children() {
            let s = self.block(child);
            if !s.trim().is_empty() {
                parts.push(s);
            }
        }
        parts.join("\n\n")
    }

    fn block(&self, node: &'a AstNode<'a>) -> String {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::FrontMatter(_) => String::new(),
            NodeValue::Heading(h) => {
                let level = h.level.clamp(1, 6) as usize;
                format!("{} {}", "=".repeat(level), self.inline(node))
            }
            NodeValue::Paragraph => self.inline(node),
            NodeValue::BlockQuote => {
                format!("#quote(block: true)[\n{}\n]", self.blocks(node))
            }
            NodeValue::List(list) => self.list(node, &list),
            NodeValue::CodeBlock(cb) => {
                let lang = cb.info.split_whitespace().next().unwrap_or("");
                if lang.is_empty() {
                    format!("#raw(block: true, {})", ts_str(&cb.literal))
                } else {
                    format!(
                        "#raw(block: true, lang: {}, {})",
                        ts_str(lang),
                        ts_str(&cb.literal)
                    )
                }
            }
            NodeValue::ThematicBreak => {
                "#line(length: 100%, stroke: 0.5pt + luma(200))".to_string()
            }
            NodeValue::Table(table) => self.table(node, &table.alignments),
            // Definitions are inlined at their reference site; emit nothing here.
            NodeValue::FootnoteDefinition(_) => String::new(),
            NodeValue::HtmlBlock(_) => String::new(),
            // Fallback: treat unknown blocks as their inline content.
            _ => self.inline(node),
        }
    }

    fn list(&self, node: &'a AstNode<'a>, list: &comrak::nodes::NodeList) -> String {
        let mut items = Vec::new();
        for item in node.children() {
            let value = item.data.borrow().value.clone();
            let content = match value {
                NodeValue::TaskItem(task) => {
                    let mark = if task.symbol.is_some() {
                        "\u{2611} " // ballot box with check
                    } else {
                        "\u{2610} " // ballot box
                    };
                    format!("{}{}", ts_content(mark), self.blocks(item))
                }
                _ => self.blocks(item),
            };
            items.push(format!("[{content}]"));
        }
        let joined = items.join(", ");
        match list.list_type {
            ListType::Bullet => format!("#list({joined})"),
            ListType::Ordered => {
                if list.start != 1 {
                    format!("#enum(start: {}, {joined})", list.start)
                } else {
                    format!("#enum({joined})")
                }
            }
        }
    }

    fn table(&self, node: &'a AstNode<'a>, aligns: &[TableAlignment]) -> String {
        let align_of = |a: Option<&TableAlignment>| match a {
            Some(TableAlignment::Center) => "center",
            Some(TableAlignment::Right) => "right",
            _ => "left",
        };
        let ncols = aligns.len().max(1);
        let align_list: Vec<&str> = (0..ncols).map(|i| align_of(aligns.get(i))).collect();

        let mut cells = Vec::new();
        let mut header_cells: Option<Vec<String>> = None;
        for row in node.children() {
            let is_header = matches!(&row.data.borrow().value, NodeValue::TableRow(true));
            let row_cells: Vec<String> = row
                .children()
                .map(|cell| format!("[{}]", self.inline(cell)))
                .collect();
            if is_header && header_cells.is_none() {
                header_cells = Some(row_cells);
            } else {
                cells.extend(row_cells);
            }
        }

        let mut out = String::from("#table(\n");
        out.push_str(&format!("  columns: {ncols},\n"));
        out.push_str(&format!("  align: ({},),\n", align_list.join(", ")));
        if let Some(hcells) = header_cells {
            out.push_str(&format!("  table.header({}),\n", hcells.join(", ")));
        }
        if !cells.is_empty() {
            out.push_str("  ");
            out.push_str(&cells.join(", "));
            out.push_str(",\n");
        }
        out.push(')');
        out
    }

    /// Render the inline children of `node`, concatenated.
    fn inline(&self, node: &'a AstNode<'a>) -> String {
        let mut out = String::new();
        for child in node.children() {
            out.push_str(&self.inline_node(child));
        }
        out
    }

    fn inline_node(&self, node: &'a AstNode<'a>) -> String {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Text(t) => ts_content(&t),
            NodeValue::SoftBreak => " ".to_string(),
            NodeValue::LineBreak => "#linebreak()".to_string(),
            NodeValue::Emph => format!("#emph[{}]", self.inline(node)),
            NodeValue::Strong => format!("#strong[{}]", self.inline(node)),
            NodeValue::Strikethrough => format!("#strike[{}]", self.inline(node)),
            NodeValue::Superscript => format!("#super[{}]", self.inline(node)),
            NodeValue::Code(c) => format!("#raw({})", ts_str(&c.literal)),
            NodeValue::HtmlInline(_) => String::new(),
            NodeValue::Link(link) => {
                format!("#link({})[{}]", ts_str(&link.url), self.inline(node))
            }
            NodeValue::Image(link) => self.image(&link.url, node),
            NodeValue::FootnoteReference(fr) => match self.footnotes.get(&fr.name) {
                Some(def) => {
                    if self.active_footnotes.borrow().contains(&fr.name) {
                        // Cycle: emit a plain marker instead of recursing forever.
                        ts_content(&format!("[{}]", fr.name))
                    } else {
                        self.active_footnotes.borrow_mut().insert(fr.name.clone());
                        let inner = self.blocks(def);
                        self.active_footnotes.borrow_mut().remove(&fr.name);
                        format!("#footnote[{}]", inner)
                    }
                }
                None => String::new(),
            },
            NodeValue::Math(m) => {
                // v1: typeset raw so LaTeX-flavoured math never breaks compilation.
                if m.display_math {
                    format!("#align(center, raw({}))", ts_str(m.literal.trim()))
                } else {
                    format!("#raw({})", ts_str(&m.literal))
                }
            }
            _ => self.inline(node),
        }
    }

    /// Emit an image if it resolves inside the project; else its alt text.
    fn image(&self, url: &str, node: &'a AstNode<'a>) -> String {
        let alt = parse::node_text(node);
        if is_remote(url) {
            return ts_content(&alt);
        }
        // Resolve relative to the document, keep inside the project root, and
        // probe that the image is actually decodable — a broken image must
        // degrade to alt text rather than abort the whole compile.
        match self.project.resolve_asset(&self.rel, url) {
            Ok(abs) if abs.is_file() && image_is_valid(&abs) => {
                let rootrel = self.project.relativize(&abs);
                let p = rootrel.to_string_lossy().replace('\\', "/");
                format!("#image({}, width: 80%)", ts_str(&p))
            }
            _ => ts_content(&alt),
        }
    }
}

fn is_remote(url: &str) -> bool {
    let u = url.trim_start();
    u.starts_with("http://") || u.starts_with("https://") || u.starts_with("//")
}

/// Cheaply verify a raster image is well-formed enough to embed. SVGs are
/// passed through (Typst decodes them itself). Catches empty/truncated/
/// wrong-format files so they degrade to alt text instead of failing export.
fn image_is_valid(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return true;
    }
    match image::ImageReader::open(path) {
        Ok(reader) => match reader.with_guessed_format() {
            Ok(r) => r.decode().is_ok(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// A Typst string literal (with surrounding quotes), safely escaped.
fn ts_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A text run as Typst content: a string literal shown in code position, which
/// is never re-parsed as markup.
fn ts_content(s: &str) -> String {
    format!("#({})", ts_str(s))
}

// ---------------------------------------------------------------------------
// Typst World (in-process compilation)
// ---------------------------------------------------------------------------

/// Fonts are loaded once from `typst-assets` and shared across compiles.
static FONTS: LazyLock<(LazyHash<FontBook>, Vec<Font>)> = LazyLock::new(|| {
    let mut fonts = Vec::new();
    for data in typst_assets::fonts() {
        let bytes = Bytes::new(data.to_vec());
        for font in Font::iter(bytes) {
            fonts.push(font);
        }
    }
    let book = FontBook::from_fonts(&fonts);
    (LazyHash::new(book), fonts)
});

use std::sync::LazyLock;

struct TypstWorld {
    library: LazyHash<Library>,
    main: Source,
    /// Project root; `file()` serves images from here with a path-safety check.
    root: PathBuf,
    canonical_root: PathBuf,
}

impl TypstWorld {
    fn new(source_text: String, root: PathBuf) -> Self {
        let library = Library::builder().with_inputs(Dict::new()).build();
        let vpath = VirtualPath::new("main.typ").expect("valid virtual path");
        let file_id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
        let main = Source::new(file_id, source_text);
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        TypstWorld {
            library: LazyHash::new(library),
            main,
            root,
            canonical_root,
        }
    }

    /// Resolve a Typst virtual path to a real file inside the project root.
    fn resolve(&self, id: FileId) -> Option<PathBuf> {
        let vpath = id.vpath();
        let rel = vpath.get_without_slash();
        let joined = self.root.join(rel);
        let canonical = joined.canonicalize().ok()?;
        if canonical.starts_with(&self.canonical_root) {
            Some(canonical)
        } else {
            None
        }
    }
}

impl World for TypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.0
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self
            .resolve(id)
            .ok_or_else(|| FileError::NotFound(id.vpath().get_without_slash().into()))?;
        let bytes = std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))?;
        Ok(Bytes::new(bytes))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.1.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        Some(Datetime::from_ymd(1970, 1, 1).unwrap())
    }
}

fn compile(world: &TypstWorld) -> Result<Vec<u8>> {
    let result: typst::diag::Warned<SourceResult<typst_layout::PagedDocument>> =
        typst::compile(world);
    let document = result
        .output
        .map_err(|errs| Error::Pdf(format_diagnostics(&errs)))?;
    let options = typst_pdf::PdfOptions::default();
    typst_pdf::pdf(&document, &options).map_err(|errs| Error::Pdf(format_diagnostics(&errs)))
}

fn format_diagnostics(errs: &ecow::EcoVec<typst::diag::SourceDiagnostic>) -> String {
    let mut msgs = Vec::new();
    for e in errs.iter().take(5) {
        msgs.push(e.message.to_string());
    }
    if msgs.is_empty() {
        "unknown Typst error".to_string()
    } else {
        msgs.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
---
title: Smoke Test
author: ynote
date: 2026-07-05
---

# Heading One

A paragraph with **bold**, *italic*, ~~strike~~, `inline code`, and a
[link](https://example.com). Special Typst chars: # * _ @ $ [ ] \\ < >.

## Lists

- bullet one
- bullet two
  - nested
- [x] done task
- [ ] todo task

1. first
2. second

## Code

```rust
fn main() {
    println!(\"hello, world\");
}
```

## Table

| Left | Center | Right |
|:-----|:------:|------:|
| a    | b      | c     |

## Quote

> A quoted line.

Footnote here.[^1]

[^1]: The footnote body.

---

Inline math $a^2 + b^2$ and display:

$$E = mc^2$$
";

    #[test]
    fn renders_valid_pdf() {
        let dir = std::env::temp_dir().join("ynote_pdf_smoke");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        std::fs::write(&file, DOC).unwrap();

        let project = Project::open(&dir).unwrap();
        let pdf = render_pdf(&project, Path::new("doc.md"), DOC).expect("pdf render");
        assert!(pdf.len() > 1000, "pdf too small: {} bytes", pdf.len());
        assert_eq!(&pdf[0..5], b"%PDF-", "missing PDF magic");
    }

    /// The escaping property from CLAUDE.md §8: an arbitrary Unicode string
    /// placed in body text must never break Typst compilation. We test a
    /// curated adversarial set plus deterministically-generated noise.
    #[test]
    fn arbitrary_text_never_breaks_compilation() {
        let dir = std::env::temp_dir().join("ynote_pdf_escape");
        std::fs::create_dir_all(&dir).unwrap();
        let project = Project::open(&dir).unwrap();

        let mut cases: Vec<String> = vec![
            "# ",
            "#[]",
            "*_`$@<>\\",
            "]]][[[",
            "-- --- ->",
            "```",
            "$a$ $$b$$",
            "1. 2. 3.",
            "> not a quote",
            "\\\\ \\# \\* backslashes",
            "emoji 😀 और हिन्दी ☃ \u{202e}rtl",
            "\"smart\" 'quotes' -- ...",
            "#let x = 5",
            "#(dangerous)",
            "control\u{0007}bell\ttab",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // Deterministic pseudo-random punctuation noise (no rng dep).
        let alphabet: Vec<char> = "#*_`$@<>[]\\{}()~-+=/.:;\"' aZ9\n\t😀".chars().collect();
        let mut state: u64 = 0x9E3779B97F4A7C15;
        for _ in 0..40 {
            let mut s = String::new();
            let len = 1 + (state % 30) as usize;
            for _ in 0..len {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                s.push(alphabet[(state >> 33) as usize % alphabet.len()]);
            }
            cases.push(s);
        }

        for case in &cases {
            let md = format!("Body: {case}\n");
            let pdf = render_pdf(&project, Path::new("doc.md"), &md)
                .unwrap_or_else(|e| panic!("compilation broke on {case:?}: {e}"));
            assert_eq!(&pdf[0..5], b"%PDF-");
        }
    }

    #[test]
    fn toc_override_flows_into_source() {
        let dir = std::env::temp_dir().join("ynote_toc_override");
        std::fs::create_dir_all(&dir).unwrap();
        let project = Project::open(&dir).unwrap();
        let md = "# H\n\ntext\n";
        let on = build_typst_source_with(&project, Path::new("d.md"), md, Some(true));
        let off = build_typst_source_with(&project, Path::new("d.md"), md, Some(false));
        assert!(on.contains("toc: true"));
        assert!(off.contains("toc: false"));
    }

    #[test]
    fn self_referential_footnote_does_not_overflow() {
        let dir = std::env::temp_dir().join("ynote_pdf_fncycle");
        std::fs::create_dir_all(&dir).unwrap();
        let project = Project::open(&dir).unwrap();
        // A footnote whose body references itself, and two mutually-referential ones.
        let md = "See[^a] and[^b].\n\n[^a]: loops back[^a]\n\n[^b]: go[^c]\n\n[^c]: back[^b]\n";
        let pdf = render_pdf(&project, Path::new("doc.md"), md).expect("must not overflow");
        assert_eq!(&pdf[0..5], b"%PDF-");
    }

    #[test]
    fn missing_and_broken_images_fall_back_to_alt() {
        let dir = std::env::temp_dir().join("ynote_pdf_img");
        std::fs::create_dir_all(&dir).unwrap();
        // A file that exists but is not a valid image.
        std::fs::write(dir.join("bad.png"), b"not really a png").unwrap();
        let project = Project::open(&dir).unwrap();
        let md = "![missing](nope.png) and ![bad](bad.png) still export.\n";
        let pdf =
            render_pdf(&project, Path::new("doc.md"), md).expect("must not fail on bad images");
        assert_eq!(&pdf[0..5], b"%PDF-");
    }
}
