//! The single error type for ynote-core.
//!
//! Adapters map this onto their own surfaces:
//! - CLI  -> exit codes + stderr message
//! - MCP  -> tool error result with a human-readable message
//! - app  -> serialized error to the frontend
//!
//! Core never `panic!`s on user input; every fallible operation returns `Error`.

use std::path::PathBuf;

/// Result alias used throughout core.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    IoBare(#[from] std::io::Error),

    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("path escapes the project root: {0}")]
    PathEscapesRoot(PathBuf),

    #[error("document not found: {0}")]
    DocumentNotFound(PathBuf),

    #[error("heading not found: no section with slug `{0}`")]
    HeadingNotFound(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("invalid ynote.toml: {0}")]
    Config(String),

    #[error("invalid front matter: {0}")]
    FrontMatter(String),

    #[error("HTML render failed: {0}")]
    Render(String),

    #[error("PDF export failed: {0}")]
    Pdf(String),

    #[error("DOCX export failed: {0}")]
    Docx(String),

    #[error("unsupported export format: {0}")]
    UnsupportedFormat(String),
}

impl Error {
    /// Attach a path to a bare `std::io::Error` for a clearer message.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    /// The process exit code this error maps to in the CLI.
    /// 2 = usage, 3 = IO/render — matches the CLI contract in CLAUDE.md.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::UnsupportedFormat(_) | Error::PathEscapesRoot(_) => 2,
            _ => 3,
        }
    }
}
