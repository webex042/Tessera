//! Stage 4 exit criterion: no fact is auto-closed without user confirmation (plan §11.3).
//!
//! A source scan over every crate and the app: fact status may be changed
//! - by `tsr_store::facts::set_status`, which only the app's approval-token command calls
//!   (`src-tauri/src/views.rs`, `set_fact_status`), and
//! - by SQL that writes `proposed_done` (the pipeline's only status change).
//!
//! Any other `SET status` on facts, or another caller of `set_status`, fails this test.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.is_dir() {
            if !matches!(name.as_str(), "target" | "tests" | "node_modules" | "gen") {
                rust_files(&p, out);
            }
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

#[test]
fn facts_reach_done_or_dropped_only_through_the_user_approval_command() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("src-tauri").join("src"), &mut files);
    rust_files(&root.join("evals"), &mut files);
    assert!(files.len() > 50, "scanned {} files", files.len());

    let mut callers = Vec::new();
    let mut writes = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let src = std::fs::read_to_string(f).unwrap();
        for (n, line) in src.lines().enumerate() {
            if line.contains("set_status(") && !line.contains("fn set_status(") {
                callers.push(format!("{rel}:{}", n + 1));
            }
            if line.contains("SET status") {
                writes.push((format!("{rel}:{}", n + 1), line.to_string()));
            }
        }
    }
    assert!(!callers.is_empty());
    for c in &callers {
        assert!(
            c.starts_with("src-tauri/src/views.rs:"),
            "unexpected status change caller: {c}"
        );
    }
    for (at, line) in &writes {
        let ok = (at.starts_with("crates/tsr-store/src/facts.rs:") && line.contains("SET status = ?2"))
            || line.contains("SET status = 'proposed_done'");
        assert!(ok, "unexpected fact status write at {at}: {}", line.trim());
    }
}

#[test]
fn the_approval_command_requires_a_single_use_token() {
    // The one caller must sit behind token consumption in the same function.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let views = std::fs::read_to_string(root.join("src-tauri/src/views.rs")).unwrap();
    let call = views.find("facts::set_status(").expect("caller");
    let func = views[..call].rfind("pub fn ").expect("enclosing fn");
    let body = &views[func..call];
    assert!(
        body.contains("pub fn set_fact_status"),
        "set_status is called from set_fact_status"
    );
    assert!(
        body.contains("token"),
        "set_fact_status checks the approval token first"
    );
}
