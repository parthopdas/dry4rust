//! T14 — the third-party corpus harness (N102/N107/N108).
//!
//! One artifact, four consumers: D5-confirmation, N78, N84(a,d,e) and T13 all
//! block on a corpus that is **not our own tree**. Everything here is an
//! `#[ignore]`d measurement, never an assertion: the numbers are corpus- and
//! machine-dependent, and golden rule #8 forbids asserting on a wall clock.
//!
//! **The corpus is never vendored.** The crates are cloned *outside* the
//! working tree and named by sha in `docs/features/001-duplicate-code-detection.md`
//! (T14 record §0.2); this file carries the pins and **enforces** them at run
//! time — a clone whose `HEAD` is not the pinned sha **fails the test, is never
//! measured** — so a number can never be recorded against the wrong tree.
//!
//! ```text
//! # clone once, outside the repo (shas per T14 record §0.2)
//! git clone https://github.com/serde-rs/serde.git      ../_t14-corpus/serde
//! git clone https://github.com/rust-itertools/itertools.git ../_t14-corpus/itertools
//! git clone https://github.com/dtolnay/syn.git         ../_t14-corpus/syn
//!
//! cargo build --release
//! DRY4RUST_CORPUS_ROOT=../_t14-corpus \
//!   cargo test --release --test corpus -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is load-bearing for the timed measurement, exactly as in
//! `tests/perf.rs`: the default parallel harness measures contention between
//! benchmarks rather than the pipeline.
//!
//! **Re-runnable on a verifier's budget** — the same precedent, and the reason
//! for it: T12's full run cost the better part of an hour and its first
//! reproduction attempt was abandoned.
//!
//! - `DRY4RUST_CORPUS_ROOT` — directory holding the clones. **Unset ⇒ every
//!   measurement here skips with a printed note**, so nothing depends on a path
//!   that exists on one machine. **Set, but pointing at a clone off its pinned
//!   sha ⇒ the test fails**: green with nothing measured would silently
//!   invalidate a verification run.
//! - `DRY4RUST_CORPUS_REPEATS` — repetitions per timed configuration (default
//!   `3`, floored at `1`). `3` is what §0.1's *best-of-3* budget form asks for;
//!   `1` reproduces every deterministic number (F, TED evaluations, dedup
//!   counts, scores, shapes) at a third of the cost, losing only the spread.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use dry4rust::{run, Format, RunOptions, RunOutput};

// --------------------------------------------------------------------------
// The pins (T14 record §0.2) — pre-registered before the tool was ever run on
// these crates (N108).
// --------------------------------------------------------------------------

/// One pinned corpus: the crate, the sha it is measured at, and the subtree
/// scanned. `path` is relative to `DRY4RUST_CORPUS_ROOT`.
struct Pinned {
    name: &'static str,
    /// Full sha, verified at run time against the clone's `HEAD`.
    sha: &'static str,
    path: &'static str,
    /// The N108 criterion this crate was chosen to cover.
    criterion: &'static str,
}

const CORPUS: [Pinned; 3] = [
    Pinned {
        name: "serde",
        sha: "a874a1b1bb1cc16cf5ee3b1b7b527af5705742bb",
        path: "serde/serde_core/src",
        criterion: "many small impl blocks",
    },
    Pinned {
        name: "itertools",
        sha: "af6d17d3f4a963c087e81b327b957966e1169ff5",
        path: "itertools/src",
        criterion: "trait-heavy (default bodies)",
    },
    Pinned {
        name: "syn",
        sha: "b5d62a6e43a29418e118b7bcb48e211cefc0154f",
        path: "syn/src",
        criterion: "generated code + scale target",
    },
];

/// The crate the budget verdict is pronounced on (T14 record §0.1): one
/// mid-size crate, default flags, best-of-3.
const SCALE_TARGET: &str = "syn";

/// **Set by the human at T14, deliberately aggressive** (§0.1): he chose `10 s`
/// over `30 s` ("a coin toss") and `60 s` ("likely to pass"), knowing it would
/// probably fail, so that a failure forces optimisation work into S3. This
/// harness **measures against** the number and never tunes anything to meet it.
const BUDGET: Duration = Duration::from_secs(10);

/// The shipped defaults, written out because every recorded measurement is a
/// `(corpus, sha, full flag set)` triple with the defaults spelled out (N90).
const DEFAULT_THRESHOLD: f64 = 0.85;
const DEFAULT_MIN_LINES: usize = 4;
const DEFAULT_MIN_NODES: usize = 20;
const DEFAULT_MAX_NODES: usize = 2000;

