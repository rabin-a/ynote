//! papery command-line interface — a thin adapter over `papery-core`.
//!
//! Exit codes: 0 ok · 1 lint findings · 2 usage error (clap) · 3 IO/render error.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use papery_core::{check, export, outline, Format, Project};

#[derive(Parser)]
#[command(
    name = "papery",
    version,
    about = "Markdown editor, previewer, and exporter"
)]
struct Cli {
    /// Project directory (defaults to the current dir, walking up to papery.toml).
    #[arg(long, global = true)]
    project: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all project documents (relative paths).
    List(ListArgs),
    /// Print the heading tree of a document.
    Outline(OutlineArgs),
    /// Render a document to standalone HTML.
    Render(RenderArgs),
    /// Export a document (or all) to PDF/DOCX/HTML.
    Export(ExportArgs),
    /// Re-export a document whenever it changes on disk.
    Watch(WatchArgs),
    /// Lint the project for broken relative links and images.
    Check(CheckArgs),
}

#[derive(Args)]
struct ListArgs {
    /// Machine-readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct OutlineArgs {
    file: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RenderArgs {
    file: PathBuf,
    /// Output file (stdout if omitted).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct ExportArgs {
    /// Document to export (omit with --all).
    file: Option<PathBuf>,
    /// Export every document in the project.
    #[arg(long)]
    all: bool,
    /// Output format: pdf, docx, or html.
    #[arg(short, long)]
    format: String,
    /// Output file or directory.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct WatchArgs {
    file: PathBuf,
    #[arg(short, long, default_value = "html")]
    format: String,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct CheckArgs {
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(3);
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<i32> {
    let project = open_project(cli.project.as_deref())?;
    match cli.command {
        Command::List(a) => cmd_list(&project, a),
        Command::Outline(a) => cmd_outline(&project, a),
        Command::Render(a) => cmd_render(&project, a),
        Command::Export(a) => cmd_export(&project, a),
        Command::Watch(a) => cmd_watch(&project, a),
        Command::Check(a) => cmd_check(&project, a),
    }
}

fn open_project(dir: Option<&Path>) -> anyhow::Result<Project> {
    let start = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let project = Project::discover(&start)?;
    for w in project.warnings() {
        eprintln!("warning: {w}");
    }
    Ok(project)
}

/// Convert a user-supplied path (cwd-relative or absolute) to a project-relative
/// path core can consume, enforcing the project boundary.
fn to_rel(project: &Project, arg: &Path) -> anyhow::Result<PathBuf> {
    let abs = if arg.is_absolute() {
        arg.to_path_buf()
    } else {
        // Prefer resolving against the project root; fall back to the cwd.
        let from_project = project.root().join(arg);
        if from_project.exists() {
            from_project
        } else {
            std::env::current_dir()?.join(arg)
        }
    };
    let canonical = abs.canonicalize().unwrap_or(abs);
    let rel = project.relativize(&canonical);
    if rel.is_absolute() || rel.starts_with("..") {
        anyhow::bail!("{} is outside the project root", arg.display());
    }
    Ok(rel)
}

fn cmd_list(project: &Project, args: ListArgs) -> anyhow::Result<i32> {
    let docs = project.documents()?;
    if args.json {
        let items: Vec<_> = docs
            .iter()
            .map(|d| {
                let title = project
                    .read_document(d)
                    .ok()
                    .and_then(|text| papery_core::parse::title_of(&text));
                serde_json::json!({ "path": d.to_string_lossy(), "title": title })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        for d in &docs {
            println!("{}", d.to_string_lossy());
        }
    }
    Ok(0)
}

fn cmd_outline(project: &Project, args: OutlineArgs) -> anyhow::Result<i32> {
    let rel = to_rel(project, &args.file)?;
    let text = project.read_document(&rel)?;
    let headings = outline(&text);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&headings)?);
    } else {
        print!("{}", papery_core::outline::outline_text(&headings));
    }
    Ok(0)
}

fn cmd_render(project: &Project, args: RenderArgs) -> anyhow::Result<i32> {
    let rel = to_rel(project, &args.file)?;
    let text = project.read_document(&rel)?;
    let html = export::html_standalone(project, &rel, &text)?;
    match args.output {
        Some(out) => {
            std::fs::write(&out, html)?;
            eprintln!("wrote {}", out.display());
        }
        None => print!("{html}"),
    }
    Ok(0)
}

fn cmd_export(project: &Project, args: ExportArgs) -> anyhow::Result<i32> {
    let format = Format::from_str_ci(&args.format)?;
    let out = args.output.unwrap_or_default();

    if args.all {
        let docs = project.documents()?;
        if docs.is_empty() {
            eprintln!("no documents to export");
        }
        for d in &docs {
            let written = export::export(project, d, format, &out)?;
            println!("{}", written.display());
        }
        return Ok(0);
    }

    let file = args
        .file
        .ok_or_else(|| anyhow::anyhow!("provide a FILE or use --all"))?;
    let rel = to_rel(project, &file)?;
    let written = export::export(project, &rel, format, &out)?;
    println!("{}", written.display());
    Ok(0)
}

fn cmd_check(project: &Project, args: CheckArgs) -> anyhow::Result<i32> {
    let findings = check::check_project(project)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&findings)?);
    } else if findings.is_empty() {
        eprintln!("no issues found");
    } else {
        for f in &findings {
            println!("{}:{}: {} [{}]", f.file, f.line, f.message, f.kind);
        }
    }
    Ok(if findings.is_empty() { 0 } else { 1 })
}

fn cmd_watch(project: &Project, args: WatchArgs) -> anyhow::Result<i32> {
    use notify::{RecursiveMode, Watcher};
    use std::sync::mpsc::channel;

    let format = Format::from_str_ci(&args.format)?;
    let rel = to_rel(project, &args.file)?;
    let abs = project.resolve_path(&rel)?;
    let out = args.output.unwrap_or_default();

    // Export once up front.
    do_export(project, &rel, format, &out);

    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    // Watch the parent dir; editors often replace files rather than write in place.
    let watch_target = abs.parent().unwrap_or(&abs);
    watcher.watch(watch_target, RecursiveMode::NonRecursive)?;
    eprintln!("watching {} (Ctrl-C to stop)", abs.display());

    for res in rx {
        match res {
            Ok(event) => {
                if event.paths.iter().any(|p| p == &abs) {
                    do_export(project, &rel, format, &out);
                }
            }
            Err(e) => eprintln!("watch error: {e}"),
        }
    }
    Ok(0)
}

fn do_export(project: &Project, rel: &Path, format: Format, out: &Path) {
    match export::export(project, rel, format, out) {
        Ok(p) => eprintln!("exported {}", p.display()),
        Err(e) => eprintln!("export failed: {e}"),
    }
}
