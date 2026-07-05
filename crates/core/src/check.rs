//! Project lint: broken relative links and images. Used by the CLI `check`
//! command and the MCP `check_project` tool.

use std::path::Path;

use comrak::nodes::NodeValue;
use comrak::Arena;
use serde::Serialize;

use crate::error::Result;
use crate::parse;
use crate::project::Project;

/// A single lint finding.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    /// Document the finding is in (root-relative).
    pub file: String,
    /// 1-based source line.
    pub line: usize,
    /// `"link"` or `"image"`.
    pub kind: String,
    /// The offending URL/target as written.
    pub target: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Lint every document in the project.
pub fn check_project(project: &Project) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for doc in project.documents()? {
        let text = project.read_document(&doc)?;
        findings.extend(check_document(project, &doc, &text));
    }
    Ok(findings)
}

/// Lint a single already-loaded document.
pub fn check_document(project: &Project, rel: &Path, source: &str) -> Vec<Finding> {
    let arena = Arena::new();
    let (root, _fm) = parse::parse(&arena, source);
    let file = rel.to_string_lossy().to_string();
    let mut findings = Vec::new();

    for node in root.descendants() {
        let (url, kind, line) = {
            let d = node.data.borrow();
            match &d.value {
                NodeValue::Link(l) => (l.url.clone(), "link", d.sourcepos.start.line),
                NodeValue::Image(l) => (l.url.clone(), "image", d.sourcepos.start.line),
                _ => continue,
            }
        };

        // Skip remote URLs and pure in-document anchors.
        if is_remote(&url) || url.starts_with('#') || url.is_empty() {
            continue;
        }
        // Split off any `#fragment`.
        let path_part = url.split('#').next().unwrap_or(&url);
        if path_part.is_empty() {
            continue;
        }

        match project.resolve_asset(rel, path_part) {
            Ok(abs) => {
                if !abs.exists() {
                    findings.push(Finding {
                        file: file.clone(),
                        line,
                        kind: kind.to_string(),
                        target: url.clone(),
                        message: format!("{kind} target does not exist: {path_part}"),
                    });
                }
            }
            Err(_) => findings.push(Finding {
                file: file.clone(),
                line,
                kind: kind.to_string(),
                target: url.clone(),
                message: format!("{kind} target escapes the project root: {path_part}"),
            }),
        }
    }
    findings
}

fn is_remote(url: &str) -> bool {
    let u = url.trim_start();
    u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("//")
        || u.starts_with("mailto:")
        || u.starts_with("tel:")
        || u.starts_with("data:")
}
