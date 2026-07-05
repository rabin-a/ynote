//! DOCX export: comrak AST → `docx-rs` → .docx bytes.
//!
//! Maps to Word built-in styles (Heading 1–6, Quote) and real Word tables and
//! numbered/bulleted lists, so downstream editing in Word/Google Docs behaves.
//!
//! Documented v1 gaps (per spec):
//! - Math is emitted as plain text (no OMML).
//! - Footnotes are collected into an endnote-style "Notes" section at the end;
//!   references render as `[n]`.
//! - Images render as their alt text in italics (no embedded picture yet).
//! - Links render as styled text (the URL is not a live hyperlink field).

use std::path::Path;

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::Arena;
use docx_rs::*;

use crate::error::{Error, Result};
use crate::parse;
use crate::project::Project;

const BULLET_NUM: usize = 1;
const MONO: &str = "Consolas";

/// Render a document to DOCX bytes.
pub fn render_docx(_project: &Project, _rel: &Path, source: &str) -> Result<Vec<u8>> {
    let arena = Arena::new();
    let (root, _fm) = parse::parse(&arena, source);

    let mut b = Builder::default();
    b.blocks(root, 0);
    b.finish()
}

/// Accumulated document elements (docx-rs consumes `self` on every `add_*`,
/// so we collect first and fold into the `Docx` at the end).
enum El {
    Para(Box<Paragraph>),
    Table(Box<Table>),
}

#[derive(Clone, Copy, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

/// Concrete numbering ids for ordered lists start here; each ordered list gets
/// its own id so it restarts at 1 (or its authored start) instead of continuing
/// the previous list's counter.
const ORDERED_BASE: usize = 100;

#[derive(Default)]
struct Builder {
    els: Vec<El>,
    /// Footnote texts collected for the endnote section, in first-seen order.
    footnotes: Vec<String>,
    /// footnote name -> 1-based number, so repeated references share a number.
    footnote_index: std::collections::HashMap<String, usize>,
    /// (numId, start) for every ordered list, registered in `finish`.
    ordered_lists: Vec<(usize, usize)>,
}

impl Builder {
    /// Allocate the numbering id for a list: bullets share one id; each ordered
    /// list gets a fresh id (registered with its start) so it restarts.
    fn alloc_list(&mut self, list: &comrak::nodes::NodeList) -> usize {
        match list.list_type {
            ListType::Bullet => BULLET_NUM,
            ListType::Ordered => {
                let id = ORDERED_BASE + self.ordered_lists.len();
                self.ordered_lists.push((id, list.start.max(1)));
                id
            }
        }
    }
}

