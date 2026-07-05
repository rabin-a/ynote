//! Markdown parsing (comrak) and the small set of AST helpers shared by every
//! exporter. Parsing happens once per render/export call; exporters walk the
//! returned AST rather than re-parsing strings.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};

/// The comrak arena that owns a parsed document's nodes.
pub type Ast<'a> = &'a AstNode<'a>;

/// Build the comrak options papery uses everywhere: full GFM, front matter,
/// and math. Using one function keeps preview and every export byte-consistent.
pub fn options() -> Options<'static> {
    let mut o = Options::default();
    // GFM
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.footnotes = true;
    o.extension.superscript = true;
    // Front matter: `---` fenced YAML at the top of the file.
    o.extension.front_matter_delimiter = Some("---".to_string());
    // Math: `$inline$`, `$$display$$`, and `` $`code math`$ ``.
    o.extension.math_dollars = true;
    o.extension.math_code = true;
    // Parsing niceties.
    o.parse.smart = false;
    o
}

/// Parse `source` into `arena`, returning the document root and any front matter.
///
/// The caller owns the arena so the returned nodes' lifetime is tied to it:
/// ```ignore
/// let arena = Arena::new();
/// let (root, fm) = parse::parse(&arena, source);
/// ```
pub fn parse<'a>(arena: &'a Arena<'a>, source: &str) -> (Ast<'a>, Option<FrontMatter>) {
    let opts = options();
    let root = parse_document(arena, source, &opts);
    let front_matter = extract_front_matter(root);
    (root, front_matter)
}

/// YAML front matter parsed from the top of a document.
#[derive(Debug, Clone)]
pub struct FrontMatter {
    pub raw: String,
    pub value: serde_yaml::Value,
}

impl FrontMatter {
    fn string_field(&self, key: &str) -> Option<String> {
        match self.value.get(key)? {
            serde_yaml::Value::String(s) => Some(s.clone()),
            // Coerce scalars like a bare date/number into text for templates.
            other => match other {
                serde_yaml::Value::Null => None,
                v => serde_yaml::to_string(v).ok().map(|s| s.trim().to_string()),
            },
        }
    }

    pub fn title(&self) -> Option<String> {
        self.string_field("title")
    }
    pub fn author(&self) -> Option<String> {
        self.string_field("author")
    }
    pub fn date(&self) -> Option<String> {
        self.string_field("date")
    }
}

/// The front matter node is the first child of the document root; its string
/// still carries the `---` fences, which we trim before parsing YAML.
fn extract_front_matter<'a>(root: Ast<'a>) -> Option<FrontMatter> {
    for child in root.children() {
        if let NodeValue::FrontMatter(raw) = &child.data.borrow().value {
            let inner = raw
                .trim()
                .trim_start_matches("---")
                .trim_end_matches("---")
                .trim();
            let value = serde_yaml::from_str(inner).unwrap_or(serde_yaml::Value::Null);
            return Some(FrontMatter {
                raw: raw.clone(),
                value,
            });
        }
    }
    None
}

/// Concatenate the plain-text content of a node's subtree (used for heading
/// text and slug generation). Ignores formatting, keeps inline-code literals.
pub fn node_text<'a>(node: Ast<'a>) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out
}

fn collect_text<'a>(node: Ast<'a>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(t) => out.push_str(t),
        NodeValue::Code(c) => out.push_str(&c.literal),
        NodeValue::Math(m) => out.push_str(&m.literal),
        NodeValue::LineBreak | NodeValue::SoftBreak => out.push(' '),
        _ => {}
    }
    for child in node.children() {
        collect_text(child, out);
    }
}

/// The document title straight from raw markdown (front matter `title`, else
/// the first H1). Convenience wrapper that manages the arena internally.
pub fn title_of(source: &str) -> Option<String> {
    let arena = Arena::new();
    let (root, fm) = parse(&arena, source);
    document_title(root, fm.as_ref())
}

/// The document title: front matter `title`, else the first level-1 heading's text.
pub fn document_title<'a>(root: Ast<'a>, fm: Option<&FrontMatter>) -> Option<String> {
    if let Some(t) = fm.and_then(|f| f.title()) {
        return Some(t);
    }
    for node in root.descendants() {
        let is_h1 = matches!(&node.data.borrow().value, NodeValue::Heading(h) if h.level == 1);
        if is_h1 {
            return Some(node_text(node));
        }
    }
    None
}
