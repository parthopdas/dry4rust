//! Discovery adapter: walks paths via the `ignore` crate, honors `.gitignore`,
//! scans `*.rs`, skips `/target`, and emits `/`-normalized paths.
//! Confines the `ignore` dependency to this module.

use std::path::{Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};

use crate::error::{Error, Result};

/// Discover the `.rs` files to analyze under the given CLI input `paths`.
///
/// Directories are walked with the `ignore` crate, honoring `.gitignore` and
/// skipping any `target` directory; files named explicitly on input are
/// included as-is. Every returned path is normalized to `/`-separated form and
/// the list is sorted and de-duplicated for deterministic, cross-OS-stable
/// output.
pub(crate) fn discover_rust_files(paths: &[PathBuf]) -> Result<Vec<String>> {
    let mut files: Vec<String> = Vec::new();
    let mut dirs: Vec<&PathBuf> = Vec::new();

    for input in paths {
        if input.is_file() {
            // Explicitly named files are included regardless of ignore rules.
            if has_rs_ext(input) {
                files.push(normalize(input));
            }
        } else {
            // Directories (and anything non-existent) go through the walker,
            // which surfaces missing paths as walk errors.
            dirs.push(input);
        }
    }

    if let Some((first, rest)) = dirs.split_first() {
        let mut builder = WalkBuilder::new(first);
        for dir in rest {
            builder.add(dir);
        }
        // Make discovery a pure function of the scanned tree so output is
        // stable across machines and checkouts (design.md determinism / A7/R6):
        // honor only in-tree `.gitignore` (even outside a git repo, e.g. temp
        // trees) and ignore any machine- or checkout-specific ignore sources.
        builder.require_git(false);
        builder.git_ignore(true); // honor in-tree `.gitignore`
        builder.git_global(false); // ignore machine-global `core.excludesFile`
        builder.git_exclude(false); // ignore `.git/info/exclude`
        builder.parents(false); // ignore `.gitignore` files above the scanned root
        builder.filter_entry(|entry| !is_target_dir(entry));

        for result in builder.build() {
            let entry = result.map_err(walk_err)?;
            let path = entry.path();
            if entry.file_type().is_some_and(|ft| ft.is_file()) && has_rs_ext(path) {
                files.push(normalize(path));
            }
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

/// Whether `path` has a `.rs` extension.
fn has_rs_ext(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("rs")
}

/// Whether `entry` is a directory named `target` (to be pruned).
fn is_target_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_some_and(|ft| ft.is_dir()) && entry.file_name() == "target"
}

/// Normalize a path to a `/`-separated string (cross-OS stability).
fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Map an `ignore` walk error onto [`Error::Io`], keeping the `ignore` crate
/// confined to this module. The offending path (normalized to `/`, or
/// `"<unknown>"` when absent) goes in `path`; the message is synthesized once
/// into `source` — carrying the underlying `io::Error` when available and
/// otherwise a single `io::Error::other` — so the Display does not stutter.
fn walk_err(err: ignore::Error) -> Error {
    let path = err_path(&err)
        .map(normalize)
        .unwrap_or_else(|| "<unknown>".to_string());
    // Compute the path-free inner message *before* `into_io_error` consumes
    // `err`; it is only used on the non-IO fallback path.
    let message = inner_message(&err);
    let source = err
        .into_io_error()
        .unwrap_or_else(|| std::io::Error::other(message));
    Error::Io { path, source }
}

/// The underlying error text with any path prefix peeled off. `ignore`'s
/// wrapper variants fold the offending path into their own Display, but that
/// path already lives in [`Error::Io`]'s `path` field — so recurse to the
/// innermost meaningful error to keep the composed Display from stuttering.
fn inner_message(err: &ignore::Error) -> String {
    match err {
        ignore::Error::WithPath { err, .. } | ignore::Error::WithDepth { err, .. } => {
            inner_message(err)
        }
        ignore::Error::WithLineNumber { line, err } => {
            format!("line {line}: {}", inner_message(err))
        }
        // `child`/`ancestor` already surface via `Error::Io.path`.
        ignore::Error::Loop { .. } => "cycle detected".to_string(),
        ignore::Error::Io(e) => e.to_string(),
        // Leaf variants (`Glob`, `Partial`, `UnrecognizedFileType`,
        // `InvalidDefinition`, …) carry no path in their own Display.
        _ => err.to_string(),
    }
}

/// Best-effort extraction of the offending path from an `ignore` walk error.
/// `ignore` embeds the path in its wrapper variants rather than exposing an
/// accessor, so peel those to find it.
fn err_path(err: &ignore::Error) -> Option<&Path> {
    match err {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::Loop { child, .. } => Some(child),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            err_path(err)
        }
        ignore::Error::Partial(errs) => errs.iter().find_map(err_path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a fixture tree with `.rs`/non-`.rs` files, a `.gitignore`, and a
    /// `target/` dir. Returns the temp dir (kept alive by the caller).
    fn fixture() -> TempDir {
        let dir = TempDir::new().expect("create temp dir");
        let root = dir.path();

        write(root, ".gitignore", "ignored.rs\n");
        write(root, "src/a.rs", "fn a() {}\n");
        write(root, "src/b.rs", "fn b() {}\n");
        write(root, "src/nested/c.rs", "fn c() {}\n");
        write(root, "notes.txt", "not rust\n");
        write(root, "ignored.rs", "fn ignored() {}\n");
        write(root, "target/debug/build.rs", "fn built() {}\n");

        dir
    }

    /// A non-IO walk error whose `ignore` Display already embeds the path must
    /// not have that path repeated by [`Error::Io`]'s own Display: the
    /// synthesized `source` carries only the inner message. Regression guard
    /// for the N6 stutter (`... at bad/.gitignore: bad/.gitignore: ...`).
    #[test]
    fn walk_err_does_not_stutter_path_for_non_io_error() {
        // `WithPath` around a non-IO leaf: `into_io_error` yields `None`, so
        // `walk_err` falls back to the synthesized inner message.
        let err = ignore::Error::WithPath {
            path: PathBuf::from("bad/.gitignore"),
            err: Box::new(ignore::Error::UnrecognizedFileType("weird".to_string())),
        };
        let mapped = walk_err(err);
        let rendered = format!("{mapped}");

        assert_eq!(
            rendered.matches("bad/.gitignore").count(),
            1,
            "path should appear exactly once, got: {rendered}"
        );
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, contents).expect("write file");
    }

    /// Return discovered paths relative to `root` (root prefix stripped).
    fn relative(root: &Path, found: &[String]) -> Vec<String> {
        let prefix = format!("{}/", normalize(root));
        found
            .iter()
            .map(|p| p.strip_prefix(&prefix).unwrap_or(p).to_string())
            .collect()
    }

    #[test]
    fn filters_to_rs_and_honors_gitignore_and_skips_target() {
        let dir = fixture();
        let found = discover_rust_files(&[dir.path().to_path_buf()]).expect("discover");
        let rel = relative(dir.path(), &found);

        assert_eq!(rel, vec!["src/a.rs", "src/b.rs", "src/nested/c.rs"]);
        // Non-`.rs` excluded.
        assert!(!rel.iter().any(|p| p.ends_with("notes.txt")));
        // `.gitignore` honored.
        assert!(!rel.iter().any(|p| p.ends_with("ignored.rs")));
        // `/target` skipped.
        assert!(!rel.iter().any(|p| p.contains("target/")));
    }

    #[test]
    fn normalizes_paths_to_forward_slashes() {
        let dir = fixture();
        let found = discover_rust_files(&[dir.path().to_path_buf()]).expect("discover");

        assert!(!found.is_empty());
        assert!(found.iter().all(|p| !p.contains('\\')));
        assert!(found.iter().any(|p| p.contains("src/nested/c.rs")));
    }

    #[test]
    fn ordering_is_deterministic() {
        let dir = fixture();
        let found = discover_rust_files(&[dir.path().to_path_buf()]).expect("discover");

        let mut sorted = found.clone();
        sorted.sort();
        assert_eq!(found, sorted);
    }

    #[test]
    fn explicit_file_is_included_even_if_gitignored() {
        let dir = fixture();
        let ignored = dir.path().join("ignored.rs");
        let found = discover_rust_files(&[ignored]).expect("discover");
        let rel = relative(dir.path(), &found);

        assert_eq!(rel, vec!["ignored.rs"]);
    }

    #[test]
    fn explicit_non_rs_file_is_excluded() {
        let dir = fixture();
        let notes = dir.path().join("notes.txt");
        let found = discover_rust_files(&[notes]).expect("discover");

        assert!(found.is_empty());
    }

    #[test]
    fn duplicate_inputs_are_de_duplicated() {
        let dir = fixture();
        let file = dir.path().join("src/a.rs");
        let found = discover_rust_files(&[file.clone(), file]).expect("discover");
        let rel = relative(dir.path(), &found);

        assert_eq!(rel, vec!["src/a.rs"]);
    }

    /// A `.gitignore` in a parent directory *above* the scanned root must not
    /// influence results (`parents(false)`), keeping discovery a pure function
    /// of the scanned tree for cross-machine determinism.
    #[test]
    fn parent_gitignore_above_scanned_root_is_ignored() {
        let dir = TempDir::new().expect("create temp dir");
        let root = dir.path();

        // Parent-level ignore that would hide `hidden.rs` if honored.
        write(root, ".gitignore", "hidden.rs\n");
        write(root, "sub/hidden.rs", "fn hidden() {}\n");
        write(root, "sub/visible.rs", "fn visible() {}\n");

        // Scan only the subdirectory; the parent `.gitignore` is above it.
        let sub = root.join("sub");
        let found = discover_rust_files(std::slice::from_ref(&sub)).expect("discover");
        let prefix = format!("{}/", normalize(&sub));
        let rel: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(&prefix).unwrap_or(p).to_string())
            .collect();

        assert_eq!(rel, vec!["hidden.rs", "visible.rs"]);
    }
}