impl Builder {
    fn blocks<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) {
        for child in node.children() {
            self.block(child, depth);
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>, depth: usize) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::FrontMatter(_) => {}
            NodeValue::Document => self.blocks(node, depth),

            NodeValue::Heading(h) => {
                let style = format!("Heading{}", h.level.clamp(1, 6));
                let mut p = Paragraph::new().style(&style);
                for run in self.inline(node, Fmt::default()) {
                    p = p.add_run(run);
                }
                self.els.push(El::Para(Box::new(p)));
            }

            NodeValue::Paragraph => {
                let mut p = Paragraph::new();
                for run in self.inline(node, Fmt::default()) {
                    p = p.add_run(run);
                }
                self.els.push(El::Para(Box::new(p)));
            }

            NodeValue::BlockQuote => {
                // Style each contained paragraph as Quote.
                for child in node.children() {
                    if matches!(child.data.borrow().value, NodeValue::Paragraph) {
                        let mut p = Paragraph::new().style("Quote");
                        for run in self.inline(child, Fmt::default()) {
                            p = p.add_run(run);
                        }
                        self.els.push(El::Para(Box::new(p)));
                    } else {
                        self.block(child, depth);
                    }
                }
            }

            NodeValue::List(list) => {
                let num_id = self.alloc_list(&list);
                for item in node.children() {
                    self.list_item(item, num_id, depth);
                }
            }

            NodeValue::CodeBlock(cb) => {
                for line in cb.literal.lines() {
                    let run = Run::new()
                        .add_text(line)
                        .fonts(RunFonts::new().ascii(MONO).hi_ansi(MONO));
                    self.els
                        .push(El::Para(Box::new(Paragraph::new().add_run(run))));
                }
                if cb.literal.is_empty() {
                    self.els.push(El::Para(Box::new(Paragraph::new())));
                }
            }

            NodeValue::ThematicBreak => {
                self.els.push(El::Para(Box::new(
                    Paragraph::new().add_run(Run::new().add_text("\u{2014}".repeat(20))),
                )));
            }

            NodeValue::Table(table) => {
                let t = self.table(node, &table.alignments);
                self.els.push(El::Table(Box::new(t)));
            }

            NodeValue::FootnoteDefinition(_) => {} // rendered inline at reference
            NodeValue::HtmlBlock(_) => {}

            _ => self.blocks(node, depth),
        }
    }

    fn list_item<'a>(&mut self, item: &'a AstNode<'a>, num_id: usize, depth: usize) {
        let level = depth.min(8);
        // Task-item prefix.
        let prefix = match &item.data.borrow().value {
            NodeValue::TaskItem(t) => {
                if t.symbol.is_some() {
                    Some("\u{2611} ")
                } else {
                    Some("\u{2610} ")
                }
            }
            _ => None,
        };

        let mut first_para = true;
        for child in item.children() {
            let value = child.data.borrow().value.clone();
            match value {
                NodeValue::Paragraph => {
                    let mut p = Paragraph::new();
                    if first_para {
                        // Only the first paragraph carries the list marker;
                        // later paragraphs are continuation text at the same
                        // indent (so they don't each get their own number).
                        p = p.numbering(NumberingId::new(num_id), IndentLevel::new(level));
                        if let Some(pre) = prefix {
                            p = p.add_run(Run::new().add_text(pre));
                        }
                    } else {
                        p = p.indent(Some((level as i32 + 1) * 720), None, None, None);
                    }
                    for run in self.inline(child, Fmt::default()) {
                        p = p.add_run(run);
                    }
                    self.els.push(El::Para(Box::new(p)));
                    first_para = false;
                }
                NodeValue::List(inner) => {
                    let inner_id = self.alloc_list(&inner);
                    for sub in child.children() {
                        self.list_item(sub, inner_id, depth + 1);
                    }
                }
                _ => self.block(child, depth),
            }
        }
    }

    fn table<'a>(&mut self, node: &'a AstNode<'a>, aligns: &[TableAlignment]) -> Table {
        let mut rows = Vec::new();
        for row in node.children() {
            let is_header = matches!(&row.data.borrow().value, NodeValue::TableRow(true));
            let mut cells = Vec::new();
            for (col, cell) in row.children().enumerate() {
                let mut p = Paragraph::new();
                match aligns.get(col) {
                    Some(TableAlignment::Center) => p = p.align(AlignmentType::Center),
                    Some(TableAlignment::Right) => p = p.align(AlignmentType::Right),
                    _ => {}
                }
                let fmt = Fmt {
                    bold: is_header,
                    ..Fmt::default()
                };
                for run in self.inline(cell, fmt) {
                    p = p.add_run(run);
                }
                cells.push(TableCell::new().add_paragraph(p));
            }
            rows.push(TableRow::new(cells));
        }
        Table::new(rows).set_grid(vec![]).style("Table Grid")
    }

    /// Render inline children of `node` into a flat list of styled runs.
    fn inline<'a>(&mut self, node: &'a AstNode<'a>, fmt: Fmt) -> Vec<Run> {
        let mut runs = Vec::new();
        for child in node.children() {
            self.inline_node(child, fmt, &mut runs);
        }
        runs
    }

    fn inline_node<'a>(&mut self, node: &'a AstNode<'a>, fmt: Fmt, runs: &mut Vec<Run>) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Text(t) => runs.push(styled(&t, fmt)),
            NodeValue::SoftBreak => runs.push(Run::new().add_text(" ")),
            NodeValue::LineBreak => runs.push(Run::new().add_break(BreakType::TextWrapping)),
            NodeValue::Emph => runs.extend(self.inline(
                node,
                Fmt {
                    italic: true,
                    ..fmt
                },
            )),
            NodeValue::Strong => runs.extend(self.inline(node, Fmt { bold: true, ..fmt })),
            NodeValue::Strikethrough => runs.extend(self.inline(
                node,
                Fmt {
                    strike: true,
                    ..fmt
                },
            )),
            NodeValue::Superscript => runs.extend(self.inline(node, fmt)),
            NodeValue::Code(c) => runs.push(styled(&c.literal, Fmt { code: true, ..fmt })),
            NodeValue::Link(link) => {
                // Text + parenthetical URL so the destination survives.
                runs.extend(self.inline(node, fmt));
                if !link.url.is_empty() {
                    runs.push(styled(
                        &format!(" ({})", link.url),
                        Fmt { code: false, ..fmt },
                    ));
                }
            }
            NodeValue::Image(_) => {
                let alt = parse::node_text(node);
                runs.push(styled(
                    &alt,
                    Fmt {
                        italic: true,
                        ..fmt
                    },
                ));
            }
            NodeValue::Math(m) => runs.push(styled(&m.literal, Fmt { code: true, ..fmt })),
            NodeValue::FootnoteReference(fr) => {
                // Reuse the number for a footnote referenced more than once, and
                // only collect its text on first encounter.
                let n = if let Some(&n) = self.footnote_index.get(&fr.name) {
                    n
                } else {
                    let text = self.find_footnote_text(node, &fr.name).unwrap_or_default();
                    self.footnotes.push(text);
                    let n = self.footnotes.len();
                    self.footnote_index.insert(fr.name.clone(), n);
                    n
                };
                runs.push(styled(&format!("[{n}]"), fmt));
            }
            NodeValue::HtmlInline(_) => {}
            _ => runs.extend(self.inline(node, fmt)),
        }
    }

    /// Walk up to the document root to find the matching footnote definition.
    fn find_footnote_text<'a>(&self, node: &'a AstNode<'a>, name: &str) -> Option<String> {
        let mut root = node;
        while let Some(p) = root.parent() {
            root = p;
        }
        for n in root.descendants() {
            if let NodeValue::FootnoteDefinition(def) = &n.data.borrow().value {
                if def.name == name {
                    return Some(parse::node_text(n).trim().to_string());
                }
            }
        }
        None
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        // Append the collected footnotes as an endnote-style section.
        if !self.footnotes.is_empty() {
            self.els.push(El::Para(Box::new(
                Paragraph::new()
                    .style("Heading2")
                    .add_run(Run::new().add_text("Notes")),
            )));
            for (i, text) in self.footnotes.iter().enumerate() {
                self.els.push(El::Para(Box::new(
                    Paragraph::new().add_run(Run::new().add_text(format!("[{}] {}", i + 1, text))),
                )));
            }
        }

        let mut docx = Docx::new()
            .add_style(
                Style::new("Quote", StyleType::Paragraph)
                    .name("Quote")
                    .indent(Some(720), None, None, None)
                    .italic(),
            )
            .add_abstract_numbering(bullet_abstract())
            .add_numbering(Numbering::new(BULLET_NUM, BULLET_NUM));

        // One abstract + concrete numbering per ordered list so each restarts.
        for (num_id, start) in &self.ordered_lists {
            docx = docx
                .add_abstract_numbering(ordered_abstract(*num_id, *start))
                .add_numbering(Numbering::new(*num_id, *num_id));
        }

        for el in self.els {
            docx = match el {
                El::Para(p) => docx.add_paragraph(*p),
                El::Table(t) => docx.add_table(*t),
            };
        }

        let mut buf: Vec<u8> = Vec::new();
        docx.build()
            .pack(std::io::Cursor::new(&mut buf))
            .map_err(|e| Error::Docx(e.to_string()))?;
        Ok(buf)
    }
}

