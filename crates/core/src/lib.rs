//! ynote-core — all of ynote's logic.
//!
//! One markdown parser (comrak) feeds one set of exporters. The preview and
//! the HTML export come from the same [`render_html`] function, so what you
//! see is what you ship. `cli`, `mcp`, and `app` are thin adapters over this
//! crate and contain no rendering logic of their own.

pub mod assets;
pub mod config;
pub mod error;
pub mod outline;
pub mod parse;
pub mod project;
pub mod render_html;
pub mod slug;

#[cfg(feature = "pdf")]
pub mod export_pdf;

#[cfg(feature = "docx")]
pub mod export_docx;

pub mod check;
pub mod export;
pub mod section;

pub use config::Config;
pub use error::{Error, Result};
pub use outline::{outline, Heading};
pub use project::Project;
pub use render_html::{render_html, RenderOptions};

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Pdf,
    Docx,
}

impl Format {
    pub fn from_str_ci(s: &str) -> Result<Format> {
        match s.to_ascii_lowercase().as_str() {
            "html" | "htm" => Ok(Format::Html),
            "pdf" => Ok(Format::Pdf),
            "docx" | "doc" | "word" => Ok(Format::Docx),
            other => Err(Error::UnsupportedFormat(other.to_string())),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Docx => "docx",
        }
    }
}
