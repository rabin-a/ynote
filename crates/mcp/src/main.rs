//! papery MCP server — a small, stable, typed tool surface over `papery-core`.
//!
//! stdio transport (rmcp). Every tool is documented for an LLM consumer and
//! every file operation goes through core's path-safety check, so no tool can
//! read or write outside the project root.
//!
//! Registration (e.g. Claude Code):
//! ```json
//! { "mcpServers": { "papery": { "command": "papery-mcp", "args": ["--project", "."] } } }
//! ```

use std::path::{Path, PathBuf};

use papery_core::{check, export, outline, section, Format, Project};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};

#[derive(Clone)]
struct PaperyServer {
    root: PathBuf,
    #[allow(dead_code)] // read by macro-generated code
    tool_router: ToolRouter<PaperyServer>,
}

// ---- tool parameter types (field doc comments become JSON Schema docs) ----

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadArgs {
    /// Project-relative path to the markdown document, e.g. `docs/intro.md`.
    path: String,
    /// Optional heading slug. When set, only that section (the heading and its
    /// body, up to the next same-or-higher heading) is returned — cheaper than
    /// reading a long document in full. Get slugs from `get_outline`.
    #[serde(default)]
    heading_slug: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct WriteArgs {
    /// Project-relative path to write. Parent directories are created. Paths
    /// outside the project root are refused.
    path: String,
    /// Full new file contents (markdown).
    content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EditSectionArgs {
    /// Project-relative path to the document.
    path: String,
    /// Slug of the heading whose section to replace (from `get_outline`).
    heading_slug: String,
    /// Replacement markdown for the whole section. Include the heading line
    /// itself (e.g. `## Setup\n\n...`) — it replaces the heading and its body.
    content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct PathArg {
    /// Project-relative path to the markdown document.
    path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ExportArgs {
    /// Project-relative path to the document to export.
    path: String,
    /// Output format: `pdf`, `docx`, or `html`.
    format: String,
    /// Optional output path (project-relative or absolute). When omitted, the
    /// file is written next to the project root as `<name>.<ext>`.
    #[serde(default)]
    out: Option<String>,
}

#[tool_router]
impl PaperyServer {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            tool_router: Self::tool_router(),
        }
    }

    fn project(&self) -> Result<Project, McpError> {
        Project::discover(&self.root).map_err(core_err)
    }

    /// List every document in the project with its title (front matter `title`
    /// or first H1). Use this to discover what exists before reading files.
    #[tool(description = "List all project documents with their relative paths and titles.")]
    async fn list_documents(&self) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let docs = project.documents().map_err(core_err)?;
        let items: Vec<_> = docs
            .iter()
            .map(|d| {
                let title = project
                    .read_document(d)
                    .ok()
                    .and_then(|t| papery_core::parse::title_of(&t));
                serde_json::json!({ "path": d.to_string_lossy(), "title": title })
            })
            .collect();
        json_result(&items)
    }

    /// Read a document, or just one section of it.
    #[tool(
        description = "Read a document's full markdown, or only the section under a heading slug (pass heading_slug to avoid loading a long file in full)."
    )]
    async fn read_document(
        &self,
        Parameters(args): Parameters<ReadArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let text = project.read_document(&args.path).map_err(core_err)?;
        let out = match args.heading_slug {
            Some(slug) => section::extract_section(&text, &slug).map_err(core_err)?,
            None => text,
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(out)]))
    }

    /// Overwrite a whole document. Prefer `edit_section` for targeted changes
    /// to long files — it is cheaper and less error-prone than rewriting.
    #[tool(
        description = "Write a full document (creates parent dirs; refuses paths outside the project root). Prefer edit_section for targeted changes to long documents."
    )]
    async fn write_document(
        &self,
        Parameters(args): Parameters<WriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let abs = project
            .write_document(&args.path, &args.content)
            .map_err(core_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "wrote {}",
            abs.display()
        ))]))
    }

    /// Replace one section (identified by heading slug) instead of the whole
    /// file. Preferred over `write_document` for long documents.
    #[tool(
        description = "Replace the section under a heading slug with new markdown. Preferred over write_document for targeted edits to long documents."
    )]
    async fn edit_section(
        &self,
        Parameters(args): Parameters<EditSectionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let text = project.read_document(&args.path).map_err(core_err)?;
        let updated =
            section::replace_section(&text, &args.heading_slug, &args.content).map_err(core_err)?;
        let abs = project
            .write_document(&args.path, &updated)
            .map_err(core_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "updated section `{}` in {}",
            args.heading_slug,
            abs.display()
        ))]))
    }

    /// Heading tree with slugs and line numbers — navigate long documents
    /// without reading them fully.
    #[tool(description = "Get a document's heading outline (level, text, slug, line) as JSON.")]
    async fn get_outline(
        &self,
        Parameters(args): Parameters<PathArg>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let text = project.read_document(&args.path).map_err(core_err)?;
        json_result(&outline(&text))
    }

    /// Render a document to standalone, self-contained HTML — the same output
    /// the export pipeline produces.
    #[tool(
        description = "Render a document to a standalone HTML string (embedded CSS, inlined images)."
    )]
    async fn render_html(
        &self,
        Parameters(args): Parameters<PathArg>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let text = project.read_document(&args.path).map_err(core_err)?;
        let html =
            export::html_standalone(&project, Path::new(&args.path), &text).map_err(core_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(html)]))
    }

    /// Export a document to PDF/DOCX/HTML and return the absolute output path.
    #[tool(
        description = "Export a document to pdf, docx, or html. Returns the absolute output path."
    )]
    async fn export(
        &self,
        Parameters(args): Parameters<ExportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let format = Format::from_str_ci(&args.format).map_err(core_err)?;
        // Confined export: `out` is project-relative and cannot escape the root
        // (absolute paths and `..` are rejected). No MCP tool writes outside it.
        let out_ref = args.out.as_deref().map(Path::new);
        let written = export::export_confined(&project, Path::new(&args.path), format, out_ref)
            .map_err(core_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            written.display().to_string(),
        )]))
    }

    /// Lint the project for broken relative links and images.
    #[tool(
        description = "Check the project for broken relative links and images; returns findings as JSON."
    )]
    async fn check_project(&self) -> Result<CallToolResult, McpError> {
        let project = self.project()?;
        let findings = check::check_project(&project).map_err(core_err)?;
        json_result(&findings)
    }
}

#[tool_handler]
impl ServerHandler for PaperyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "papery exposes a folder of markdown files as a project. Use get_outline to \
                 navigate, read_document (with heading_slug) to read a section, and edit_section \
                 rather than write_document for targeted edits to long documents. render_html and \
                 export use the same shared renderer, so preview and export match. All paths are \
                 project-relative and confined to the project root.",
            )
    }
}

fn core_err(e: papery_core::Error) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

fn json_result<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
}

fn parse_project_arg() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--project" || a == "-p" {
            if let Some(v) = args.next() {
                return PathBuf::from(v);
            }
        } else if let Some(v) = a.strip_prefix("--project=") {
            return PathBuf::from(v);
        }
    }
    PathBuf::from(".")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = parse_project_arg();
    // Validate the project root up front; a clear stderr message beats a
    // confusing handshake failure.
    if let Err(e) = Project::open(&root) {
        eprintln!("papery-mcp: cannot open project at {}: {e}", root.display());
        std::process::exit(2);
    }
    let service = PaperyServer::new(root).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
