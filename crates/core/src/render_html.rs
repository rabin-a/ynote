//! The one HTML renderer. Preview (fragment) and export (standalone) both go
//! through [`render_html`]; the only difference is the wrapper. Markdown is
//! never rendered in JavaScript — this function is the single source of truth.

use std::path::PathBuf;
use std::sync::LazyLock;

use base64::Engine;
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::Arena;
use syntect::highlighting::ThemeSet;
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::error::Result;
use crate::parse;
use crate::slug::SlugMaker;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Options controlling a single render.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Emit a full standalone HTML document (embedded CSS) vs a body fragment.
    pub standalone: bool,
    /// Base64-embed local images (default for standalone export).
    pub inline_images: bool,
    /// Render `$...$` / `$$...$$` as KaTeX-delimited math (else literal text).
    pub math: bool,
    /// syntect theme name for code highlighting (`github`, `base16-ocean.dark`, ...).
    pub syntax_theme: String,
    /// Extra theme CSS embedded into standalone output (page/typography theme).
    pub theme_css: Option<String>,
    /// Directory that relative image paths resolve against (the document's dir).
    pub base_dir: Option<PathBuf>,
    /// Project root; local images are only inlined when they resolve inside it
    /// (path-safety for base64 inlining). `None` disables the containment check.
    pub root: Option<PathBuf>,
    /// Document title for `<title>` (falls back to front matter / first H1).
    pub title: Option<String>,
    /// Editing mode for in-preview WYSIWYG: keeps each image's original URL in
    /// `data-osrc` (alongside the base64 `src`) so the editor serializes the
    /// original path back to markdown instead of the inlined data URI.
    pub preview_edit: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            standalone: false,
            inline_images: false,
            math: true,
            syntax_theme: "github".to_string(),
            theme_css: None,
            base_dir: None,
            root: None,
            title: None,
            preview_edit: false,
        }
    }
}

impl RenderOptions {
    /// Fragment for the live preview webview (no embedded CSS, no base64).
    pub fn preview() -> Self {
        RenderOptions {
            standalone: false,
            inline_images: false,
            ..Default::default()
        }
    }

    /// Standalone, self-contained HTML for export (embedded CSS + base64 images).
    pub fn standalone() -> Self {
        RenderOptions {
            standalone: true,
            inline_images: true,
            theme_css: Some(crate::assets::default_theme_css().to_string()),
            ..Default::default()
        }
    }

    /// Set the directory relative image paths resolve against.
    pub fn with_base_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.base_dir = dir;
        self
    }

    /// Set the project root used to confine base64 image inlining.
    pub fn with_root(mut self, root: Option<PathBuf>) -> Self {
        self.root = root;
        self
    }

    /// Enable in-preview editing (blocks tagged with source byte ranges).
    pub fn with_preview_edit(mut self, on: bool) -> Self {
        self.preview_edit = on;
        self
    }

    /// Apply the `[render]` section of a project config.
    pub fn with_config(mut self, cfg: &crate::config::RenderConfig) -> Self {
        self.math = cfg.math;
        self.syntax_theme = cfg.syntax_theme.clone();
        self.theme_css = Some(crate::assets::theme_css(&cfg.theme).to_string());
        self
    }
}

/// Render markdown `source` to HTML per `opts`.
pub fn render_html(source: &str, opts: &RenderOptions) -> Result<String> {
    let arena = Arena::new();
    let (root, fm) = parse::parse(&arena, source);
    let title = opts
        .title
        .clone()
        .or_else(|| parse::document_title(root, fm.as_ref()));

    let mut w = Writer::new(opts);
    w.index_footnotes(root);
    w.children(root);
    let body = w.out;

    if opts.standalone {
        Ok(standalone_document(&body, title.as_deref(), opts))
    } else {
        Ok(format!("<div class=\"ynote\">\n{body}</div>\n"))
    }
}

/// syntect CSS for a theme name (class-based). Exposed so the desktop app can
/// inject the same code styling into its preview webview.
pub fn syntect_css(theme_name: &str) -> String {
    let theme = theme_by_name(theme_name);
    css_for_theme_with_class_style(theme, ClassStyle::Spaced).unwrap_or_default()
}

fn theme_by_name(name: &str) -> &'static syntect::highlighting::Theme {
    // There is no bundled theme literally named "github"; map it to InspiredGitHub.
    let key = match name {
        "github" | "InspiredGitHub" => "InspiredGitHub",
        other => other,
    };
    THEME_SET
        .themes
        .get(key)
        .or_else(|| THEME_SET.themes.get("InspiredGitHub"))
        .expect("default syntect theme present")
}

