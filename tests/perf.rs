//! Performance envelope harness (T12 — N59/N70/N83/N84c/N96; D9's nested-shape
//! curve folded in at T14).
//!
//! Four of the six measurements are **`#[ignore]`d benchmarks**, not
//! assertions: a wall-clock threshold on a dev machine is a flake, and a number
//! taken under an unknown load is not evidence. They print a table and are run
//! on demand:
//!
//! ```text
//! cargo test --release --test perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is not optional for the timing runs: the default harness
//! runs tests in parallel, and two benchmarks contending for the same cores
//! measure the contention rather than the pipeline.
//!
//! **Re-runnable on a verifier's budget.** The full default run repeats every
//! timed configuration three times over eight curve points and costs the better
//! part of an hour, which defeats the point of committing the harness. Two
//! environment variables, read at run time, make that controllable without
//! changing the defaults:
//!
//! - `DRY4RUST_PERF_REPEATS` — repetitions per timed configuration (default
//!   `3`, floored at `1`). `1` reproduces the deterministic counters exactly;
//!   only the fastest/slowest *spread* is lost.
//! - `DRY4RUST_PERF_MAX_F` — largest requested fragment count in the curve
//!   (default `800`); larger points are skipped, in **both** `min_nodes`
//!   series.
//!
//! So the cheap deterministic counters can be re-checked without paying for the
//! expensive wall-clock points:
//!
//! ```text
//! DRY4RUST_PERF_REPEATS=1 DRY4RUST_PERF_MAX_F=200 \
//!   cargo test --release --test perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! What *is* asserted is deterministic and load-free: the size-ratio pre-filter
//! must keep the number of TED evaluations far below the O(F²) pair count, and
//! a clone site nested `d` deep must cost at most `d²` TED evaluations for one
//! finding (N83). Those are the guardrails — the first fails if the pre-filter
//! is ever disabled or its `break` stops terminating rows — while the timing
//! numbers stay in the feature file's record, pinned per N90.
//!
//! **Everything here is reported as a function of F** (the fragments that clear
//! the floors and enter the pair loop) with `min_nodes` an explicit input, so a
//! later floor change **re-parameterizes** these numbers along F instead of
//! voiding them (N96). It does **not** simply rescale them: the floor also
//! moves the survivors' node-count and shape distributions, which set the
//! constant. See the feature file's *N59/N96 — the curve* section for exactly
//! what the two series do and do not show.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dry4rust::{run, Format, RunOptions, RunOutput};
use tempfile::TempDir;

/// Fragments per generated file — only affects IO shape, never F.
const FRAGMENTS_PER_FILE: usize = 20;

/// The binary's ceiling (`cli::MAX_NODES`), i.e. the largest fragment the
/// pipeline ever admits and therefore the worst pair TED can be asked for.
const MAX_NODES: usize = 2000;

/// How many times each timed configuration is repeated by default; the record
/// reports the range across repeats rather than a single figure. Overridable
/// per run via `DRY4RUST_PERF_REPEATS` so a verifier can re-check the
/// deterministic counters at one repetition.
const DEFAULT_REPEATS: usize = 3;

/// The fragment counts the curve is taken at. Four points spanning 8× is
/// enough to read the *shape* (the deterministic TED-evaluation count settles
/// it exactly); the top point already costs a minute a run, and the cost of
/// another doubling is ~4× that. `DRY4RUST_PERF_MAX_F` trims the expensive tail
/// of this list without touching the cheap points.
const CURVE_POINTS: [usize; 4] = [100, 200, 400, 800];

/// A `usize` knob read from the environment at run time, `default` when unset
/// or unparseable. No new dependency, and no effect on the default run.
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// Repetitions per timed configuration (`DRY4RUST_PERF_REPEATS`), at least one.
fn repeats() -> usize {
    env_usize("DRY4RUST_PERF_REPEATS", DEFAULT_REPEATS).max(1)
}

/// The curve points to measure, capped by `DRY4RUST_PERF_MAX_F`.
fn curve_points() -> Vec<usize> {
    let max_f = env_usize("DRY4RUST_PERF_MAX_F", CURVE_POINTS[CURVE_POINTS.len() - 1]);
    CURVE_POINTS
        .iter()
        .copied()
        .filter(|requested| *requested <= max_f)
        .collect()
}

// --------------------------------------------------------------------------
// Corpus synthesis
// --------------------------------------------------------------------------