/// The threshold the score histogram and the hand-label sample are taken at:
/// the `[0.75, 0.85)` bucket is invisible at the default, and N102(1) requires
/// a *per-bucket* curve. Not a proposal to move any default.
const HISTOGRAM_THRESHOLD: f64 = 0.75;

/// `min_nodes` series for the F-per-kLOC curve: the shipped floor, T13's
/// contested `30-40` estimate, and the `42` at which our own corpus's exact-
/// `1.00` class finally empties (§7c).
const MIN_NODES_SERIES: [usize; 5] = [20, 30, 35, 40, 42];

const DEFAULT_REPEATS: usize = 3;

// --------------------------------------------------------------------------
// Environment
// --------------------------------------------------------------------------

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn repeats() -> usize {
    env_usize("DRY4RUST_CORPUS_REPEATS", DEFAULT_REPEATS).max(1)
}

/// The clone root, or `None` with a printed note — every measurement here is a
/// no-op without it.
fn corpus_root() -> Option<PathBuf> {
    match std::env::var("DRY4RUST_CORPUS_ROOT") {
        Ok(root) => Some(PathBuf::from(root)),
        Err(_) => {
            println!("\nDRY4RUST_CORPUS_ROOT unset — skipped (see this file's module docs)");
            None
        }
    }
}

/// The clone's `HEAD`, or `"unknown"` when `git` cannot answer.
fn head_sha(dir: &Path) -> String {
    Command::new("git")
        .args(["-C"])
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Resolves a pin to its scan path and **enforces the sha**: a clone at any
/// other commit **fails the test**, never measures. A warning would not be
/// enforcement — it scrolls past in a long run and the number is recorded
/// anyway, which is exactly what N90's pins exist to prevent. Failing (rather
/// than skipping) is the difference between an *unset* root and an *explicitly
/// configured* one: unset is a clean skip, but a configured root pointing at a
/// stale clone would otherwise return green with zero measurements and silently
/// invalidate a long verification run.
fn resolve(root: &Path, pinned: &Pinned) -> Option<PathBuf> {
    let clone = root.join(pinned.name);
    let path = root.join(pinned.path);
    if !path.is_dir() {
        println!("  {} — {} not found, skipped", pinned.name, path.display());
        return None;
    }
    let head = head_sha(&clone);
    assert_eq!(
        head,
        pinned.sha,
        "\n  {} — SHA MISMATCH: pinned {}, clone at {} — no number is recorded against an unpinned tree; `git -C {} checkout {}` to measure",
        pinned.name,
        pinned.sha,
        head,
        clone.display(),
        pinned.sha
    );
    Some(path)
}

// --------------------------------------------------------------------------
// Corpus measurement helpers
// --------------------------------------------------------------------------

/// Physical lines across the `.rs` files under `root`.
///
/// Counted here rather than taken from `discovery` (which is `pub(crate)`);
/// the clones are fresh and carry no build output, so the two file sets agree.
/// The file count is printed beside the LOC so a divergence would be visible.
fn loc(root: &Path) -> (usize, usize) {
    let mut files = 0;
    let mut lines = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    files += 1;
                    lines += source.lines().count();
                }
            }
        }
    }
    (files, lines)
}

fn options(root: &Path, threshold: f64, min_nodes: usize, format: Format) -> RunOptions {
    RunOptions {
        paths: vec![root.to_path_buf()],
        threshold,
        min_lines: DEFAULT_MIN_LINES,
        min_nodes,
        max_nodes: DEFAULT_MAX_NODES,
        format,
    }
}

fn run_on(options: &RunOptions) -> RunOutput {
    run(options).expect("run should succeed")
}

/// Wall-clock over [`repeats()`] runs — fastest, slowest, and one run's output.
fn timed(options: &RunOptions) -> (RunOutput, Duration, Duration) {
    let mut elapsed = Vec::new();
    let mut output = None;
    for _ in 0..repeats() {
        let start = Instant::now();
        let result = run_on(options);
        elapsed.push(start.elapsed());
        output = Some(result);
    }
    elapsed.sort();
    let output = output.expect("repeats() is non-zero");
    (output, elapsed[0], elapsed[elapsed.len() - 1])
}

