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

/// A single-method `impl`: `Impl(F)` vs `F` differs by one node, so the
/// `impl`↔method pair scores ~0.95 and would sail through the 0.75 gate.
const SINGLE_METHOD_IMPL: &str = "struct Counter {
    seen: u32,
}

impl Counter {
    fn classify(&mut self, flag: bool) -> String {
        self.seen += 1;
        match flag {
            true => String::from(\"yes\"),
            false => String::from(\"no\"),
        }
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
        // The binary's ceiling (`cli::MAX_NODES`); every fixture here is far
        // below it, so it is inert except in the ceiling test below.
        max_nodes: 2000,
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

/// A7/R6: line endings must not perturb the report. The same fixture written
/// with `\n` and with `\r\n` must produce byte-identical output (CI runs
/// Windows, where `autocrlf` can rewrite checkouts).
#[test]
fn crlf_and_lf_fixtures_produce_byte_identical_reports() {
    let lf = duplicate_pair_tree();
    let crlf = TempDir::new().expect("create temp dir");
    write(crlf.path(), "a.rs", &LEFT.replace('\n', "\r\n"));
    write(crlf.path(), "b.rs", &RIGHT.replace('\n', "\r\n"));

    for (format, marker) in [(Format::Text, "DUPLICATE"), (Format::Json, "\"score\"")] {
        // The only legitimate difference is the temp-dir prefix; strip it so
        // the remaining bytes must match exactly.
        let from_lf = run_on(lf.path(), format)
            .report
            .replace(&prefix(lf.path()), "");
        let from_crlf = run_on(crlf.path(), format)
            .report
            .replace(&prefix(crlf.path()), "");

        // Non-vacuous locally: the LF side must actually report the finding, so
        // a both-sides-empty regression cannot pass this test (N51).
        assert!(
            from_lf.contains(marker),
            "the LF side reported nothing for {format:?}: {from_lf}"
        );

        assert_eq!(
            from_lf, from_crlf,
            "line endings changed the {format:?} report"
        );
    }
}

/// S1 acceptance: `.gitignore`d files and `target/` are skipped end-to-end
/// through the façade, not just inside the discovery adapter (N44). Since
/// `discover_rust_files` is crate-private (N49), this is the integration-level
/// evidence for the skip rules.
#[test]
fn gitignored_and_target_copies_are_skipped_through_the_facade() {
    let dir = duplicate_pair_tree();
    let clean = run_on(dir.path(), Format::Text);

    // Extra copies of `a.rs` that must never be discovered — each would
    // otherwise pair with `a.rs` and `b.rs` and add DUPLICATE blocks.
    write(dir.path(), ".gitignore", "ignored.rs\n");
    write(dir.path(), "ignored.rs", LEFT);
    write(dir.path(), "target/debug/copy.rs", LEFT);
    // A non-`.rs` file is not scanned either.
    write(dir.path(), "README.md", "# docs\n");

    let output = run_on(dir.path(), Format::Text);

    // Strictly stronger than a shape check: the report bytes are exactly the
    // clean baseline's (N51).
    assert_eq!(output.report, clean.report);
    assert!(output.diagnostics.is_empty());
}

/// N23/N30: a fragment above `max_nodes` is dropped before detection, with its
/// own deterministic one-line diagnostic, and the run still succeeds. TED is
/// super-quadratic and the size-ratio pre-filter cannot prune two similar-sized
/// giants, so the ceiling is the only thing bounding a pathological pair.
#[test]
fn an_oversized_fragment_is_skipped_with_a_diagnostic_and_the_run_continues() {
    let dir = duplicate_pair_tree();
    let at = prefix(dir.path());

    // Both fragments are 23 nodes; a ceiling of 10 drops both.
    let mut opts = options(dir.path(), Format::Text);
    opts.max_nodes = 10;
    let output = run(&opts).expect("run should succeed");

    assert_eq!(output.report, "No duplicate candidates found.\n");
    assert_eq!(
        output.diagnostics,
        vec![
            format!("warning: skipping {at}a.rs:1-9: 23 nodes exceeds the 10-node ceiling"),
            format!("warning: skipping {at}b.rs:1-9: 23 nodes exceeds the 10-node ceiling"),
        ]
    );
}

/// The ceiling is inclusive: a fragment exactly at it is still compared, so the
/// clean report is unchanged and nothing is reported to stderr.
#[test]
fn a_fragment_exactly_at_the_ceiling_is_kept() {
    let dir = duplicate_pair_tree();
    let clean = run_on(dir.path(), Format::Text);

    let mut opts = options(dir.path(), Format::Text);
    opts.max_nodes = 23;
    let output = run(&opts).expect("run should succeed");

    assert_eq!(output.report, clean.report);
    assert!(output.diagnostics.is_empty());
}

/// N61: a single-method `impl` must not report a DUPLICATE against its own
/// method — `detect` drops that pair at admission because the two fragments
/// overlap in source. The genuine cross-file clone still reports.
#[test]
fn a_single_method_impl_does_not_pair_with_its_own_method() {
    let dir = duplicate_pair_tree();
    let at = prefix(dir.path());
    write(dir.path(), "wrapper.rs", SINGLE_METHOD_IMPL);

    let output = run_on(dir.path(), Format::Text);
    assert_eq!(
        output.report,
        format!("DUPLICATE score=1.00\n  {at}a.rs:1-9\n  {at}b.rs:1-9\n"),
        "expected exactly the cross-file clone"
    );
    assert!(output.diagnostics.is_empty());
}

/// The separate property (N86d): when that same `impl` *is* duplicated across
/// files, the whole `run` path reports the **maximal** `impl↔impl` pair only.
///
/// Post-dedup expectation, **not** the non-vacuity evidence (N51). Since T9 the
/// four cross-file findings collapse to the maximal `impl↔impl` (N68), so only
/// 5-13 is reported; `!contains(6-12)` would also pass if the 6-12 method
/// stopped being extracted at all, and it is vacuous on its own. The evidence
/// that 6-12 clears the floors and does pair lives at the core seam, in
/// `dedup::tests::two_copied_single_method_impls_go_from_four_findings_to_one`
/// (pre-dedup: four findings, 6-12 among them) — and that is the *only*
/// evidence for 6-12: this probe cannot show it survives, since it would still
/// pass if 6-12 were discarded before detection. What this probe uniquely
/// covers is discovery → file read → the composition root's `max_nodes` filter
/// for the one **observable surviving** finding, i.e. that 5-13 comes through
/// the whole `run` path.
#[test]
fn a_copied_single_method_impl_reports_only_the_maximal_impl_pair() {
    let dir = duplicate_pair_tree();
    let at = prefix(dir.path());
    write(dir.path(), "wrapper.rs", SINGLE_METHOD_IMPL);
    write(dir.path(), "copy.rs", SINGLE_METHOD_IMPL);

    let with_copy = run_on(dir.path(), Format::Text);
    assert!(with_copy.report.contains(&format!("{at}copy.rs:5-13")));
    assert!(!with_copy.report.contains(&format!("{at}copy.rs:6-12")));
}

/// T10 (N68/N79) — the sole end-to-end evidence for A3 clause 2. Two files,
/// each holding **one single-method `impl`**, duplicated between them. Before
/// dedup `detect` emits **four** findings — `impl↔impl`, `m↔m`, and the two
/// `impl↔m` crosses at ~0.96, which are strict on one side and *equal* on the
/// other. After dedup exactly **one** survives, the maximal `impl↔impl`.
///
/// The pre-dedup count of four is not observable through the façade (dedup is
/// inside `run`); it is pinned at the core seam in `src/dedup.rs`. This test
/// pins the post-dedup report **bytes**, which is what a user sees.
#[test]
fn two_copied_single_method_impls_report_one_finding() {
    let single_method_impl = "struct Counter {
    seen: u32,
}

impl Counter {
    fn classify(&mut self, flag: bool) -> String {
        self.seen += 1;
        match flag {
            true => String::from(\"yes\"),
            false => String::from(\"no\"),
        }
    }
}
";
    let dir = TempDir::new().expect("create temp dir");
    let at = prefix(dir.path());
    write(dir.path(), "a.rs", single_method_impl);
    write(dir.path(), "b.rs", single_method_impl);

    let output = run_on(dir.path(), Format::Text);

    // Exactly the `impl↔impl` block pair — the method echo and both
    // cross-granularity crosses are gone.
    assert_eq!(
        output.report,
        format!("DUPLICATE score=1.00\n  {at}a.rs:5-13\n  {at}b.rs:5-13\n")
    );
    assert!(output.diagnostics.is_empty());
}

#[test]
fn a_missing_root_fails_the_whole_run() {
    let dir = TempDir::new().expect("create temp dir");
    let missing = dir.path().join("nope");

    assert!(run(&options(&missing, Format::Text)).is_err());
}