/// Statement templates a generated function body is drawn from. Shapes differ
/// structurally (not just in identifiers and literals, which A6 erases), so
/// generated fragments are mostly *unrelated* — a corpus of accidental exact
/// clones would measure the report path, not the pair loop.
const STATEMENTS: [&str; 6] = [
    "    total += values[IDX];\n",
    "    if total > IDX { total -= 1; }\n",
    "    for value in values { total += *value; }\n",
    "    let bound = total * IDX + 1;\n",
    "    while total < IDX { total += 2; }\n",
    "    match total { 0 => total += 1, _ => total -= 1 }\n",
];

/// A deterministic 32-bit LCG — the corpus must be identical on every machine
/// and every run, so no `rand`, no time, no hashing of paths.
fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// A free function of `statements` statements, shapes drawn from [`STATEMENTS`]
/// by the seeded LCG. Node count grows linearly in `statements`.
fn generated_function(index: usize, statements: usize) -> String {
    let mut state = index as u32 + 1;
    let mut source =
        format!("fn generated_{index}(values: &[i32]) -> i32 {{\n    let mut total = 0;\n");
    for step in 0..statements {
        let template = STATEMENTS[(next(&mut state) >> 8) as usize % STATEMENTS.len()];
        source.push_str(&template.replace("IDX", &step.to_string()));
    }
    source.push_str("    total\n}\n");
    source
}

/// A corpus of `fragments` free functions spread over files, with body lengths
/// cycling through 2..14 statements so node counts span a realistic band and
/// the size-ratio pre-filter has something to prune.
fn corpus(fragments: usize) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let mut file = String::new();
    for index in 0..fragments {
        file.push_str(&generated_function(index, 2 + index % 13));
        if (index + 1) % FRAGMENTS_PER_FILE == 0 || index + 1 == fragments {
            let name = format!("gen_{}.rs", index / FRAGMENTS_PER_FILE);
            std::fs::write(dir.path().join(name), &file).expect("write corpus file");
            file.clear();
        }
    }
    dir
}

/// A single function with `statements` sequential statements: **flat** shape —
/// depth ~3, one long run of siblings.
fn flat_fragment(statements: usize) -> String {
    let mut source = String::from("fn flat(values: &[i32]) -> i32 {\n    let mut total = 0;\n");
    for step in 0..statements {
        let _ = writeln!(source, "    total += values[{step}] * {step};");
    }
    source.push_str("    total\n}\n");
    source
}

/// A single function of `levels` nested `if` blocks, each carrying six
/// statements: **nested** shape — depth grows with `levels` while the leaf
/// count stays large. Zhang–Shasha costs `O(n₁·n₂·min(depth₁,leaves₁)·
/// min(depth₂,leaves₂))`, so at equal node counts this is the expensive shape
/// and the flat one is the cheap shape.
fn nested_fragment(levels: usize) -> String {
    let mut source = String::from("fn nested(values: &[i32]) -> i32 {\n    let mut total = 0;\n");
    for level in 0..levels {
        let _ = writeln!(source, "    if total > {level} {{");
        for step in 0..6 {
            let _ = writeln!(source, "    total += values[{step}] * {level};");
        }
    }
    for _ in 0..levels {
        source.push_str("    }\n");
    }
    source.push_str("    total\n}\n");
    source
}

// --------------------------------------------------------------------------
// Measurement helpers
// --------------------------------------------------------------------------

fn options(root: &Path, min_nodes: usize) -> RunOptions {
    RunOptions {
        paths: vec![PathBuf::from(root)],
        // The shipped default (`clap` owns the number — N29); written out
        // explicitly because every recorded measurement is pinned with its full
        // flag set, defaults included (N90).
        threshold: 0.85,
        min_lines: 4,
        min_nodes,
        max_nodes: MAX_NODES,
        format: Format::Text,
    }
}

fn run_on(root: &Path, min_nodes: usize) -> RunOutput {
    run(&options(root, min_nodes)).expect("run should succeed")
}

/// Wall-clock of [`repeats()`] runs, fastest and slowest, plus one run's output.
fn timed(root: &Path, min_nodes: usize) -> (RunOutput, Duration, Duration) {
    let mut elapsed = Vec::new();
    let mut output = None;
    for _ in 0..repeats() {
        let start = Instant::now();
        let result = run_on(root, min_nodes);
        elapsed.push(start.elapsed());
        output = Some(result);
    }
    elapsed.sort();
    let output = output.expect("repeats() is non-zero");
    let last = elapsed[elapsed.len() - 1];
    (output, elapsed[0], last)
}

