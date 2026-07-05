//! Export dispatch: the one entry point adapters call to turn a document into
//! HTML/PDF/DOCX bytes or write it to a file. Keeps `cli`, `mcp`, and `app`
//! free of any format logic.
//!
//! Two write entry points with different trust models:
//! - [`export`] — CLI/app facing. `out` may be absolute or cwd-relative, so a
//!   user can export anywhere on their disk.
//! - [`export_confined`] — agent/MCP facing. `out` is always project-relative
//!   and confined to the project root via the path-safety check. No MCP tool
//!   may write outside the root.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::project::Project;
use crate::render_html::{render_html, RenderOptions};
use crate::Format;

/// Render a document to standalone, self-contained HTML using project config.
pub fn html_standalone(project: &Project, rel: &Path, source: &str) -> Result<String> {
    let abs = project.resolve_path(rel)?;
    let base_dir = abs.parent().map(|p| p.to_path_buf());
    let opts = RenderOptions::standalone()
        .with_config(&project.config().render)
        .with_base_dir(base_dir)
        .with_root(Some(project.root().to_path_buf()));
    render_html(source, &opts)
}

/// Render a document to a fragment for live preview (no embedded CSS/base64).
pub fn html_preview(project: &Project, rel: &Path, source: &str) -> Result<String> {
    let mut opts = RenderOptions::preview()
        .with_config(&project.config().render)
        .with_root(Some(project.root().to_path_buf()))
        .with_preview_edit(true);
    // Inline local images as base64 so they render inside the webview without
    // needing an asset-protocol grant; containment is enforced by `with_root`.
    opts.inline_images = true;
    if let Ok(abs) = project.resolve_path(rel) {
        opts.base_dir = abs.parent().map(|p| p.to_path_buf());
    }
    render_html(source, &opts)
}

/// Produce the raw bytes for a given format (TOC per project config).
pub fn to_bytes(project: &Project, rel: &Path, source: &str, format: Format) -> Result<Vec<u8>> {
    to_bytes_with(project, rel, source, format, None)
}

/// Produce the raw bytes for a given format, optionally overriding the PDF
/// table-of-contents setting (`None` = use `ynote.toml`). Ignored for
/// non-PDF formats.
pub fn to_bytes_with(
    project: &Project,
    rel: &Path,
    source: &str,
    format: Format,
    pdf_toc: Option<bool>,
) -> Result<Vec<u8>> {
    match format {
        Format::Html => Ok(html_standalone(project, rel, source)?.into_bytes()),
        Format::Pdf => {
            #[cfg(feature = "pdf")]
            {
                crate::export_pdf::render_pdf_with(project, rel, source, pdf_toc)
            }
            #[cfg(not(feature = "pdf"))]
            {
                let _ = (project, rel, source, pdf_toc);
                Err(Error::UnsupportedFormat(
                    "pdf (built without pdf feature)".into(),
                ))
            }
        }
        Format::Docx => {
            #[cfg(feature = "docx")]
            {
                crate::export_docx::render_docx(project, rel, source)
            }
            #[cfg(not(feature = "docx"))]
            {
                let _ = (project, rel, source);
                Err(Error::UnsupportedFormat(
                    "docx (built without docx feature)".into(),
                ))
            }
        }
    }
}

/// CLI/app export. `out` may be a file path or a directory (existing, or ending
/// in a separator) to derive the name in; empty derives the name in the cwd.
/// `out` is taken at face value — absolute and cwd-relative paths are allowed
/// so users can write anywhere. Agents must use [`export_confined`] instead.
pub fn export(project: &Project, rel: &Path, format: Format, out: &Path) -> Result<PathBuf> {
    export_with(project, rel, format, out, None)
}

/// Like [`export`], but with an optional PDF table-of-contents override for
/// this one export (`None` = use `ynote.toml`).
pub fn export_with(
    project: &Project,
    rel: &Path,
    format: Format,
    out: &Path,
    pdf_toc: Option<bool>,
) -> Result<PathBuf> {
    let source = project.read_document(rel)?;
    let bytes = to_bytes_with(project, rel, &source, format, pdf_toc)?;
    let out_path = resolve_out(rel, format, out);
    write_bytes(&out_path, &bytes)?;
    Ok(canonical_or_self(out_path))
}

/// Agent/MCP export. `out` is interpreted **project-relative** and must stay
/// inside the project root. Absolute paths and `..` escapes are rejected via
/// the project path-safety check. `None` writes `<stem>.<ext>` at the root.
pub fn export_confined(
    project: &Project,
    rel: &Path,
    format: Format,
    out: Option<&Path>,
) -> Result<PathBuf> {
    let source = project.read_document(rel)?;
    let bytes = to_bytes(project, rel, &source, format)?;

    let final_rel = match out {
        None => derived_name(rel, format),
        Some(o) => {
            // resolve_path rejects absolute paths and any `..` that escapes root.
            let abs = project.resolve_path(o)?;
            if abs.is_dir() {
                project.relativize(&abs).join(derived_name(rel, format))
            } else {
                project.relativize(&abs)
            }
        }
    };
    // Final containment check on the derived path (catches a dir + `..` name).
    let out_abs = project.resolve_path(&final_rel)?;
    write_bytes(&out_abs, &bytes)?;
    Ok(canonical_or_self(out_abs))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    std::fs::write(path, bytes).map_err(|e| Error::io(path, e))
}

fn canonical_or_self(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn derived_name(rel: &Path, format: Format) -> PathBuf {
    let stem = rel.file_stem().unwrap_or_default();
    let mut name = PathBuf::from(stem);
    name.set_extension(format.extension());
    name
}

/// Work out the final output file path for [`export`]. Treats `out` as a
/// directory only when it actually exists as one or ends in a path separator —
/// an extensionless *filename* (e.g. `README`) is used verbatim.
fn resolve_out(rel: &Path, format: Format, out: &Path) -> PathBuf {
    if out.as_os_str().is_empty() {
        derived_name(rel, format)
    } else if out.is_dir() || ends_with_separator(out) {
        out.join(derived_name(rel, format))
    } else {
        out.to_path_buf()
    }
}

fn ends_with_separator(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.ends_with('/') || s.ends_with(std::path::MAIN_SEPARATOR)
}
