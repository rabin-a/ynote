//! Heading slug generation, matching GitHub's algorithm, with per-document
//! de-duplication (`title`, `title-1`, `title-2`, ...).
//!
//! GitHub's rule: lowercase, remove characters that are not letters, digits,
//! spaces, or hyphens (Unicode letters/digits are kept), then replace runs of
//! spaces with single hyphens. Existing hyphens are preserved.

use std::collections::HashMap;

/// Convert a single heading's plain text to a base slug (no de-duplication).
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            out.push('-');
        } else if ch == '_' {
            out.push('_');
        }
        // everything else (punctuation, symbols) is dropped
    }
    out
}

/// Tracks slugs seen so far in a document and appends `-1`, `-2`, ... on collision.
#[derive(Debug, Default)]
pub struct SlugMaker {
    seen: HashMap<String, u32>,
}

impl SlugMaker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Produce a unique slug for `text` within this document.
    pub fn make(&mut self, text: &str) -> String {
        let base = slugify(text);
        match self.seen.get_mut(&base) {
            None => {
                self.seen.insert(base.clone(), 0);
                base
            }
            Some(count) => {
                *count += 1;
                format!("{base}-{count}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("C++ & Rust"), "c--rust");
        assert_eq!(slugify("Über Café"), "über-café");
    }

    #[test]
    fn dedup() {
        let mut m = SlugMaker::new();
        assert_eq!(m.make("Intro"), "intro");
        assert_eq!(m.make("Intro"), "intro-1");
        assert_eq!(m.make("Intro"), "intro-2");
    }
}