/// The exact node count of the single fragment in `source`, read out of the
/// oversized-fragment diagnostic: running with a 1-node ceiling skips the
/// fragment and names its node count, so this costs one parse and no TED.
fn node_count_of(source: &str) -> usize {
    let dir = TempDir::new().expect("create temp dir");
    std::fs::write(dir.path().join("one.rs"), source).expect("write file");
    let mut opts = options(dir.path(), 0);
    opts.max_nodes = 1;
    let output = run(&opts).expect("run should succeed");
    let diagnostic = output
        .diagnostics
        .first()
        .expect("the fragment is above the 1-node ceiling")
        .clone();
    let (_, tail) = diagnostic.rsplit_once(": ").expect("diagnostic shape");
    tail.split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .expect("diagnostic names the node count")
}

/// The largest `parameter` for which `build`'s fragment stays at or below
/// [`MAX_NODES`] — the worst pair the pipeline can ever admit for that shape.
fn largest_admitted(build: impl Fn(usize) -> String) -> (usize, String, usize) {
    let (mut low, mut high) = (1usize, 2usize);
    while node_count_of(&build(high)) <= MAX_NODES {
        low = high;
        high *= 2;
    }
    while low + 1 < high {
        let middle = (low + high) / 2;
        if node_count_of(&build(middle)) <= MAX_NODES {
            low = middle;
        } else {
            high = middle;
        }
    }
    let source = build(low);
    let nodes = node_count_of(&source);
    (low, source, nodes)
}

// --------------------------------------------------------------------------
// The guardrail (asserted, deterministic, timing-free)
// --------------------------------------------------------------------------

/// The pre-filter is what makes the O(F²) pair loop survivable, and its effect
/// is a **count**, not a duration: how many of the F(F−1)/2 pairs reach TED.
/// Counting is deterministic and load-independent, so it can be asserted where
/// a wall-clock number cannot.
///
/// This fails if the pre-filter is removed, if its `break` degrades to a
/// `continue` (the N53/N58 coupling), or if the node-count sort that makes the
/// `break` sound is ever dropped.
///
/// F is kept small deliberately: the property is a ratio, so it does not need a
/// large corpus, and this test runs in the **debug** gate where TED is orders
/// of magnitude slower than the release numbers in the record.
#[test]
fn the_size_ratio_pre_filter_keeps_ted_evaluations_far_below_the_pair_count() {
    let dir = corpus(60);
    let stats = run_on(dir.path(), 20).stats;

    let pairs = stats.fragments * (stats.fragments - 1) / 2;
    assert!(stats.fragments > 30, "F = {}", stats.fragments);
    assert!(
        stats.ted_evaluations * 3 < pairs,
        "pre-filter admitted {} of {pairs} pairs at F = {}",
        stats.ted_evaluations,
        stats.fragments
    );
}

// --------------------------------------------------------------------------
// N83 — the per-finding cost multiplier
// --------------------------------------------------------------------------

/// A clone site nested **three** levels deep in EXTENDED's granularity set:
/// `impl` block ⊃ method ⊃ closure. Each level clears the default floors, so
/// each copy of this file contributes three fragments covering the same lines.
const NESTED_CLONE: &str = "struct Counter {
    seen: u32,
}

impl Counter {
    fn classify(&mut self, values: &[i32]) -> i32 {
        let mut total = 0;
        let apply = |value: i32| {
            let scaled = value * 2 + 1;
            if scaled > 10 {
                return scaled - 1;
            }
            scaled + total
        };
        for value in values {
            total += apply(*value);
        }
        total
    }
}
";