/// One reported finding, read back out of the JSON report — the only way to
/// get at scores and node counts, since `Candidate` is `pub(crate)`.
struct Finding {
    score: f64,
    left: String,
    right: String,
    left_nodes: usize,
    right_nodes: usize,
}

impl Finding {
    /// `min(left, right)`: `min_nodes` gates each fragment independently, so
    /// this is the column that decides whether a floor keeps a pair (N100).
    fn min_nodes(&self) -> usize {
        self.left_nodes.min(self.right_nodes)
    }
}

fn findings(report: &str) -> Vec<Finding> {
    let parsed: serde_json::Value = serde_json::from_str(report).expect("json report");
    parsed["candidates"]
        .as_array()
        .expect("candidates array")
        .iter()
        .map(|candidate| {
            let side = |name: &str| {
                let side = &candidate[name];
                format!(
                    "{}:{}-{}",
                    side["file"].as_str().unwrap_or("?"),
                    side["start_line"].as_u64().unwrap_or(0),
                    side["end_line"].as_u64().unwrap_or(0)
                )
            };
            Finding {
                score: candidate["score"].as_f64().unwrap_or(0.0),
                left: side("left"),
                right: side("right"),
                left_nodes: candidate["left_nodes"].as_u64().unwrap_or(0) as usize,
                right_nodes: candidate["right_nodes"].as_u64().unwrap_or(0) as usize,
            }
        })
        .collect()
}

/// N102(1)'s four score buckets, in order: `[0.75,0.85)`, `[0.85,0.95)`,
/// `[0.95,1.00)`, `=1.00`. `=1.00` is a bucket of its own because T13's
/// central claim is about exactly that population.
const BUCKETS: [&str; 4] = ["[0.75,0.85)", "[0.85,0.95)", "[0.95,1.00)", "=1.00"];

fn bucket_of(score: f64) -> usize {
    if score >= 1.0 {
        3
    } else if score >= 0.95 {
        2
    } else if score >= 0.85 {
        1
    } else {
        0
    }
}

/// Node-count bands for the coincidence null (N102(5)): the shipped floor,
/// T13's contested `30-40` window, and everything above it.
const NODE_BANDS: [&str; 5] = ["20-24", "25-29", "30-39", "40-59", "60+"];

fn node_band_of(nodes: usize) -> usize {
    match nodes {
        0..=24 => 0,
        25..=29 => 1,
        30..=39 => 2,
        40..=59 => 3,
        _ => 4,
    }
}

// --------------------------------------------------------------------------
// N107(1) — F per kLOC on real code, as a function of `min_nodes`
// --------------------------------------------------------------------------

/// The constant nobody had named: T12's envelope is parameterized in **F**
/// while every user has **LOC**, so without F/kLOC the `Θ(F²)` statement cannot
/// answer *"how long on my 80 kLOC crate?"*. Our 134-fragments-for-`src` point
/// is one crate, our own — an anecdote.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn f_per_kloc_by_min_nodes() {
    let Some(root) = corpus_root() else { return };
    println!("\nN107(1) — F per kLOC (threshold 0.85, min-lines 4, max-nodes 2000)");
    for pinned in &CORPUS {
        let Some(path) = resolve(&root, pinned) else {
            continue;
        };
        let (files, lines) = loc(&path);
        println!(
            "\n{} @ {} — {} ({} files, {} LOC)",
            pinned.name,
            &pinned.sha[..8],
            pinned.criterion,
            files,
            lines
        );
        println!("  min_nodes  F       F/kLOC   pairs        TED evals   findings");
        for min_nodes in MIN_NODES_SERIES {
            let output = run_on(&options(&path, DEFAULT_THRESHOLD, min_nodes, Format::Text));
            let stats = output.stats;
            let pairs = stats.fragments * stats.fragments.saturating_sub(1) / 2;
            println!(
                "  {min_nodes:<10} {:<7} {:<8.2} {pairs:<12} {:<11} {}",
                stats.fragments,
                stats.fragments as f64 * 1000.0 / lines as f64,
                stats.ted_evaluations,
                stats.candidates_after_dedup
            );
        }
    }
}

// --------------------------------------------------------------------------
// N107(2) — end-to-end wall-clock against the human's 10 s budget
// --------------------------------------------------------------------------

