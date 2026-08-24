//! Integration test for the discovery entry point (`discover_rust_files`).
//!
//! Builds a temp tree with `.rs`/non-`.rs` files, a `.gitignore`, and a
//! `target/` dir, then asserts the discovered set, ordering, and `/`
//! normalization through the library's public API.

use std::fs;
use std::path::Path;

use dry4rust::discover_rust_files;
use tempfile::TempDir;

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, contents).expect("write file");
}

#[test]
fn discovers_rs_files_honoring_gitignore_and_skipping_target() {
    let dir = TempDir::new().expect("create temp dir");
    let root = dir.path();

    write(root, ".gitignore", "generated.rs\n");
    write(root, "src/lib.rs", "fn lib() {}\n");
    write(root, "src/util/helpers.rs", "fn help() {}\n");
    write(root, "README.md", "# docs\n");
    write(root, "generated.rs", "fn generated() {}\n");
    write(root, "target/debug/gen.rs", "fn gen() {}\n");

    let found = discover_rust_files(&[root.to_path_buf()]).expect("discover");

    let prefix = format!("{}/", root.to_string_lossy().replace('\\', "/"));
    let relative: Vec<String> = found
        .iter()
        .map(|p| p.strip_prefix(&prefix).unwrap_or(p).to_string())
        .collect();

    // Exact set + deterministic (sorted) ordering.
    assert_eq!(relative, vec!["src/lib.rs", "src/util/helpers.rs"]);

    // Every path is `/`-normalized.
    assert!(found.iter().all(|p| !p.contains('\\')));

    // Explicitly sorted output.
    let mut sorted = found.clone();
    sorted.sort();
    assert_eq!(found, sorted);
}