struct Writer<'o> {
    out: String,
    opts: &'o RenderOptions,
    slugs: SlugMaker,
    /// Whether we are inside a tight list (paragraphs render without `<p>`).
    tight: bool,
    /// footnote name -> sequential number (from first reference), so the
    /// definition marker matches its reference marker.
    footnotes: std::collections::HashMap<String, u32>,
}

impl<'o> Writer<'o> {
    fn new(opts: &'o RenderOptions) -> Self {
        Writer {
            out: String::new(),
            opts,
            slugs: SlugMaker::new(),
            tight: false,
            footnotes: std::collections::HashMap::new(),
        }
    }

    fn children<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.node(child);
        }
    }

    fn node<'a>(&mut self, node: &'a AstNode<'a>) {
        // Clone the value out of the borrow so we can recurse into children
        // without holding the RefCell borrow.
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Document => self.children(node),
            NodeValue::FrontMatter(_) => {} // stripped from output

            NodeValue::Heading(h) => {
                let text = parse::node_text(node);
                let slug = self.slugs.make(&text);
                let lvl = h.level.clamp(1, 6);
                self.out.push_str(&format!(
                    "<h{lvl} id=\"{id}\"><a class=\"anchor\" href=\"#{id}\" aria-hidden=\"true\">#</a>",
                    id = esc_attr(&slug)
                ));
                self.children(node);
                self.out.push_str(&format!("</h{lvl}>\n"));
            }

            NodeValue::Paragraph => {
                if self.tight {
                    self.children(node);
                } else {
                    self.out.push_str("<p>");
                    self.children(node);
                    self.out.push_str("</p>\n");
                }
            }

            NodeValue::BlockQuote => {
                // A blockquote is a fresh block context; don't inherit the
                // enclosing list's tightness (paragraphs inside must get <p>).
                let prev_tight = self.tight;
                self.tight = false;
                self.out.push_str("<blockquote>\n");
                self.children(node);
                self.out.push_str("</blockquote>\n");
                self.tight = prev_tight;
            }

            NodeValue::List(list) => {
                let (tag, extra) = match list.list_type {
                    ListType::Bullet => ("ul", String::new()),
                    ListType::Ordered => (
                        "ol",
                        if list.start != 1 {
                            format!(" start=\"{}\"", list.start)
                        } else {
                            String::new()
                        },
                    ),
                };
                let prev_tight = self.tight;
                self.tight = list.tight;
                self.out.push_str(&format!("<{tag}{extra}>\n"));
                self.children(node);
                self.out.push_str(&format!("</{tag}>\n"));
                self.tight = prev_tight;
            }

            NodeValue::Item(_) => {
                self.out.push_str("<li>");
                self.children(node);
                self.out.push_str("</li>\n");
            }

            NodeValue::TaskItem(task) => {
                let checked = task.symbol.is_some();
                self.out.push_str("<li class=\"task-list-item\">");
                self.out.push_str(&format!(
                    "<input type=\"checkbox\" disabled{}> ",
                    if checked { " checked" } else { "" }
                ));
                self.children(node);
                self.out.push_str("</li>\n");
            }

            NodeValue::CodeBlock(cb) => {
                let lang = cb.info.split_whitespace().next().unwrap_or("");
                self.out
                    .push_str(&highlight_block(&cb.literal, lang, &self.opts.syntax_theme));
            }

            NodeValue::HtmlBlock(html) => {
                // Raw HTML is escaped by default (comrak's unsafe_=false stance):
                // the renderer feeds untrusted/agent-authored content into the
                // preview webview via innerHTML and into self-contained exports,
                // so passing raw `<script>`/handlers through would be an XSS hole.
                self.out.push_str(&format!(
                    "<pre class=\"raw-html\">{}</pre>\n",
                    esc_text(&html.literal)
                ));
            }

            NodeValue::ThematicBreak => self.out.push_str("<hr />\n"),

            NodeValue::Table(table) => self.table(node, &table.alignments),

            NodeValue::FootnoteDefinition(def) => {
                // Show the sequential number that matches the reference marker,
                // not the raw label. Falls back to the label if never referenced.
                let marker = self
                    .footnotes
                    .get(&def.name)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| def.name.clone());
                let prev_tight = self.tight;
                self.tight = false;
                self.out.push_str(&format!(
                    "<div class=\"footnote-definition\" id=\"fn-{}\"><sup>{}</sup> ",
                    esc_attr(&def.name),
                    esc_text(&marker)
                ));
                self.children(node);
                self.out.push_str("</div>\n");
                self.tight = prev_tight;
            }

            // ---- inline ----
            NodeValue::Text(t) => self.out.push_str(&esc_text(&t)),
            NodeValue::SoftBreak => self.out.push('\n'),
            NodeValue::LineBreak => self.out.push_str("<br />\n"),
            NodeValue::Emph => self.wrap("em", node),
            NodeValue::Strong => self.wrap("strong", node),
            NodeValue::Strikethrough => self.wrap("del", node),
            NodeValue::Superscript => self.wrap("sup", node),
            NodeValue::Code(c) => {
                self.out
                    .push_str(&format!("<code>{}</code>", esc_text(&c.literal)));
            }
            // Raw inline HTML is escaped for the same safety reason as blocks.
            NodeValue::HtmlInline(html) => self.out.push_str(&esc_text(&html)),

            NodeValue::Link(link) => {
                self.out.push_str(&format!(
                    "<a href=\"{}\"{}>",
                    esc_attr(&sanitize_href(&link.url)),
                    title_attr(&link.title)
                ));
                self.children(node);
                self.out.push_str("</a>");
            }

            NodeValue::Image(link) => {
                let alt = parse::node_text(node);
                // In edit mode, keep the original (relative) URL so the WYSIWYG
                // serializer restores it instead of the base64 src.
                let osrc = if self.opts.preview_edit {
                    format!(" data-osrc=\"{}\"", esc_attr(&link.url))
                } else {
                    String::new()
                };
                self.out.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"{}{} />",
                    esc_attr(&self.image_src(&link.url)),
                    esc_attr(&alt),
                    title_attr(&link.title),
                    osrc
                ));
            }

            NodeValue::FootnoteReference(fr) => {
                self.out.push_str(&format!(
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{}\">{}</a></sup>",
                    esc_attr(&fr.name),
                    fr.ix
                ));
            }

            NodeValue::Math(m) => self.out.push_str(&self.math(&m.literal, m.display_math)),

            // Anything else: render children so no content is silently dropped.
            _ => self.children(node),
        }
    }

    fn wrap<'a>(&mut self, tag: &str, node: &'a AstNode<'a>) {
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push('>');
        self.children(node);
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }

    fn table<'a>(&mut self, table: &'a AstNode<'a>, aligns: &[TableAlignment]) {
        self.out.push_str("<table>\n");
        let mut first_row = true;
        for row in table.children() {
            let is_header = matches!(&row.data.borrow().value, NodeValue::TableRow(true));
            if first_row {
                self.out
                    .push_str(if is_header { "<thead>\n" } else { "<tbody>\n" });
            }
            self.out.push_str("<tr>\n");
            let cell_tag = if is_header { "th" } else { "td" };
            for (col, cell) in row.children().enumerate() {
                let style = match aligns.get(col) {
                    Some(TableAlignment::Left) => " style=\"text-align:left\"",
                    Some(TableAlignment::Center) => " style=\"text-align:center\"",
                    Some(TableAlignment::Right) => " style=\"text-align:right\"",
                    _ => "",
                };
                self.out.push_str(&format!("<{cell_tag}{style}>"));
                self.children(cell);
                self.out.push_str(&format!("</{cell_tag}>\n"));
            }
            self.out.push_str("</tr>\n");
            if first_row {
                self.out
                    .push_str(if is_header { "</thead>\n<tbody>\n" } else { "" });
                first_row = false;
            }
        }
        self.out.push_str("</tbody>\n</table>\n");
    }

    fn math(&self, tex: &str, display: bool) -> String {
        if !self.opts.math {
            return format!("<code class=\"math\">{}</code>", esc_text(tex));
        }
        // KaTeX auto-render picks up these delimiters from the DOM text nodes.
        if display {
            format!(
                "<span class=\"math math-display\">\\[{}\\]</span>",
                esc_text(tex)
            )
        } else {
            format!(
                "<span class=\"math math-inline\">\\({}\\)</span>",
                esc_text(tex)
            )
        }
    }

    /// Pre-pass: assign each footnote its number from the first reference, so
    /// definition markers match reference markers.
    fn index_footnotes<'a>(&mut self, root: &'a AstNode<'a>) {
        for node in root.descendants() {
            if let NodeValue::FootnoteReference(fr) = &node.data.borrow().value {
                self.footnotes.entry(fr.name.clone()).or_insert(fr.ix);
            }
        }
    }

    /// Resolve an image URL, base64-inlining local files when configured.
    ///
    /// Security: dangerous schemes are dropped, and local files are only
    /// inlined when they resolve *inside the project root* — otherwise the base
    /// join could read arbitrary files (`../../../etc/passwd`).
    fn image_src(&self, url: &str) -> String {
        // Reject script-y schemes even in <img src>.
        if is_dangerous_scheme(url) {
            return String::new();
        }
        if !self.opts.inline_images || is_remote(url) || url.starts_with("data:") {
            return url.to_string();
        }
        let Some(base) = &self.opts.base_dir else {
            return url.to_string();
        };
        let path = base.join(url);
        // Containment: the canonicalized target must stay within the project
        // root (when known). Escapes fall back to the original URL.
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => return url.to_string(),
        };
        if let Some(root) = &self.opts.root {
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
            if !canonical.starts_with(&root_canonical) {
                return url.to_string();
            }
        }
        match std::fs::read(&canonical) {
            Ok(bytes) => {
                let mime = mime_for(&canonical);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                format!("data:{mime};base64,{b64}")
            }
            // Missing/unreadable image: keep the original path, don't fail render.
            Err(_) => url.to_string(),
        }
    }
}

