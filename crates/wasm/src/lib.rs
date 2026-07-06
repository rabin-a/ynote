//! ynote-wasm — the browser adapter over `ynote-core`.
//!
//! Same rule as `cli`/`mcp`/`app`: this crate contains **no rendering logic**.
//! It is a thin `wasm-bindgen` surface that hands markdown strings to the one
//! renderer in core and returns HTML/JSON strings. Storage (opening a folder,
//! reading and writing files) lives in JavaScript — the `StorageProvider`
//! seam — so this WASM module never touches the filesystem or the network.

use wasm_bindgen::prelude::*;
use ynote_core::render_html::{render_html, syntect_css, RenderOptions};
use ynote_core::{outline, parse};

const SYNTAX_THEME: &str = "github";

/// Install a panic hook so Rust panics show up as readable console errors.
/// Called once from JS at startup.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Render markdown to a `<div class="ynote">…</div>` fragment for live
/// preview. No embedded CSS — the page supplies it via [`preview_css`].
///
/// Local images are **not** inlined here: the browser holds files behind a
/// folder handle, not a path, so relative-image resolution is done in JS
/// (rewrite `src` to a blob URL) before/after this call. Phase-1 gap.
#[wasm_bindgen]
pub fn render_fragment(markdown: &str) -> Result<String, JsError> {
    let opts = RenderOptions {
        syntax_theme: SYNTAX_THEME.to_string(),
        ..RenderOptions::preview()
    };
    render_html(markdown, &opts).map_err(to_js)
}

/// Render markdown to a full standalone, self-contained HTML document
/// (embedded theme CSS). This is the HTML *export* — identical pipeline to the
/// preview, only wrapped. Suitable for writing back to the folder or download.
#[wasm_bindgen]
pub fn render_standalone(markdown: &str, title: Option<String>) -> Result<String, JsError> {
    let mut opts = RenderOptions {
        syntax_theme: SYNTAX_THEME.to_string(),
        ..RenderOptions::standalone()
    };
    opts.title = title;
    render_html(markdown, &opts).map_err(to_js)
}

/// The syntax-highlighting token colors for the preview, as class-based CSS.
///
/// Matches the desktop app: the web frontend supplies its own prose theme
/// (`style.css`), so from core we only need the syntect token colors. Injected
/// once into a `<style>` tag; stable for the session.
#[wasm_bindgen]
pub fn preview_css() -> String {
    syntect_css(SYNTAX_THEME)
}

/// The document outline as a JSON array of `{level, text, slug, line}` — feeds
/// the sidebar and click-to-jump. Slugs match the in-document anchor ids.
#[wasm_bindgen]
pub fn outline_json(markdown: &str) -> Result<String, JsError> {
    serde_json::to_string(&outline(markdown)).map_err(|e| JsError::new(&e.to_string()))
}

/// The document's display title (front-matter `title`, else first H1), for the
/// file list / tab label. Empty string when the document has no title.
#[wasm_bindgen]
pub fn doc_title(markdown: &str) -> String {
    parse::display_title(markdown).unwrap_or_default()
}

/// Full text of the section under `slug` (heading line included). Lets the UI
/// (and later the extension) target one section without rewriting the file.
#[wasm_bindgen]
pub fn extract_section(markdown: &str, slug: &str) -> Result<String, JsError> {
    ynote_core::section::extract_section(markdown, slug).map_err(to_js)
}

/// Replace the whole section under `slug` with `content` (which must include
/// the heading line). Returns the new full document text.
#[wasm_bindgen]
pub fn replace_section(markdown: &str, slug: &str, content: &str) -> Result<String, JsError> {
    ynote_core::section::replace_section(markdown, slug, content).map_err(to_js)
}

/// Render markdown straight to PDF bytes, compiled in-browser by Typst (WASM).
/// `toc` toggles the table of contents. Returns the raw `.pdf` bytes for the
/// page to download or write back to storage. Only present in the `pdf` build.
#[cfg(feature = "pdf")]
#[wasm_bindgen]
pub fn export_pdf(markdown: &str, toc: bool) -> Result<Vec<u8>, JsError> {
    let cfg = ynote_core::config::PdfConfig::default();
    ynote_core::export_pdf::render_pdf_from_source(markdown, &cfg, Some(toc)).map_err(to_js)
}

fn to_js(e: ynote_core::Error) -> JsError {
    JsError::new(&e.to_string())
}
