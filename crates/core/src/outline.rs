//! Document outline: the heading tree used by the UI sidebar and the MCP
//! `get_outline` tool. Slugs are produced by the same [`crate::slug::SlugMaker`]
//! the HTML renderer uses, so outline slugs and in-document anchor links match.

use comrak::nodes::NodeValue;
use comrak::Arena;
use serde::Serialize;

use crate::parse;
use crate::slug::SlugMaker;

/// One heading in a document.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Heading {
    /// Heading level, 1–6.
    pub level: u8,
    /// Rendered plain text of the heading.
    pub text: String,
    /// Stable slug/anchor id (GitHub algorithm, de-duplicated).
    pub slug: String,
    /// 1-based source line where the heading starts.
    pub line: usize,
}

/// Extract the heading tree (flat, in document order) from markdown source.
pub fn outline(source: &str) -> Vec<Heading> {
    let arena = Arena::new();
    let (root, _fm) = parse::parse(&arena, source);
    let mut slugs = SlugMaker::new();
    let mut headings = Vec::new();

    for node in root.descendants() {
        // Copy out the Copy fields under a short-lived borrow, then drop it
        // before re-borrowing descendants for the text (comrak RefCell rule).
        let info = {
            let d = node.data.borrow();
            if let NodeValue::Heading(h) = &d.value {
                Some((h.level, d.sourcepos.start.line))
            } else {
                None
            }
        };
        if let Some((level, line)) = info {
            let text = parse::node_text(node);
            let slug = slugs.make(&text);
            headings.push(Heading {
                level,
                text,
                slug,
                line,
            });
        }
    }
    headings
}

/// Render an outline as an indented plain-text tree.
pub fn outline_text(headings: &[Heading]) -> String {
    let mut out = String::new();
    for h in headings {
        let indent = "  ".repeat(h.level.saturating_sub(1) as usize);
        out.push_str(&format!(
            "{indent}{} {}\n",
            "#".repeat(h.level as usize),
            h.text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_headings_with_lines_and_slugs() {
        let md = "# Title\n\nintro\n\n## Section A\n\ntext\n\n## Section A\n";
        let o = outline(md);
        assert_eq!(o.len(), 3);
        assert_eq!(o[0].level, 1);
        assert_eq!(o[0].slug, "title");
        assert_eq!(o[0].line, 1);
        assert_eq!(o[1].slug, "section-a");
        assert_eq!(o[2].slug, "section-a-1"); // de-duplicated
        assert_eq!(o[2].line, 9);
    }
}
