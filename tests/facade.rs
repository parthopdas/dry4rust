//! Integration tests for the library façade (`dry4rust::run`).
//!
//! Drives the whole pipeline — discover → read + parse → detect → render — over
//! a temp fixture tree and asserts the exact report bytes for both formats, the
//! `/`-normalized paths (R6), and the N39 skip-with-diagnostic policy.

use std::fs;
use std::path::{Path, PathBuf};

use dry4rust::{run, Format, RunOptions, RunOutput};
use tempfile::TempDir;

/// A renamed (Type-2) clone pair: same structure, different identifiers and
/// literals, comfortably above the default `--min-lines`/`--min-nodes` floors.
const LEFT: &str = "fn sum_positive(values: &[i32]) -> i32 {
    let mut total = 0;
    for value in values {
        if *value > 0 {
            total += *value;
        }
    }
    total
}
";

const RIGHT: &str = "fn add_upbeat(numbers: &[i32]) -> i32 {
    let mut running = 7;
    for number in numbers {
        if *number > 3 {
            running += *number;
        }
    }
    running
}
";

/// A structurally unrelated function — no pair, so no candidates.
const LONE: &str = "fn describe(flag: bool) -> String {
    match flag {
        true => String::from(\"yes\"),
        false => String::from(\"no\"),
    }
}
";

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, contents).expect("write file");
}

/// The `/`-normalized `root/` prefix every reported path carries.
fn prefix(root: &Path) -> String {
    format!("{}/", root.to_string_lossy().replace('\\', "/"))
}

fn options(root: &Path, format: Format) -> RunOptions {
    RunOptions {
        paths: vec![PathBuf::from(root)],
        threshold: 0.75,
        min_lines: 4,
        min_nodes: 20,
        format,
    }
}

fn run_on(root: &Path, format: Format) -> RunOutput {
    run(&options(root, format)).expect("run should succeed")
}

/// A tree with exactly one duplicate pair, `a.rs` ↔ `b.rs`.
fn duplicate_pair_tree() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    write(dir.path(), "a.rs", LEFT);
    write(dir.path(), "b.rs", RIGHT);
    dir
}

#[test]
fn text_report_bytes_for_a_duplicate_pair() {
    let dir = duplicate_pair_tree();
    let at = prefix(dir.path());
    let output = run_on(dir.path(), Format::Text);

    assert_eq!(
        output.report,
        format!("DUPLICATE score=1.00\n  {at}a.rs:1-9\n  {at}b.rs:1-9\n")
    );
    assert!(output.diagnostics.is_empty());
}

#[test]
fn json_report_bytes_for_a_duplicate_pair() {
    let dir = duplicate_pair_tree();
    let at = prefix(dir.path());
    let output = run_on(dir.path(), Format::Json);

    assert_eq!(
        output.report,
        format!(
            r#"{{
  "candidates": [
    {{
      "score": 1.0,
      "left": {{
        "file": "{at}a.rs",
        "start_line": 1,
        "end_line": 9
      }},
      "right": {{
        "file": "{at}b.rs",
        "start_line": 1,
        "end_line": 9
      }},
      "left_nodes": 23,
      "right_nodes": 23
    }}
  ]
}}
"#
        )
    );
}

#[test]
fn reported_paths_are_forward_slash_normalized() {
    // Nested dirs make the separator visible on every OS (R6) — no cfg gate.
    let dir = TempDir::new().expect("create temp dir");
    write(dir.path(), "src/one/a.rs", LEFT);
    write(dir.path(), "src/two/b.rs", RIGHT);

    let output = run_on(dir.path(), Format::Text);
    assert!(!output.report.contains('\\'), "got: {}", output.report);
    assert!(output.report.contains("src/one/a.rs:1-9"));
    assert!(output.report.contains("src/two/b.rs:1-9"));
}

#[test]
fn a_tree_without_duplicates_reports_none() {
    let dir = TempDir::new().expect("create temp dir");
    write(dir.path(), "lone.rs", LONE);

    let output = run_on(dir.path(), Format::Text);
    assert_eq!(output.report, "No duplicate candidates found.\n");
}

#[test]
fn an_unparsable_file_is_skipped_with_a_diagnostic_and_does_not_change_stdout() {
    let dir = duplicate_pair_tree();
    let clean = run_on(dir.path(), Format::Text);

    // Same tree, plus one file `syn` cannot parse (N39).
    write(dir.path(), "broken.rs", "fn broken( {\n");
    let with_broken = run_on(dir.path(), Format::Text);

    assert!(clean.diagnostics.is_empty());
    // stdout bytes are unaffected by the skipped file.
    assert_eq!(with_broken.report, clean.report);

    assert_eq!(with_broken.diagnostics.len(), 1);
    let expected_prefix = format!("warning: skipping {}broken.rs: ", prefix(dir.path()));
    assert!(
        with_broken.diagnostics[0].starts_with(&expected_prefix),
        "got: {}",
        with_broken.diagnostics[0]
    );
    // One line, so it can never disturb the report when written to stderr.
    assert!(!with_broken.diagnostics[0].contains('\n'));
}

#[test]
fn a_missing_root_fails_the_whole_run() {
    let dir = TempDir::new().expect("create temp dir");
    let missing = dir.path().join("nope");

    assert!(run(&options(&missing, Format::Text)).is_err());
}
