//! Assets embedded into the binary so the tool is a single self-contained
//! file: HTML themes and the default Typst template. Bundling (rather than
//! reading from disk at runtime) is what keeps "zero runtime deps" honest.

/// The default HTML preview/export theme.
pub const DEFAULT_THEME_CSS: &str = include_str!("../../../assets/themes/default.css");

/// The default Typst PDF template (the `papery-doc` function).
pub const DEFAULT_TYPST_TEMPLATE: &str = include_str!("../../../assets/typst/default.typ");

/// Return the default theme CSS.
pub fn default_theme_css() -> &'static str {
    DEFAULT_THEME_CSS
}

/// Resolve a theme name to its CSS. Unknown names fall back to the default.
pub fn theme_css(name: &str) -> &'static str {
    match name {
        "default" => DEFAULT_THEME_CSS,
        _ => DEFAULT_THEME_CSS,
    }
}

/// Resolve a Typst template name to its markup. Unknown names fall back.
pub fn typst_template(name: &str) -> &'static str {
    match name {
        "default" => DEFAULT_TYPST_TEMPLATE,
        _ => DEFAULT_TYPST_TEMPLATE,
    }
}

/// `<head>` markup that makes standalone HTML math self-rendering.
///
/// v1 gap: KaTeX's JS/CSS/font assets are not yet bundled, so standalone HTML
/// currently emits math with `\(...\)` / `\[...\]` delimiters (readable, and
/// picked up automatically once the KaTeX assets are embedded here). The
/// desktop preview webview loads KaTeX itself. Returns an empty string until
/// the assets land — never a network `<script src>` (that would break the
/// self-contained guarantee).
pub fn katex_head() -> String {
    String::new()
}