/// The budget verdict. **Default flags, best-of-3, `--release`, one mid-size
/// crate** (§0.1's stated form), with the fastest of the repeats as the
/// headline — it is the run least contaminated by machine load, the same
/// convention T12 used.
///
/// Nothing here is tuned to meet the number. A clean FAIL against a
/// deliberately hard line is the measurement working, not the measurement
/// failing; where the time goes is read off the T12 counters printed beside it.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn end_to_end_wall_clock_against_the_budget() {
    let Some(root) = corpus_root() else { return };
    println!(
        "\nN107(2) — wall-clock vs the {:?} budget (threshold 0.85, min-lines 4, min-nodes 20, max-nodes 2000; best of {})",
        BUDGET,
        repeats()
    );
    println!("crate       LOC      F      TED evals    findings  fastest     slowest     verdict");
    for pinned in &CORPUS {
        let Some(path) = resolve(&root, pinned) else {
            continue;
        };
        let (_, lines) = loc(&path);
        let (output, fastest, slowest) = timed(&options(
            &path,
            DEFAULT_THRESHOLD,
            DEFAULT_MIN_NODES,
            Format::Text,
        ));
        let stats = output.stats;
        // The verdict is pronounced on the scale target alone; the other two
        // crates are context, because the budget names one mid-size crate.
        let verdict = if pinned.name == SCALE_TARGET {
            if fastest <= BUDGET {
                "PASS (scale target)"
            } else {
                "FAIL (scale target)"
            }
        } else if fastest <= BUDGET {
            "under (context)"
        } else {
            "over (context)"
        };
        println!(
            "{:<11} {lines:<8} {:<6} {:<12} {:<9} {:<11.3?} {:<11.3?} {verdict}",
            pinned.name,
            stats.fragments,
            stats.ted_evaluations,
            stats.candidates_after_dedup,
            fastest,
            slowest
        );
    }
}

// --------------------------------------------------------------------------
// N107(3) — the joint (node count, depth) distribution of admitted fragments
// --------------------------------------------------------------------------

/// D9's revisit trigger, stated as a measurement: are there **real** admitted
/// fragments in the deep-and-large quadrant, or is that shape purely synthetic
/// (N70's 42-nested-`if` adversary)?
///
/// The ceiling prices the node axis only; per-pair cost also scales with depth
/// (**R9**). The histogram is over *admitted* fragments — the ones that cleared
/// the floors and entered the pair loop — because those are the ones whose
/// shape is ever paid for.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn admitted_fragment_shape_histogram() {
    let Some(root) = corpus_root() else { return };
    println!(
        "\nN107(3) — joint (node count, depth) of admitted fragments (threshold 0.85, min-lines 4, min-nodes 20, max-nodes 2000)"
    );
    let node_bands = [
        (20usize, 49usize),
        (50, 99),
        (100, 249),
        (250, 499),
        (500, 2000),
    ];
    let depth_bands = [
        (1usize, 9usize),
        (10, 14),
        (15, 19),
        (20, 29),
        (30, usize::MAX),
    ];
    for pinned in &CORPUS {
        let Some(path) = resolve(&root, pinned) else {
            continue;
        };
        let output = run_on(&options(
            &path,
            DEFAULT_THRESHOLD,
            DEFAULT_MIN_NODES,
            Format::Text,
        ));
        println!(
            "\n{} @ {} — F = {}",
            pinned.name,
            &pinned.sha[..8],
            output.stats.fragments
        );
        print!("  nodes \\ depth ");
        for (low, high) in depth_bands {
            let label = if high == usize::MAX {
                format!("{low}+")
            } else {
                format!("{low}-{high}")
            };
            print!("{label:>8}");
        }
        println!("      max depth");
        for (node_low, node_high) in node_bands {
            print!("  {node_low:>5}-{node_high:<7} ");
            let mut deepest = 0;
            for (depth_low, depth_high) in depth_bands {
                let count = output
                    .admitted
                    .iter()
                    .filter(|shape| {
                        (node_low..=node_high).contains(&shape.node_count)
                            && (depth_low..=depth_high).contains(&shape.depth)
                    })
                    .count();
                print!("{count:>8}");
            }
            for shape in &output.admitted {
                if (node_low..=node_high).contains(&shape.node_count) {
                    deepest = deepest.max(shape.depth);
                }
            }
            println!("      {deepest}");
        }
        let deepest = output.admitted.iter().max_by_key(|shape| shape.depth);
        let largest = output.admitted.iter().max_by_key(|shape| shape.node_count);
        if let (Some(deepest), Some(largest)) = (deepest, largest) {
            println!(
                "  deepest admitted: {} nodes / depth {}   largest admitted: {} nodes / depth {}",
                deepest.node_count, deepest.depth, largest.node_count, largest.depth
            );
        }
    }
}

