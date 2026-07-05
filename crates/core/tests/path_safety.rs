//! Hard security requirement: no path may escape the project root. The MCP
//! server exposes these operations to agents, so this is exercised directly.

use std::path::Path;

use ynote_core::Project;

fn temp_project() -> (tempdir::Dir, Project) {
    let dir = tempdir::Dir::new("ynote_pathsafety");
    std::fs::write(dir.path().join("ok.md"), "# ok\n").unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/inner.md"), "# inner\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    (dir, project)
}

/// Minimal self-contained temp-dir helper (no external tempfile dep needed for
/// the integration test target).
mod tempdir {
    use std::path::{Path, PathBuf};
    pub struct Dir(PathBuf);
    impl Dir {
        pub fn new(prefix: &str) -> Dir {
            // Deterministic-ish unique name from pid + a static counter.
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn rejects_parent_traversal() {
    let (_d, project) = temp_project();
    assert!(project.resolve_path("../secret.md").is_err());
    assert!(project.resolve_path("sub/../../secret.md").is_err());
    assert!(project
        .resolve_path("a/b/c/../../../../etc/passwd")
        .is_err());
}

#[test]
fn rejects_absolute_paths() {
    let (_d, project) = temp_project();
    assert!(project.resolve_path("/etc/passwd").is_err());
    assert!(project.read_document("/etc/passwd").is_err());
}

#[test]
fn allows_paths_inside_root() {
    let (_d, project) = temp_project();
    assert!(project.resolve_path("ok.md").is_ok());
    assert!(project.resolve_path("sub/inner.md").is_ok());
    assert!(project.resolve_path("new/dir/file.md").is_ok()); // not-yet-existing is fine
}

#[test]
fn interior_dotdot_that_stays_inside_is_allowed() {
    let (_d, project) = temp_project();
    // sub/../ok.md resolves back to ok.md — inside the root, so allowed.
    let resolved = project.resolve_path("sub/../ok.md").unwrap();
    assert!(resolved.ends_with("ok.md"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    let (dir, project) = temp_project();
    // Create a directory outside the root and a symlink to it inside the root.
    let outside = std::env::temp_dir().join(format!("ynote_outside_{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("target.md"), "secret\n").unwrap();
    let link = dir.path().join("escape");
    let _ = std::os::unix::fs::symlink(&outside, &link);

    // Reading through the symlink must be refused (canonicalized path is outside).
    let result = project.resolve_path("escape/target.md");
    assert!(result.is_err(), "symlink escape must be rejected");
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn write_stays_within_root_and_creates_dirs() {
    let (dir, project) = temp_project();
    let abs = project
        .write_document("deep/nested/new.md", "# new\n")
        .unwrap();
    assert!(abs.starts_with(dir.path().canonicalize().unwrap()));
    assert!(abs.is_file());
    assert!(project.write_document("../evil.md", "x").is_err());
}

#[test]
fn confined_export_rejects_escapes() {
    use ynote_core::{export, Format};
    let (_d, project) = temp_project();
    let fmt = Format::Html; // HTML export needs no extra feature
                            // Relative `..` escape.
    assert!(export::export_confined(
        &project,
        Path::new("ok.md"),
        fmt,
        Some(Path::new("../evil.html"))
    )
    .is_err());
    // Absolute out.
    assert!(export::export_confined(
        &project,
        Path::new("ok.md"),
        fmt,
        Some(Path::new("/tmp/evil.html"))
    )
    .is_err());
    // A benign relative out inside the root succeeds.
    let ok = export::export_confined(
        &project,
        Path::new("ok.md"),
        fmt,
        Some(Path::new("dist/ok.html")),
    );
    assert!(ok.is_ok());
}

#[test]
fn asset_resolution_is_confined() {
    let (_d, project) = temp_project();
    assert!(project.resolve_asset("sub/inner.md", "../ok.md").is_ok()); // sibling dir, inside
    assert!(project
        .resolve_asset("sub/inner.md", "../../outside.png")
        .is_err());
    assert!(project
        .resolve_asset(Path::new("ok.md"), "/etc/passwd")
        .is_err());
}
