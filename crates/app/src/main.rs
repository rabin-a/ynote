//! papery desktop app (Tauri 2) — a thin adapter over `papery-core`.
//!
//! All rendering/export logic lives in core; these commands only marshal
//! between the webview and core. Live preview renders the *editor buffer*
//! (unsaved content), so what you type is what you see and what you export.

// Hide the console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use papery_core::{check, export, outline, render_html, Format, Project};
use serde::Serialize;

fn proj(root: &str) -> Result<Project, String> {
    Project::open(root).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct ProjectInfo {
    name: String,
    root: String,
    config_path: String,
    docs: Vec<DocEntry>,
}

#[derive(Serialize)]
struct DocEntry {
    path: String,
    title: Option<String>,
    /// Creation time (unix seconds) for date-based sorting; falls back to the
    /// modified time when the filesystem doesn't expose a creation time.
    created: u64,
}

fn doc_entries(project: &Project) -> Result<Vec<DocEntry>, String> {
    let docs = project.documents().map_err(|e| e.to_string())?;
    Ok(docs
        .iter()
        .map(|d| {
            let content = project.read_document(d).ok();
            let title = content
                .as_deref()
                .and_then(papery_core::parse::display_title);
            let created = project
                .resolve_path(d)
                .ok()
                .and_then(|p| std::fs::metadata(&p).ok())
                .and_then(|m| m.created().or_else(|_| m.modified()).ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            DocEntry {
                path: d.to_string_lossy().replace('\\', "/"),
                title,
                created,
            }
        })
        .collect())
}

#[tauri::command]
fn open_project(path: String) -> Result<ProjectInfo, String> {
    let project = proj(&path)?;
    let name = project.config().project.name.clone().unwrap_or_else(|| {
        project
            .root()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Project".to_string())
    });
    Ok(ProjectInfo {
        name,
        root: project.root().to_string_lossy().to_string(),
        config_path: project
            .root()
            .join("papery.toml")
            .to_string_lossy()
            .to_string(),
        docs: doc_entries(&project)?,
    })
}

#[tauri::command]
fn list_docs(root: String) -> Result<Vec<DocEntry>, String> {
    doc_entries(&proj(&root)?)
}

#[tauri::command]
fn read_file(root: String, path: String) -> Result<String, String> {
    proj(&root)?.read_document(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_file(root: String, path: String, content: String) -> Result<(), String> {
    proj(&root)?
        .write_document(&path, &content)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_file(root: String, path: String) -> Result<(), String> {
    let project = proj(&root)?;
    let abs = project.resolve_path(&path).map_err(|e| e.to_string())?;
    if abs.exists() {
        return Err(format!("{path} already exists"));
    }
    let title = Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());
    project
        .write_document(&path, &format!("# {title}\n\n"))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_file(root: String, from: String, to: String) -> Result<(), String> {
    let project = proj(&root)?;
    let src = project.resolve_path(&from).map_err(|e| e.to_string())?;
    let dst = project.resolve_path(&to).map_err(|e| e.to_string())?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(src, dst).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_file(root: String, path: String) -> Result<(), String> {
    let project = proj(&root)?;
    let abs = project.resolve_path(&path).map_err(|e| e.to_string())?;
    std::fs::remove_file(abs).map_err(|e| e.to_string())
}

#[tauri::command]
fn render_preview(root: String, path: String, content: String) -> Result<String, String> {
    let project = proj(&root)?;
    export::html_preview(&project, Path::new(&path), &content).map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_css(root: String) -> Result<String, String> {
    // `root` is accepted so the frontend's call shape stays uniform, but the
    // app supplies its own bespoke prose theme (style.css); from core we only
    // need the syntect token colors. A dark theme matches the design's dark
    // code blocks.
    let _ = root;
    Ok(render_html::syntect_css("base16-ocean.dark"))
}

#[tauri::command]
fn get_outline(content: String) -> Result<Vec<papery_core::Heading>, String> {
    Ok(outline(&content))
}

#[tauri::command]
fn export_doc(
    root: String,
    path: String,
    format: String,
    out: Option<String>,
    toc: Option<bool>,
) -> Result<String, String> {
    let project = proj(&root)?;
    let fmt = Format::from_str_ci(&format).map_err(|e| e.to_string())?;
    // `out`, when provided, is the exact destination the user picked in the save
    // dialog (a file path). With no `out`, fall back to a `dist/` folder.
    let out_path = match out {
        Some(o) => PathBuf::from(o),
        None => {
            let d = project.root().join("dist");
            std::fs::create_dir_all(&d).ok();
            d
        }
    };
    export::export_with(&project, Path::new(&path), fmt, &out_path, toc)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_project(root: String) -> Result<Vec<check::Finding>, String> {
    check::check_project(&proj(&root)?).map_err(|e| e.to_string())
}

/// Resolve the project to open on launch — zero setup required:
/// 1. a directory passed on the command line, else
/// 2. the last project that was opened, else
/// 3. a default workspace at `~/Documents/papery`.
/// The chosen directory is created and seeded (if empty) and remembered, so the
/// app always opens straight into a usable project.
#[tauri::command]
fn startup_project(app: tauri::AppHandle) -> Result<String, String> {
    if let Some(arg) = std::env::args().nth(1) {
        if !arg.starts_with('-') {
            let dir = PathBuf::from(arg);
            return ensure_workspace(&app, &dir);
        }
    }
    if let Some(prev) = last_project(&app) {
        if prev.is_dir() {
            return ensure_workspace(&app, &prev);
        }
    }
    ensure_workspace(&app, &default_workspace(&app))
}

/// The default workspace location — a cloud-synced folder when one is available
/// so notes sync across devices, otherwise the local Documents folder.
fn default_workspace(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(home) = tauri::Manager::path(app).home_dir() {
        // macOS iCloud Drive.
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        if icloud.is_dir() {
            return icloud.join("papery");
        }
        // Common Dropbox location (macOS / Windows / Linux).
        let dropbox = home.join("Dropbox");
        if dropbox.is_dir() {
            return dropbox.join("papery");
        }
    }
    tauri::Manager::path(app)
        .document_dir()
        .or_else(|_| tauri::Manager::path(app).home_dir())
        .map(|d| d.join("papery"))
        .unwrap_or_else(|_| PathBuf::from("papery"))
}

/// Ensure `dir` exists, seed it with a welcome document if it has no markdown
/// yet, remember it as the last project, and return its canonical path.
fn ensure_workspace(app: &tauri::AppHandle, dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let has_md = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "md"))
        })
        .unwrap_or(false);
    if !has_md {
        let _ = std::fs::write(dir.join("welcome.md"), WELCOME_MD);
        let cfg = dir.join("papery.toml");
        if !cfg.exists() {
            let _ = std::fs::write(&cfg, DEFAULT_TOML);
        }
    }
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    remember_project(app, &canonical);
    Ok(canonical.to_string_lossy().to_string())
}

fn last_project_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = tauri::Manager::path(app).app_config_dir().ok()?;
    Some(dir.join("last-project.txt"))
}

fn last_project(app: &tauri::AppHandle) -> Option<PathBuf> {
    let f = last_project_file(app)?;
    let s = std::fs::read_to_string(f).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

fn remember_project(app: &tauri::AppHandle, dir: &Path) {
    if let Some(f) = last_project_file(app) {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(f, dir.to_string_lossy().as_bytes());
    }
}

const WELCOME_MD: &str = "---\ntitle: Welcome\n---\n# Welcome to papery\n\nThis is your local workspace. Everything here is a plain markdown **file** in a \
**folder** you own — no accounts, no database, no lock-in.\n\n## Getting started\n\n- Edit this file; changes **save automatically**.\n- Add a new file with the **+** in the sidebar.\n- Export to PDF, DOCX, or HTML from the top-right — one renderer, so the preview *is* the document.\n\n> Write in something you can still open when the company is gone.\n";

const DEFAULT_TOML: &str = "[project]\nname = \"Notes\"\n\n[render]\ntheme = \"default\"\nmath = true\n\n[export.pdf]\npaper = \"a4\"\ntoc = true\n";

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            list_docs,
            read_file,
            write_file,
            create_file,
            rename_file,
            delete_file,
            render_preview,
            preview_css,
            get_outline,
            export_doc,
            check_project,
            startup_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running papery");
}