/// N83, measured rather than derived: a clone site at nesting depth `d` costs
/// **up to `d²` TED evaluations** to yield **one** surviving finding, and every
/// one of them is paid before `dedup` runs.
///
/// Two copies of a `d = 3` site give `d² = 9` cross-file pairs. The `d·(d−1)`
/// intra-copy pairs cost nothing — they overlap in source and die at admission
/// (N61) — and of the cross-file pairs everything that clears the gate is
/// contained side-for-side, so `dedup` keeps exactly the maximal one.
///
/// `d²` is an **upper** bound, and this fixture shows why: the size-ratio
/// pre-filter also prunes cross-level pairs whose node counts are far apart, so
/// only **5** of the 9 survive here — `impl↔impl`, `method↔method`,
/// `closure↔closure` and the two `impl↔method` crosses; both `impl↔closure`
/// and both `method↔closure` pairs are pruned on size alone. All five clear the
/// gate, so the pre/post-dedup ratio at this site is **5 : 1** (N84c). The
/// pairs the filter *cannot* prune are the ones whose levels are close in size
/// — a single-method `impl` against that method — which is exactly the case
/// where the multiplicity is worst. And the filter cannot move pre-TED: a
/// dominated pair must survive if its dominator fails the score gate.
#[test]
fn a_clone_nested_three_deep_costs_at_most_d_squared_ted_evaluations_for_one_finding() {
    let dir = TempDir::new().expect("create temp dir");
    std::fs::write(dir.path().join("left.rs"), NESTED_CLONE).expect("write file");
    std::fs::write(dir.path().join("right.rs"), NESTED_CLONE).expect("write file");

    let stats = run_on(dir.path(), 20).stats;

    // Three nested fragments per copy — the depth the cost is quadratic in.
    assert_eq!(stats.fragments, 6, "stats: {stats:?}");
    assert!(
        stats.ted_evaluations <= 9,
        "d² = 9 is the ceiling; got {stats:?}"
    );
    assert_eq!(stats.ted_evaluations, 5, "stats: {stats:?}");
    assert_eq!(stats.candidates_before_dedup, 5, "stats: {stats:?}");
    assert_eq!(stats.candidates_after_dedup, 1, "stats: {stats:?}");
}

// --------------------------------------------------------------------------
// The benchmarks (printed, never asserted)
// --------------------------------------------------------------------------

/// N59/N96 — wall-clock as a function of **F**, with `min_nodes` a stated
/// input. Two series over the same corpora at different floors show that the
/// **pair-count exponent is parameterized by F**: the `min_nodes 35` series
/// reproduces Θ(F²) at its own F. The *constant* is not carried over — the
/// floor also shifts the survivors' node-count and shape distributions, and
/// those set the per-TED cost.
#[test]
#[ignore = "benchmark: run explicitly with --release -- --ignored --nocapture"]
fn scaling_curve_over_fragment_count() {
    println!("\nN59/N96 — wall-clock vs F (threshold 0.85, min-lines 4, max-nodes 2000)");
    println!("min_nodes  requested  F     pairs      TED evals  pre-dedup  post-dedup  fastest    slowest");
    for min_nodes in [20, 35] {
        for requested in curve_points() {
            let dir = corpus(requested);
            let (output, fastest, slowest) = timed(dir.path(), min_nodes);
            let stats = output.stats;
            let pairs = stats.fragments * (stats.fragments - 1) / 2;
            println!(
                "{min_nodes:<10} {requested:<10} {:<5} {pairs:<10} {:<10} {:<10} {:<11} {:<10.3?} {:.3?}",
                stats.fragments,
                stats.ted_evaluations,
                stats.candidates_before_dedup,
                stats.candidates_after_dedup,
                fastest,
                slowest
            );
        }
    }
}

/// N84(c) — the pre/post-dedup ratio on a **real** corpus, which the synthetic
/// ones cannot show: they extract only free functions, so nothing nests and
/// dedup has nothing to suppress (ratio `1.00` by construction).
///
/// The corpus is an explicit input — `DRY4RUST_PERF_CORPUS` — and must be a
/// **detached worktree at a pinned sha**, never the live tree: scanning `.`
/// before and after a change compares different corpora, because the new code
/// is itself scanned (N79). The test is skipped when the variable is unset, so
/// nothing here depends on a path that only exists on one machine.
#[test]
#[ignore = "benchmark: run explicitly with --release -- --ignored --nocapture"]
fn dedup_ratio_on_a_pinned_corpus() {
    let Ok(root) = std::env::var("DRY4RUST_PERF_CORPUS") else {
        println!("\nN84(c) — DRY4RUST_PERF_CORPUS unset, skipped");
        return;
    };
    println!("\nN84(c) — pre/post-dedup on {root} (threshold 0.85, min-lines 4, max-nodes 2000)");
    println!("min_nodes  F     TED evals  pre-dedup  post-dedup  fastest    slowest");
    for min_nodes in [20, 35] {
        let (output, fastest, slowest) = timed(Path::new(&root), min_nodes);
        let stats = output.stats;
        println!(
            "{min_nodes:<10} {:<5} {:<10} {:<10} {:<11} {:<10.3?} {:.3?}",
            stats.fragments,
            stats.ted_evaluations,
            stats.candidates_before_dedup,
            stats.candidates_after_dedup,
            fastest,
            slowest
        );
    }
}

