//! Project model: a project is just a folder of markdown files.
//!
//! No database, no hidden state. [`Project::open`] validates a directory,
//! loads `papery.toml` if present, and applies defaults otherwise.
//!
//! ## Path safety (hard security requirement)
//!
//! The MCP server exposes read/write operations to agents. Every path an
//! agent supplies is resolved through [`Project::resolve_path`], which
//! **rejects any path that escapes the project root** via both a logical
//! `..`-collapse check and a canonicalized-prefix check (to defeat symlinks).

use std::path::{Component, Path, PathBuf};

use crate::config::Config;
use crate::error::{Error, Result};

/// The config file name that marks a project root.
pub const CONFIG_FILE: &str = "papery.toml";

/// An opened project rooted at a directory.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    canonical_root: PathBuf,
    config: Config,
    /// Non-fatal warnings gathered while loading config (unknown keys, etc.).
    warnings: Vec<String>,
}

impl Project {
    /// Open the project rooted at `root`.
    ///
    /// `root` must be an existing directory. If it contains `papery.toml`,
    /// that config is loaded; otherwise all defaults apply.
    pub fn open(root: impl AsRef<Path>) -> Result<Project> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(Error::NotADirectory(root.to_path_buf()));
        }
        let canonical_root = root.canonicalize().map_err(|e| Error::io(root, e))?;

        let config_path = canonical_root.join(CONFIG_FILE);
        let (config, warnings) = if config_path.is_file() {
            let text =
                std::fs::read_to_string(&config_path).map_err(|e| Error::io(&config_path, e))?;
            Config::parse(&text)?
        } else {
            (Config::default(), Vec::new())
        };

        Ok(Project {
            root: root.to_path_buf(),
            canonical_root,
            config,
            warnings,
        })
    }

    /// Open a project by walking up from `start` to find the nearest
    /// `papery.toml`. Falls back to `start` itself if none is found.
    pub fn discover(start: impl AsRef<Path>) -> Result<Project> {
        let start = start.as_ref();
        let mut dir: Option<&Path> = Some(start);
        while let Some(d) = dir {
            if d.join(CONFIG_FILE).is_file() {
                return Project::open(d);
            }
            dir = d.parent();
        }
        Project::open(start)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Resolve a project-relative path to an absolute path, guaranteeing it
    /// stays inside the project root.
    ///
    /// Rejects absolute inputs and any path that (logically or via symlink)
    /// escapes the root. Works for not-yet-existing files (e.g. `write_document`)
    /// by canonicalizing the deepest existing ancestor.
    pub fn resolve_path(&self, rel: impl AsRef<Path>) -> Result<PathBuf> {
        let rel = rel.as_ref();
        if rel.is_absolute() {
            return Err(Error::PathEscapesRoot(rel.to_path_buf()));
        }

        // 1. Logical normalization: collapse `.` / `..` without touching the FS.
        //    A `..` that would pop above the root is an immediate reject.
        let mut normalized = PathBuf::new();
        for comp in rel.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(Error::PathEscapesRoot(rel.to_path_buf()));
                    }
                }
                Component::Normal(seg) => normalized.push(seg),
                // Prefix / RootDir on a "relative" path -> reject.
                Component::Prefix(_) | Component::RootDir => {
                    return Err(Error::PathEscapesRoot(rel.to_path_buf()));
                }
            }
        }

        let joined = self.canonical_root.join(&normalized);

        // 2. Symlink defense: canonicalize the deepest existing ancestor and
        //    verify it is still within the canonical root.
        let existing = deepest_existing_ancestor(&joined);
        let canonical_existing = existing
            .canonicalize()
            .map_err(|e| Error::io(&existing, e))?;
        if !canonical_existing.starts_with(&self.canonical_root) {
            return Err(Error::PathEscapesRoot(rel.to_path_buf()));
        }

        Ok(joined)
    }

    /// Resolve an asset path (image, include) referenced from `doc_rel`'s own
    /// directory, keeping it inside the project root.
    pub fn resolve_asset(
        &self,
        doc_rel: impl AsRef<Path>,
        asset: impl AsRef<Path>,
    ) -> Result<PathBuf> {
        let asset = asset.as_ref();
        if asset.is_absolute() {
            return Err(Error::PathEscapesRoot(asset.to_path_buf()));
        }
        let doc_dir = doc_rel.as_ref().parent().unwrap_or_else(|| Path::new(""));
        self.resolve_path(doc_dir.join(asset))
    }

    /// Turn an absolute path inside the project into a root-relative path.
    pub fn relativize(&self, abs: impl AsRef<Path>) -> PathBuf {
        let abs = abs.as_ref();
        abs.strip_prefix(&self.canonical_root)
            .or_else(|_| abs.strip_prefix(&self.root))
            .unwrap_or(abs)
            .to_path_buf()
    }

    /// Enumerate all `*.md` documents under the root, respecting `.gitignore`
    /// and the config `exclude` globs. Returns sorted, root-relative paths.
    pub fn documents(&self) -> Result<Vec<PathBuf>> {
        use ignore::overrides::OverrideBuilder;
        use ignore::WalkBuilder;

        let mut overrides = OverrideBuilder::new(&self.canonical_root);
        for pat in &self.config.project.exclude {
            // In `ignore` override syntax, a `!`-prefixed glob is an *ignore*
            // pattern, which is exactly our "exclude" semantics.
            overrides
                .add(&format!("!{pat}"))
                .map_err(|e| Error::Config(format!("bad exclude glob `{pat}`: {e}")))?;
        }
        let overrides = overrides
            .build()
            .map_err(|e| Error::Config(e.to_string()))?;

        let mut walker = WalkBuilder::new(&self.canonical_root);
        walker
            .hidden(false) // include dotfiles that aren't gitignored
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .overrides(overrides);

        let mut docs = Vec::new();
        for entry in walker.build() {
            let entry = entry.map_err(|e| Error::Config(e.to_string()))?;
            let path = entry.path();
            if entry.file_type().is_some_and(|t| t.is_file())
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                docs.push(self.relativize(path));
            }
        }
        docs.sort();
        Ok(docs)
    }

    /// Read a document's raw text by root-relative path.
    pub fn read_document(&self, rel: impl AsRef<Path>) -> Result<String> {
        let abs = self.resolve_path(&rel)?;
        if !abs.is_file() {
            return Err(Error::DocumentNotFound(rel.as_ref().to_path_buf()));
        }
        std::fs::read_to_string(&abs).map_err(|e| Error::io(&abs, e))
    }

    /// Write a document by root-relative path, creating parent directories.
    pub fn write_document(&self, rel: impl AsRef<Path>, content: &str) -> Result<PathBuf> {
        let abs = self.resolve_path(&rel)?;
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::write(&abs, content).map_err(|e| Error::io(&abs, e))?;
        Ok(abs)
    }
}

/// Return the deepest ancestor of `path` (including `path`) that exists on disk.
/// Used so we can canonicalize for symlink checks even when the leaf file does
/// not exist yet (writes create it).
fn deepest_existing_ancestor(path: &Path) -> PathBuf {
    let mut p = path;
    loop {
        if p.exists() {
            return p.to_path_buf();
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return p.to_path_buf(),
        }
    }
}