fn styled(text: &str, fmt: Fmt) -> Run {
    let mut run = Run::new().add_text(text);
    if fmt.bold {
        run = run.bold();
    }
    if fmt.italic {
        run = run.italic();
    }
    if fmt.strike {
        run = run.strike();
    }
    if fmt.code {
        run = run.fonts(RunFonts::new().ascii(MONO).hi_ansi(MONO));
    }
    run
}

fn level(id: usize, format: &str, text: &str, start: usize) -> Level {
    Level::new(
        id,
        Start::new(start),
        NumberFormat::new(format),
        LevelText::new(text),
        LevelJc::new("left"),
    )
    .indent(
        Some((id as i32 + 1) * 720),
        Some(SpecialIndentType::Hanging(360)),
        None,
        None,
    )
}

fn bullet_abstract() -> AbstractNumbering {
    let mut a = AbstractNumbering::new(BULLET_NUM);
    for i in 0..9 {
        a = a.add_level(level(i, "bullet", "\u{2022}", 1));
    }
    a
}

/// A decimal ordered-list numbering whose top level starts at `start`.
fn ordered_abstract(id: usize, start: usize) -> AbstractNumbering {
    let mut a = AbstractNumbering::new(id);
    for i in 0..9 {
        let s = if i == 0 { start } else { 1 };
        a = a.add_level(level(i, "decimal", &format!("%{}.", i + 1), s));
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_docx() {
        let md = "# Title\n\nHello **bold** and *italic*.\n\n- a\n- b\n  - nested\n\n1. one\n2. two\n\n| H1 | H2 |\n|----|----|\n| a  | b  |\n\n> quote\n\n```\ncode\n```\n\nFootnote.[^1]\n\n[^1]: note text\n";
        let dir = std::env::temp_dir().join("ynote_docx_smoke");
        std::fs::create_dir_all(&dir).unwrap();
        let project = Project::open(&dir).unwrap();
        let bytes = render_docx(&project, Path::new("doc.md"), md).expect("docx");
        // .docx is a zip; check the PK magic and non-trivial size.
        assert!(bytes.len() > 500, "docx too small: {}", bytes.len());
        assert_eq!(&bytes[0..2], b"PK");
    }
}
