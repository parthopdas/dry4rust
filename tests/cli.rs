//! Integration tests that drive the built `dry4rust` binary.
//!
//! Each run uses the fixture dir as the working directory and lets the
//! positional default (`.`) apply, so reported paths are `./a.rs` — stable and
//! identical on every OS (R6). Asserts full stdout bytes, alias byte-parity,
//! and the exit-code contract (0 on success, non-zero + empty stdout on usage
//! errors).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

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

const LONE: &str = "fn describe(flag: bool) -> String {
    match flag {
        true => String::from(\"yes\"),
        false => String::from(\"no\"),
    }
}
";

const TEXT_REPORT: &str = "DUPLICATE score=1.00\n  ./a.rs:1-9\n  ./b.rs:1-9\n";

const JSON_REPORT: &str = r#"{
  "candidates": [
    {
      "score": 1.0,
      "left": {
        "file": "./a.rs",
        "start_line": 1,
        "end_line": 9
      },
      "right": {
        "file": "./b.rs",
        "start_line": 1,
        "end_line": 9
      },
      "left_nodes": 23,
      "right_nodes": 23
    }
  ]
}
"#;

fn write(root: &Path, rel: &str, contents: &str) {
    fs::write(root.join(rel), contents).expect("write file");
}

/// A tree with exactly one duplicate pair, `a.rs` ↔ `b.rs`.
fn duplicate_pair_tree() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    write(dir.path(), "a.rs", LEFT);
    write(dir.path(), "b.rs", RIGHT);
    dir
}

/// Run the binary in `root` with `args`.
fn dry4rust(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dry4rust"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("run the dry4rust binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf-8")
}

#[test]
fn text_format_writes_the_report_and_exits_zero() {
    let dir = duplicate_pair_tree();
    let output = dry4rust(dir.path(), &["--format", "text"]);

    assert!(output.status.success());
    assert_eq!(stdout(&output), TEXT_REPORT);
}

#[test]
fn json_format_writes_the_report_and_exits_zero() {
    let dir = duplicate_pair_tree();
    let output = dry4rust(dir.path(), &["--format", "json"]);

    assert!(output.status.success());
    assert_eq!(stdout(&output), JSON_REPORT);
}

#[test]
fn no_arguments_scans_the_current_directory() {
    let dir = duplicate_pair_tree();
    let output = dry4rust(dir.path(), &[]);

    assert!(output.status.success());
    assert_eq!(stdout(&output), TEXT_REPORT);
}

#[test]
fn the_aliases_are_byte_identical_to_the_long_form() {
    let dir = duplicate_pair_tree();

    assert_eq!(
        dry4rust(dir.path(), &["--json"]).stdout,
        dry4rust(dir.path(), &["--format", "json"]).stdout
    );
    assert_eq!(
        dry4rust(dir.path(), &["--text"]).stdout,
        dry4rust(dir.path(), &["--format", "text"]).stdout
    );
}

#[test]
fn a_tree_without_duplicates_still_exits_zero() {
    let dir = TempDir::new().expect("create temp dir");
    write(dir.path(), "lone.rs", LONE);
    let output = dry4rust(dir.path(), &[]);

    assert!(output.status.success());
    assert_eq!(stdout(&output), "No duplicate candidates found.\n");
}

#[test]
fn an_unparsable_file_is_skipped_without_disturbing_stdout() {
    let dir = duplicate_pair_tree();
    write(dir.path(), "broken.rs", "fn broken( {\n");
    let output = dry4rust(dir.path(), &[]);

    assert!(output.status.success());
    assert_eq!(stdout(&output), TEXT_REPORT);

    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.starts_with("warning: skipping ./broken.rs: "),
        "got: {stderr}"
    );
    assert_eq!(stderr.lines().count(), 1);
}

#[test]
fn usage_errors_exit_non_zero_with_empty_stdout() {
    let dir = duplicate_pair_tree();

    for args in [
        vec!["--nope"],
        vec!["--threshold", "1.5"],
        vec!["--threshold", "nan"],
        vec!["--format", "yaml"],
        vec!["--json", "--text"],
    ] {
        let output = dry4rust(dir.path(), &args);
        assert!(!output.status.success(), "expected failure for {args:?}");
        assert!(
            output.stdout.is_empty(),
            "expected empty stdout for {args:?}"
        );
    }
}

#[test]
fn help_renders_the_defaults() {
    let dir = duplicate_pair_tree();
    let output = dry4rust(dir.path(), &["--help"]);
    let help = stdout(&output);

    assert!(output.status.success());
    assert!(help.contains("[default: 0.75]"), "got: {help}");
    assert!(help.contains("[default: 4]"), "got: {help}");
    assert!(help.contains("[default: 20]"), "got: {help}");
    assert!(help.contains("[default: text]"), "got: {help}");
}
