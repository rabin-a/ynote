//! Section addressing by heading slug: read or replace the block of a document
//! that lives under a given heading. Powers the MCP `read_document`
//! (`heading_slug`) and `edit_section` tools so agents can target long
//! documents without rewriting the whole file.
//!
//! A "section" runs from its heading line up to (but not including) the next
//! heading of the same or higher level, or end of file.

use crate::error::{Error, Result};
use crate::outline::outline;

/// Byte range `[start, end)` of the section whose heading has `slug`.
fn section_bytes(source: &str, slug: &str) -> Option<(usize, usize)> {
    let headings = outline(source);
    let idx = headings.iter().position(|h| h.slug == slug)?;
    let target = &headings[idx];

    // End at the next heading with level <= target level.
    let end_line = headings[idx + 1..]
        .iter()
        .find(|h| h.level <= target.level)
        .map(|h| h.line);

    let starts = line_start_offsets(source);
    let start = *starts.get(target.line - 1)?;
    let end = match end_line {
        Some(l) => *starts.get(l - 1).unwrap_or(&source.len()),
        None => source.len(),
    };
    Some((start, end))
}

/// The full text of the section under `slug` (heading line included).
pub fn extract_section(source: &str, slug: &str) -> Result<String> {
    let (start, end) =
        section_bytes(source, slug).ok_or_else(|| Error::HeadingNotFound(slug.to_string()))?;
    Ok(source[start..end].to_string())
}

/// Replace the whole section under `slug` (heading included) with `content`.
///
/// `content` should include the heading line for the section it replaces.
/// A single trailing newline is normalised so the surrounding document keeps
/// its block structure.
pub fn replace_section(source: &str, slug: &str, content: &str) -> Result<String> {
    if content.trim().is_empty() {
        return Err(Error::Invalid(
            "section replacement is empty; provide the heading line and its body".to_string(),
        ));
    }
    let (start, end) =
        section_bytes(source, slug).ok_or_else(|| Error::HeadingNotFound(slug.to_string()))?;
    let mut replacement = content.trim_end().to_string();
    replacement.push('\n');
    // Keep a blank line before the following heading (there always is one).
    if end < source.len() {
        replacement.push('\n');
    }
    let mut out = String::with_capacity(start + replacement.len() + (source.len() - end));
    out.push_str(&source[..start]);
    out.push_str(&replacement);
    out.push_str(&source[end..]);
    Ok(out)
}

/// Byte offset at which each line begins (line 1 at index 0).
fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Top\n\nintro\n\n## A\n\nalpha body\n\n## B\n\nbeta body\n\n### B.1\n\nsub\n\n## C\n\ngamma\n";

    #[test]
    fn extracts_section_until_same_level() {
        let s = extract_section(DOC, "a").unwrap();
        assert_eq!(s, "## A\n\nalpha body\n\n");
    }

    #[test]
    fn extract_includes_nested_subsections() {
        let s = extract_section(DOC, "b").unwrap();
        assert!(s.contains("beta body"));
        assert!(s.contains("### B.1"));
        assert!(s.contains("sub"));
        assert!(!s.contains("gamma")); // stops at ## C
    }

    #[test]
    fn replaces_section() {
        let out = replace_section(DOC, "a", "## A\n\nNEW alpha").unwrap();
        assert!(out.contains("## A\n\nNEW alpha"));
        assert!(!out.contains("alpha body"));
        assert!(out.contains("## B")); // rest intact
        assert!(out.contains("gamma"));
    }

    #[test]
    fn missing_slug_errors() {
        assert!(extract_section(DOC, "nope").is_err());
    }
}
