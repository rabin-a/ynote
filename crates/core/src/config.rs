//! `papery.toml` model.
//!
//! Every field is optional and has a sensible default, so a project with no
//! config file behaves identically to one with an empty `[project]` table.
//! Unknown keys are ignored (a warning is surfaced separately), never an error.

use serde::{Deserialize, Serialize};

/// Fully-resolved project configuration (defaults already applied).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub project: ProjectConfig,
    pub render: RenderConfig,
    pub export: ExportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub name: Option<String>,
    /// Glob patterns (relative to root) to exclude from document enumeration.
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    /// Theme name -> `assets/themes/<name>.css`.
    pub theme: String,
    /// syntect highlighting theme name.
    pub syntax_theme: String,
    /// Enable KaTeX-compatible math rendering.
    pub math: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            theme: "default".to_string(),
            syntax_theme: "github".to_string(),
            math: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExportConfig {
    pub pdf: PdfConfig,
    pub docx: DocxConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PdfConfig {
    pub template: String,
    pub paper: String,
    pub margin: String,
    pub font: String,
    pub toc: bool,
}

impl Default for PdfConfig {
    fn default() -> Self {
        PdfConfig {
            template: "default".to_string(),
            paper: "a4".to_string(),
            margin: "2.5cm".to_string(),
            font: "Inter".to_string(),
            toc: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DocxConfig {
    pub template: String,
}

impl Default for DocxConfig {
    fn default() -> Self {
        DocxConfig {
            template: "default".to_string(),
        }
    }
}

impl Config {
    /// Parse config text, collecting warnings for unknown keys rather than failing.
    ///
    /// Returns the parsed config plus any human-readable warnings. On a genuine
    /// syntax error the whole parse fails with `Error::Config`.
    pub fn parse(text: &str) -> crate::Result<(Config, Vec<String>)> {
        // First pass: strict-ish parse into our typed struct. `#[serde(default)]`
        // makes missing keys fine; unknown keys are silently accepted by toml.
        let config: Config =
            toml::from_str(text).map_err(|e| crate::Error::Config(e.to_string()))?;

        // Second pass: diff the raw table against known keys to warn on typos.
        let warnings = collect_unknown_key_warnings(text);
        Ok((config, warnings))
    }
}

/// Walk the raw TOML value tree and warn about keys we don't recognise,
/// including the nested `[export.pdf]` / `[export.docx]` tables.
fn collect_unknown_key_warnings(text: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let Ok(value) = text.parse::<toml::Value>() else {
        return warnings;
    };
    let known: &[(&str, &[&str])] = &[
        ("project", &["name", "exclude"]),
        ("render", &["theme", "syntax_theme", "math"]),
        ("export", &["pdf", "docx"]),
    ];
    // Allowed keys for the nested export sub-tables.
    let export_nested: &[(&str, &[&str])] = &[
        ("pdf", &["template", "paper", "margin", "font", "toc"]),
        ("docx", &["template"]),
    ];

    if let Some(table) = value.as_table() {
        for (section, keys) in table {
            let Some(allowed) = known.iter().find(|(s, _)| s == section).map(|(_, k)| *k) else {
                warnings.push(format!("unknown config section `[{section}]`"));
                continue;
            };
            if let Some(sub) = keys.as_table() {
                for (key, val) in sub {
                    if !allowed.contains(&key.as_str()) {
                        warnings.push(format!("unknown config key `{section}.{key}`"));
                        continue;
                    }
                    // Recurse one level for export.pdf / export.docx.
                    if section == "export" {
                        if let (Some(nested_allowed), Some(nested_tbl)) = (
                            export_nested
                                .iter()
                                .find(|(k, _)| k == key)
                                .map(|(_, v)| *v),
                            val.as_table(),
                        ) {
                            for nk in nested_tbl.keys() {
                                if !nested_allowed.contains(&nk.as_str()) {
                                    warnings
                                        .push(format!("unknown config key `export.{key}.{nk}`"));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    warnings
}