// --------------------------------------------------------------------------
// N84(c) / N102(1) — dedup ratio and the full score histogram
// --------------------------------------------------------------------------

/// Two numbers the record has never had off our own tree.
///
/// **Pre/post-dedup per crate (N84c).** Our corpus gave `17 pre = 17 post`, and
/// N78/N79 gave the same answer through two other doors — one cause, no nested
/// clone site. The counter-outcome is pre-written in T14's row: `1.00` across
/// several real crates is a **genuine finding**, not a failure to explain away.
///
/// **The full score histogram (N102(1)).** D5's `(0.81, 0.86)` gap was observed
/// on 28 findings; at 0.01 resolution over a real crate it is either a real
/// feature of the score distribution or a small-N artifact.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn score_histogram_and_dedup_ratio() {
    let Some(root) = corpus_root() else { return };
    println!("\nN84(c)/N102(1) — dedup ratio (default flags) and score histogram (threshold 0.75)");
    for pinned in &CORPUS {
        let Some(path) = resolve(&root, pinned) else {
            continue;
        };
        let default = run_on(&options(
            &path,
            DEFAULT_THRESHOLD,
            DEFAULT_MIN_NODES,
            Format::Text,
        ));
        let stats = default.stats;
        let ratio =
            stats.candidates_before_dedup as f64 / stats.candidates_after_dedup.max(1) as f64;
        println!(
            "\n{} @ {} — pre-dedup {} / post-dedup {} = ratio {ratio:.2}",
            pinned.name,
            &pinned.sha[..8],
            stats.candidates_before_dedup,
            stats.candidates_after_dedup
        );

        let histogram = run_on(&options(
            &path,
            HISTOGRAM_THRESHOLD,
            DEFAULT_MIN_NODES,
            Format::Json,
        ));
        let found = findings(&histogram.report);
        println!("  {} findings at threshold 0.75", found.len());
        print!("  score histogram (0.01 bins):");
        for bin in 75..100 {
            let low = bin as f64 / 100.0;
            let count = found
                .iter()
                .filter(|finding| finding.score >= low && finding.score < low + 0.01)
                .count();
            if bin % 5 == 0 {
                print!("\n   ");
            }
            print!(" {low:.2}:{count:<5}");
        }
        let exact = found.iter().filter(|finding| finding.score >= 1.0).count();
        println!("\n    1.00:{exact}");

        println!("  per bucket, and the node-count distribution T13 needs:");
        println!("    bucket       n      min-nodes: median  min  max");
        for (index, label) in BUCKETS.iter().enumerate() {
            let mut nodes: Vec<usize> = found
                .iter()
                .filter(|finding| bucket_of(finding.score) == index)
                .map(Finding::min_nodes)
                .collect();
            nodes.sort_unstable();
            let median = nodes.get(nodes.len() / 2).copied().unwrap_or(0);
            println!(
                "    {label:<12} {:<6} {median:<17} {:<4} {}",
                nodes.len(),
                nodes.first().copied().unwrap_or(0),
                nodes.last().copied().unwrap_or(0)
            );
        }
    }
}

// --------------------------------------------------------------------------
// N102(1)/(4) — the deterministic hand-label sample
// --------------------------------------------------------------------------

/// Emits the pairs to hand-label, sampled **deterministically** — every
/// `ceil(n/N)`-th finding in the run's own canonical `(left, right)` order — so
/// the sample is reproducible from the pin alone and carries no labeller
/// discretion. `N` is [`SAMPLE_PER_BUCKET`] (§0.4).
///
/// The rubric is "would I factor these out?" (actionable), plus R8's second
/// column: does the tool's **evidence** — the score and node counts, computed
/// over an identifier-, literal- and macro-erased tree — match the reason a
/// human would give (attributed)? Unsure is a label, not a coin toss.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn hand_label_sample() {
    const SAMPLE_PER_BUCKET: usize = 8;
    let Some(root) = corpus_root() else { return };
    println!(
        "\nN102(1) — hand-label sample, {SAMPLE_PER_BUCKET} per bucket (threshold 0.75, min-lines 4, min-nodes 20, max-nodes 2000)"
    );
    for pinned in &CORPUS {
        let Some(path) = resolve(&root, pinned) else {
            continue;
        };
        let output = run_on(&options(
            &path,
            HISTOGRAM_THRESHOLD,
            DEFAULT_MIN_NODES,
            Format::Json,
        ));
        let found = findings(&output.report);
        println!("\n{} @ {}", pinned.name, &pinned.sha[..8]);
        for (index, label) in BUCKETS.iter().enumerate() {
            let bucket: Vec<&Finding> = found
                .iter()
                .filter(|finding| bucket_of(finding.score) == index)
                .collect();
            let stride = bucket.len().div_ceil(SAMPLE_PER_BUCKET).max(1);
            println!("  {label} — n = {}, stride {stride}", bucket.len());
            for finding in bucket.iter().step_by(stride).take(SAMPLE_PER_BUCKET) {
                println!(
                    "    {:.4}  {}/{:<3}  {} <-> {}",
                    finding.score,
                    finding.left_nodes,
                    finding.right_nodes,
                    finding.left,
                    finding.right
                );
            }
        }
    }
}