/// N70 — the wall-clock of a single maximal pair at the `max_nodes`
/// ceiling. Reported for **both** shapes, because TED cost depends on tree
/// shape and not only on node count: `flat` (one long statement run) and
/// `nested` (deep `if` nesting). The corpus is two files holding the same
/// fragment, so F = 2 and the measured run is one parse pair plus exactly one
/// TED evaluation — asserted, not assumed.
#[test]
#[ignore = "benchmark: run explicitly with --release -- --ignored --nocapture"]
fn worst_admitted_pair_wall_clock() {
    println!("\nN70 — the worst admitted pair (threshold 0.85, min-lines 4, min-nodes 20, max-nodes 2000)");
    println!("shape    parameter  nodes  TED evals  fastest    slowest");
    for (shape, build) in [
        ("flat", &flat_fragment as &dyn Fn(usize) -> String),
        ("nested", &nested_fragment),
    ] {
        let (parameter, source, nodes) = largest_admitted(build);
        let dir = TempDir::new().expect("create temp dir");
        std::fs::write(dir.path().join("left.rs"), &source).expect("write file");
        std::fs::write(dir.path().join("right.rs"), &source).expect("write file");

        let (output, fastest, slowest) = timed(dir.path(), 20);
        assert_eq!(output.stats.ted_evaluations, 1, "expected exactly one pair");
        println!(
            "{shape:<8} {parameter:<10} {nodes:<6} {:<10} {:<10.3?} {:.3?}",
            output.stats.ted_evaluations, fastest, slowest
        );
    }
}

// --------------------------------------------------------------------------
// D9 — the nested-family exponent (optional, non-blocking; folded in at T14)
// --------------------------------------------------------------------------

/// D9 recorded one **cheap missing measurement** and asked for it to be folded
/// into T14's harness rather than reopening T12: N70 timed the nested family
/// **at the ceiling only**, so it measured *one point, not a curve*, and the
/// exponent in that family is unknown. That is why D9's replacement-ceiling
/// arithmetic is a **bracket** (`n²` ⇒ ≈280 nodes, `n⁴` ⇒ ≈740) and why no
/// replacement number is named.
///
/// This takes the same `nested_fragment` generator at ~500 / ~1000 / ~2000
/// nodes, two identical copies each, so every run is F = 2 and **exactly one**
/// TED evaluation — asserted, not assumed. It changes no T12 number: N70's
/// ceiling point stands as recorded, and this only adds the two cheaper points
/// beneath it.
///
/// It lives in `perf.rs` because the shape generators are here; duplicating
/// them into `tests/corpus.rs` to satisfy a filename would be the worse trade.
/// **The ceiling is not changed by this measurement** — D9 is a ruled decision.
#[test]
#[ignore = "benchmark: run explicitly with --release -- --ignored --nocapture"]
fn nested_shape_cost_curve() {
    println!("\nD9 — nested-family cost curve (threshold 0.85, min-lines 4, min-nodes 20, max-nodes 2000)");
    println!("target  levels  nodes  TED evals  fastest    slowest");
    for target in [500usize, 1000, 2000] {
        // Smallest `levels` reaching the target node count; each probe is one
        // parse and no TED.
        let mut levels = 1;
        while node_count_of(&nested_fragment(levels)) < target && levels < 1000 {
            levels += 1;
        }
        let source = nested_fragment(levels);
        let nodes = node_count_of(&source);
        if nodes > MAX_NODES {
            println!("{target:<7} {levels:<7} {nodes:<6} above the ceiling, skipped");
            continue;
        }
        let dir = TempDir::new().expect("create temp dir");
        std::fs::write(dir.path().join("left.rs"), &source).expect("write file");
        std::fs::write(dir.path().join("right.rs"), &source).expect("write file");

        let (output, fastest, slowest) = timed(dir.path(), 20);
        assert_eq!(output.stats.ted_evaluations, 1, "expected exactly one pair");
        println!(
            "{target:<7} {levels:<7} {nodes:<6} {:<10} {:<10.3?} {:.3?}",
            output.stats.ted_evaluations, fastest, slowest
        );
    }
}