fn highlight_block(code: &str, lang: &str, theme_name: &str) -> String {
    let ss = &*SYNTAX_SET;
    let syntax = ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut gen = ClassedHTMLGenerator::new_with_class_style(syntax, ss, ClassStyle::Spaced);
    let mut ok = true;
    for line in LinesWithEndings::from(code) {
        if gen
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            ok = false;
            break;
        }
    }
    let lang_class = if lang.is_empty() {
        String::new()
    } else {
        format!(" class=\"language-{}\"", esc_attr(lang))
    };
    if ok {
        let inner = gen.finalize();
        // theme_name selects the CSS the app/standalone doc must include.
        let _ = theme_name;
        format!("<pre class=\"mdcode\"><code{lang_class}>{inner}</code></pre>\n")
    } else {
        format!(
            "<pre class=\"mdcode\"><code{lang_class}>{}</code></pre>\n",
            esc_text(code)
        )
    }
}

/// Wrap a rendered body in a full, self-contained HTML document.
fn standalone_document(body: &str, title: Option<&str>, opts: &RenderOptions) -> String {
    let theme_css = opts
        .theme_css
        .clone()
        .unwrap_or_else(|| crate::assets::default_theme_css().to_string());
    let code_css = syntect_css(&opts.syntax_theme);
    let title = title.unwrap_or("Document");
    let katex = if opts.math {
        crate::assets::katex_head()
    } else {
        String::new()
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\
<title>{title}</title>\n<style>\n{theme_css}\n{code_css}\n</style>\n{katex}</head>\n\
<body>\n<article class=\"ynote\">\n{body}</article>\n</body>\n</html>\n",
        title = esc_text(title),
    )
}

fn title_attr(title: &str) -> String {
    if title.is_empty() {
        String::new()
    } else {
        format!(" title=\"{}\"", esc_attr(title))
    }
}

fn is_remote(url: &str) -> bool {
    let u = url.trim_start();
    u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("//")
        || u.starts_with("mailto:")
}

/// The scheme prefix of a URL, lowercased, with whitespace/control chars
/// stripped first (browsers ignore those before parsing the scheme).
fn cleaned_lower(url: &str) -> String {
    url.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Script-executing schemes — dangerous in any context (links and `<img src>`).
fn is_dangerous_scheme(url: &str) -> bool {
    let l = cleaned_lower(url);
    l.starts_with("javascript:") || l.starts_with("vbscript:")
}

/// Sanitize an `<a href>`: neutralize script schemes and `data:` (a
/// `data:text/html` link is an XSS vector) to a harmless anchor. http/https/
/// mailto/tel and relative/anchor links are left untouched.
fn sanitize_href(url: &str) -> String {
    if is_dangerous_scheme(url) || cleaned_lower(url).starts_with("data:") {
        "#".to_string()
    } else {
        url.to_string()
    }
}

fn mime_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

fn esc_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn esc_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