// --------------------------------------------------------------------------
// The zero-labelling protocol (§8, N102(5))
// --------------------------------------------------------------------------

/// Cross-crate hits read the **coincidence null** directly: two unrelated
/// crates share no code, so every cross-crate finding is non-actionable by
/// construction and no human judgement enters the loop.
///
/// Kept as §8 wrote it — per-kLOC at `>=0.75`, `>=0.85` and `=1.00` — with
/// **no** `attributed` column (attribution is undefined when every hit is a
/// false positive by construction) and **one** added axis: the same histogram
/// bucketed by `min(left_nodes, right_nodes)`, which prices T13's floor against
/// the null mechanically.
#[test]
#[ignore = "measurement: run explicitly with --release -- --ignored --nocapture"]
fn cross_crate_coincidence_histogram() {
    let Some(root) = corpus_root() else { return };
    let (Some(first), Some(second)) = (CORPUS.first(), CORPUS.get(1)) else {
        return;
    };
    let (Some(left_path), Some(right_path)) = (resolve(&root, first), resolve(&root, second))
    else {
        return;
    };
    let (_, left_loc) = loc(&left_path);
    let (_, right_loc) = loc(&right_path);
    let kloc = (left_loc + right_loc) as f64 / 1000.0;
    println!(
        "\n§8/N102(5) — cross-crate coincidence null: {} x {} ({:.1} kLOC, threshold 0.75, min-lines 4, min-nodes 20, max-nodes 2000)",
        first.name, second.name, kloc
    );

    let mut opts = options(
        &left_path,
        HISTOGRAM_THRESHOLD,
        DEFAULT_MIN_NODES,
        Format::Json,
    );
    opts.paths.push(right_path.clone());
    let output = run_on(&opts);
    let found = findings(&output.report);

    // A finding is cross-crate when its two sides live under different pinned
    // subtrees. Report paths are `/`-normalized, so the pins are matched in the
    // same form.
    let needle = |pinned: &Pinned| pinned.path.replace('\\', "/");
    let (left_needle, right_needle) = (needle(first), needle(second));
    let side_of = |location: &str| {
        if location.contains(&left_needle) {
            0
        } else if location.contains(&right_needle) {
            1
        } else {
            2
        }
    };
    let cross: Vec<&Finding> = found
        .iter()
        .filter(|finding| side_of(&finding.left) != side_of(&finding.right))
        .collect();

    println!(
        "  {} findings total, {} of them cross-crate",
        found.len(),
        cross.len()
    );
    println!("  cut-off   cross-crate hits   per kLOC");
    for (label, keep) in [(">=0.75", 0.75f64), (">=0.85", 0.85), ("=1.00", 1.0)] {
        let count = cross.iter().filter(|finding| finding.score >= keep).count();
        println!("  {label:<9} {count:<18} {:.3}", count as f64 / kloc);
    }

    println!("  the added axis (N102(5)) — cross-crate hits by min(left,right) nodes:");
    println!("    nodes    >=0.75   >=0.85   =1.00");
    for (index, band) in NODE_BANDS.iter().enumerate() {
        let in_band = |finding: &&&Finding| node_band_of(finding.min_nodes()) == index;
        let count = |keep: f64| {
            cross
                .iter()
                .filter(in_band)
                .filter(|finding| finding.score >= keep)
                .count()
        };
        println!(
            "    {band:<8} {:<8} {:<8} {}",
            count(0.75),
            count(0.85),
            count(1.0)
        );
    }
}
