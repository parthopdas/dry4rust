//! Detection orchestration: pairwise compare, filters, threshold gate, and
//! deterministic ordering (core; pure, std-only).
//!
//! The compute seam between `parse` (which produces [`Analyzed`]) and `report`:
//! fragments below the `--min-lines`/`--min-nodes` floors are dropped first,
//! every surviving tree is prepared **once** (N17), each remaining pair that
//! the size-ratio pre-filter admits and that does not overlap in source
//! ([`spans_overlap`], N61) is scored with `ted` + `similarity` composed here
//! (N21), and pairs at or above the threshold become
//! [`Candidate`]s ordered by canonical `(left, right)` key.
//! Containment/identical-span dedup is a separate policy (`dedup`).

use crate::model::{Analyzed, Candidate, Fragment};
use crate::similarity::similarity;
use crate::ted::{self, PreparedTree};

/// The gates detection applies: the similarity threshold and the two size
/// floors. All three come from CLI flags (T7).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DetectOptions {
    /// Minimum similarity (inclusive) for a pair to be reported.
    pub(crate) threshold: f64,
    /// Minimum `Fragment::line_count` for a fragment to be considered.
    pub(crate) min_lines: usize,
    /// Minimum `Fragment::node_count` for a fragment to be considered.
    pub(crate) min_nodes: usize,
}

/// [`detect`]'s candidates together with the two counters T12 needs to size the
/// performance envelope: **F**, the fragment count that drives the O(F²) pair
/// loop, and how many of those pairs actually reached TED.
///
/// Counting lives here because this is the only place that knows either number:
/// the floors decide F (so `min_nodes` is what sets it — N96) and the
/// size-ratio pre-filter plus the overlap filter decide how many pairs survive
/// to TED (N83). Both are pure functions of the input set, like the candidates.
pub(crate) struct Detected {
    /// The candidates, pre-dedup, sorted by canonical `(left, right)` key.
    pub(crate) candidates: Vec<Candidate>,
    /// Fragments that cleared the floors and entered the pair loop — **F**.
    pub(crate) fragments: usize,
    /// Pairs that reached [`ted::distance`]; the rest were pruned by the
    /// size-ratio pre-filter or dropped by the overlap filter.
    pub(crate) ted_evaluations: usize,
}

/// Scores every admissible fragment pair and returns the candidates that meet
/// `opts.threshold`, sorted by canonical `(left, right)` key.
///
/// Test-only thin wrapper over [`detect_counted`]; production goes through the
/// counted form (`lib::run` reports the counters as [`crate::RunStats`]).
#[cfg(test)]
pub(crate) fn detect(analyzed: &[Analyzed], opts: &DetectOptions) -> Vec<Candidate> {
    detect_counted(analyzed, opts).candidates
}

/// Scores every admissible fragment pair and returns the candidates that meet
/// `opts.threshold`, sorted by canonical `(left, right)` key, alongside the
/// [`Detected`] counters.
///
/// Self-pairs are never produced, each unordered pair is scored once, and a
/// pair whose fragments overlap in source is never scored at all
/// ([`spans_overlap`], N61). The result is a pure function of the input set —
/// input order does not affect it.
///
/// Fragments are walked in **node-count order** so the size-ratio pre-filter
/// ([`best_possible_score`]) can `break` the inner loop instead of testing
/// every pair (N53): with counts ascending, once a partner is too large for the
/// current fragment, every later partner is larger still. Iteration order does
/// not reach the output — the final canonical `(left, right)` sort does.
pub(crate) fn detect_counted(analyzed: &[Analyzed], opts: &DetectOptions) -> Detected {
    let mut kept: Vec<&Analyzed> = analyzed
        .iter()
        .filter(|item| passes_floors(&item.fragment, opts))
        .collect();
    kept.sort_by(|a, b| {
        (a.fragment.node_count, a.fragment.canonical_key())
            .cmp(&(b.fragment.node_count, b.fragment.canonical_key()))
    });
    // N58: the `break` below is sound only because `node_count` is the primary
    // sort key. Free in release, loud in dev/test if a future re-sort drops it.
    debug_assert!(
        kept.windows(2)
            .all(|pair| pair[0].fragment.node_count <= pair[1].fragment.node_count),
        "detect: the pre-filter break requires `kept` in non-decreasing node-count order"
    );

    // Prepare once per fragment, not once per pair (N17).
    let prepared: Vec<PreparedTree> = kept
        .iter()
        .map(|item| PreparedTree::new(&item.tree))
        .collect();

    let mut candidates = Vec::new();
    let mut ted_evaluations = 0usize;
    for (index, (item_a, tree_a)) in kept.iter().zip(&prepared).enumerate() {
        for (item_b, tree_b) in kept.iter().zip(&prepared).skip(index + 1) {
            // Admission mirrors the gate below: `>= opts.threshold`. A rejected
            // partner ends the row — with counts ascending, every later partner
            // is larger still and so rejected too (N53).
            if best_possible_score(item_a.fragment.node_count, item_b.fragment.node_count)
                < opts.threshold
            {
                break;
            }
            // N61: skip the pair, never the rest of the row — overlap says
            // nothing about later partners' node counts, so the `break` above
            // stays the size-ratio rejection's alone.
            if spans_overlap(&item_a.fragment, &item_b.fragment) {
                continue;
            }
            let delta = ted::distance(tree_a, tree_b);
            ted_evaluations += 1;
            let score = similarity(
                delta,
                item_a.fragment.node_count,
                item_b.fragment.node_count,
            );
            if score >= opts.threshold {
                let (left, right) = canonical_sides(&item_a.fragment, &item_b.fragment);
                candidates.push(Candidate {
                    left: left.clone(),
                    right: right.clone(),
                    score,
                });
            }
        }
    }

    // Key order is total and stable; score order would be ambiguous on ties.
    candidates.sort_by(|a, b| {
        (a.left.canonical_key(), a.right.canonical_key())
            .cmp(&(b.left.canonical_key(), b.right.canonical_key()))
    });
    Detected {
        candidates,
        fragments: kept.len(),
        ted_evaluations,
    }
}

/// The highest score a pair with these node counts could possibly reach — the
/// bound behind the admissible size-ratio pre-filter (T11/N57).
///
/// With unit costs, aligning two trees costs at least their size difference
/// (`δ ≥ max − min`), and `similarity` falls monotonically in δ. The pair's
/// best possible score is therefore the score it would get at that minimal δ;
/// the call site admits a pair when `best_possible_score(a, b) >= threshold`,
/// mirroring the gate's `score >= threshold` exactly (and `break`s on the
/// rejected case, which the node-count sort makes final for the row).
///
/// **The bound is evaluated by calling the frozen `similarity` itself** at
/// `δ = max − min`, rather than by comparing the algebraically-equal ratio
/// `min/max` against the threshold. This supersedes the literal N52b form
/// (`min >= threshold * max`) while keeping its intent — never prune on
/// equality — because that form is *not* conservative under f64 rounding:
/// `threshold * (max as f64)` is a rounded product that can land just above
/// `min as f64` for a pair the real gate accepts (e.g. `min = 212`,
/// `max = 685`, `threshold = similarity(473, 212, 685)`), pruning a genuine
/// match. Calling `similarity` removes the discrepancy at the root instead of
/// papering over it with an epsilon:
///
/// - The comparison at the call site and the gate in [`detect`] evaluate the
///   *same* function on the same code path, so the boundary is bit-identical
///   rather than merely close.
/// - `similarity` is weakly monotone non-increasing in δ in f64, not just in
///   exact arithmetic: `2δ` and `min + max + δ` are exactly representable for
///   every reachable node count, so the division is the correctly-rounded
///   image of an exactly-increasing quantity, and correct rounding, `1.0 - r`
///   and `clamp` are all monotone. Hence `score = similarity(δ, ..) <=
///   similarity(max − min, ..) = bound` for every δ ≥ max − min. Both premises
///   are stated as contracts at their source: `δ ≥ max − min` on
///   [`ted::distance`], monotonicity on [`crate::similarity`] (N56).
/// - Therefore `score >= threshold` implies `bound >= threshold`: a pair the
///   gate would accept is always admitted, for **all** node counts and
///   thresholds, with no slack term to justify. The converse is allowed — a
///   false admit merely pays for a TED that then fails the gate.
///
/// The `max == 0` case needs no special guard any more: `similarity(0, 0, 0)`
/// takes the frozen function's own `denominator == 0` branch and returns
/// `1.0`, admitting two empty trees at every threshold in `0.0..=1.0` (N52).
/// That case is *not* production-reachable: `parse` always sets
/// `Fragment::node_count` from `NormTree::node_count`, which is
/// `1 + descendants` and so never zero, and `--min-nodes 0` only relaxes a
/// filter — it cannot conjure a zero-node fragment. Zero reaches this
/// function only by calling it (or `similarity`) directly, so the absence of
/// a guard is a statement about the bound's contract, not about the CLI.
fn best_possible_score(nodes_a: usize, nodes_b: usize) -> f64 {
    let min = nodes_a.min(nodes_b);
    let max = nodes_a.max(nodes_b);
    similarity(max - min, min, max)
}

/// Whether two fragments overlap in source: same file, intersecting line spans.
///
/// EXTENDED extraction (T8) emits nested fragments, so the pair set contains
/// fragments paired with their own **ancestors** — a single-method `impl`
/// against that method, a function against the free block that is its whole
/// body, a closure that is nearly its whole enclosing function. Such a pair is
/// structurally near-identical by construction (`Impl(F)` vs `F` is δ = 1, i.e.
/// `1 − 1/(n+1)` ≈ 0.95 at the default floors) and is a report artifact, not a
/// clone. Overlapping pairs are therefore dropped at admission, before TED
/// (N61) — cheaper than scoring them, and it keeps `dedup`'s containment policy
/// purely *pair-vs-pair* (an outer pair suppressing a nested pair), which is a
/// different relation from this *intra-pair* containment and does not reach it.
///
/// Legitimate in-file clones have disjoint spans and are unaffected.
fn spans_overlap(a: &Fragment, b: &Fragment) -> bool {
    a.path == b.path && a.start_line <= b.end_line && b.start_line <= a.end_line
}

/// Whether a fragment clears both size floors.
fn passes_floors(fragment: &Fragment, opts: &DetectOptions) -> bool {
    fragment.node_count >= opts.min_nodes && fragment.line_count >= opts.min_lines
}

/// Orders a pair's two fragments so `left` is the canonically-smaller one.
fn canonical_sides<'a>(a: &'a Fragment, b: &'a Fragment) -> (&'a Fragment, &'a Fragment) {
    if a.canonical_key() <= b.canonical_key() {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FragmentKind;
    use crate::parse;
    use crate::tree::{BlockKind, Label, NormTree};

    /// Plain-`fn` and plain-block labels; the qualifier and tail flags do not
    /// matter to these fixtures.
    const FUNCTION: Label = Label::Function {
        is_async: false,
        is_const: false,
        is_unsafe: false,
    };
    const BLOCK: Label = Label::Block {
        kind: BlockKind::Plain,
        tail: false,
    };

    /// Options that gate on nothing but the threshold.
    fn opts(threshold: f64) -> DetectOptions {
        DetectOptions {
            threshold,
            min_lines: 0,
            min_nodes: 0,
        }
    }

    /// `Function(Block(Let, Return))` — 4 nodes.
    fn sample_tree() -> NormTree {
        NormTree::new(
            FUNCTION,
            vec![NormTree::new(
                BLOCK,
                vec![NormTree::leaf(Label::Let), NormTree::leaf(Label::Return)],
            )],
        )
    }

    /// An `Analyzed` at `path:start..end` carrying `tree`, with the fragment's
    /// counts derived from the tree and the span (as `parse` derives them).
    fn analyzed(path: &str, start_line: usize, end_line: usize, tree: NormTree) -> Analyzed {
        Analyzed {
            fragment: Fragment {
                path: path.to_string(),
                start_line,
                end_line,
                node_count: tree.node_count(),
                line_count: end_line - start_line + 1,
                kind: FragmentKind::Function,
            },
            tree,
        }
    }

    #[test]
    fn a_single_fragment_yields_no_candidates() {
        let input = [analyzed("a.rs", 1, 4, sample_tree())];
        assert!(detect(&input, &opts(0.0)).is_empty());
    }

    #[test]
    fn never_pairs_a_fragment_with_itself() {
        // Threshold 0.0 admits everything, so any self-pair would show up.
        let input = [
            analyzed("a.rs", 1, 4, sample_tree()),
            analyzed("b.rs", 1, 4, sample_tree()),
        ];
        let found = detect(&input, &opts(0.0));
        assert_eq!(found.len(), 1);
        for candidate in &found {
            assert_ne!(
                candidate.left.canonical_key(),
                candidate.right.canonical_key()
            );
        }
    }

    #[test]
    fn fragments_below_either_floor_are_excluded() {
        let gates = DetectOptions {
            threshold: 0.0,
            min_lines: 4,
            min_nodes: 4,
        };
        let input = [
            // Kept: 4 nodes, 4 lines — exactly at both floors.
            analyzed("keep.rs", 1, 4, sample_tree()),
            analyzed("keep2.rs", 1, 4, sample_tree()),
            // Dropped: 1 node, below `min_nodes`.
            analyzed("thin.rs", 1, 9, NormTree::leaf(Label::Literal)),
            // Dropped: 3 lines, below `min_lines`.
            analyzed("short.rs", 1, 3, sample_tree()),
        ];
        let found = detect(&input, &gates);
        let paths: Vec<&str> = found
            .iter()
            .flat_map(|c| [c.left.path.as_str(), c.right.path.as_str()])
            .collect();
        assert_eq!(paths, vec!["keep.rs", "keep2.rs"]);
    }

    #[test]
    fn the_threshold_gate_is_inclusive_at_the_boundary() {
        // A leaf against the sample scores 1 − 8/9 (δ = 4, |T| = 4 and 1).
        let low = 1.0 - 8.0 / 9.0;
        let input = [
            analyzed("a.rs", 1, 4, sample_tree()),
            analyzed("b.rs", 1, 4, NormTree::leaf(Label::Literal)),
        ];
        assert_eq!(detect(&input, &opts(low)).len(), 1, "== threshold is kept");
        assert!(detect(&input, &opts(low + 1e-9)).is_empty());

        // Identical trees score exactly 1.0, so threshold 1.0 still keeps them.
        let identical = [
            analyzed("a.rs", 1, 4, sample_tree()),
            analyzed("b.rs", 1, 4, sample_tree()),
        ];
        let found = detect(&identical, &opts(1.0));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].score, 1.0);
    }

    #[test]
    fn every_candidate_is_canonically_ordered_whatever_the_input_order() {
        let forwards = [
            analyzed("a.rs", 10, 20, sample_tree()),
            analyzed("a.rs", 1, 4, sample_tree()),
            analyzed("b.rs", 1, 4, sample_tree()),
        ];
        let backwards: Vec<Analyzed> = forwards.iter().rev().cloned().collect();

        for input in [&forwards[..], &backwards[..]] {
            let found = detect(input, &opts(0.0));
            assert_eq!(found.len(), 3);
            for candidate in &found {
                assert!(candidate.left.canonical_key() < candidate.right.canonical_key());
            }
        }
    }

    #[test]
    fn output_is_identical_for_every_input_permutation() {
        let base = vec![
            analyzed("b.rs", 1, 4, sample_tree()),
            analyzed("a.rs", 30, 33, NormTree::leaf(Label::Literal)),
            analyzed("a.rs", 1, 4, sample_tree()),
            analyzed("c.rs", 7, 10, sample_tree()),
        ];
        let expected = detect(&base, &opts(0.0));
        assert_eq!(expected.len(), 6);

        // Every rotation and the reversal must reproduce the same Vec exactly.
        for shift in 1..base.len() {
            let mut shuffled = base.clone();
            shuffled.rotate_left(shift);
            assert_eq!(detect(&shuffled, &opts(0.0)), expected);
        }
        let reversed: Vec<Analyzed> = base.iter().rev().cloned().collect();
        assert_eq!(detect(&reversed, &opts(0.0)), expected);

        // …and that order is ascending by canonical `(left, right)` key.
        assert!(expected.windows(2).all(|w| {
            (w[0].left.canonical_key(), w[0].right.canonical_key())
                <= (w[1].left.canonical_key(), w[1].right.canonical_key())
        }));
    }

    #[test]
    fn prepared_length_matches_the_fragments_node_count_for_parsed_source() {
        // N22: the two sources of |T| — `PreparedTree::len()` (used by TED) and
        // `Fragment.node_count` (reported) — must never diverge.
        let source = r#"
            fn alpha(a: u32) -> u32 { let b = a + 1; b * 2 }
            struct S;
            impl S {
                fn beta(&self, xs: &[u32]) -> u32 {
                    let mut total = 0;
                    for x in xs { total += x; }
                    total
                }
            }
            trait T { fn gamma(&self) -> bool { true } }
        "#;
        let parsed = parse::extract("fixture.rs", source).expect("fixture should parse");
        // Two free-standing bodies (`alpha`, `gamma`), plus the `impl` block
        // and its method `beta` (T8 extraction).
        assert_eq!(parsed.len(), 4);
        for item in &parsed {
            assert_eq!(
                PreparedTree::new(&item.tree).len(),
                item.fragment.node_count
            );
        }
    }

    #[test]
    fn renamed_type_two_clones_score_one_end_to_end() {
        // The feature's central claim: identifiers and literals are
        // canonicalized, so a renamed copy is structurally identical.
        let source = r#"
            fn sum_positive(values: &[i32]) -> i32 {
                let mut total = 0;
                for value in values {
                    if *value > 0 {
                        total += *value;
                    }
                }
                total
            }

            fn add_upbeat(numbers: &[i32]) -> i32 {
                let mut running = 7;
                for number in numbers {
                    if *number > 3 {
                        running += *number;
                    }
                }
                running
            }
        "#;
        let parsed = parse::extract("clones.rs", source).expect("fixture should parse");
        assert_eq!(parsed.len(), 2);

        let found = detect(&parsed, &opts(0.75));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].score, 1.0);
        assert_eq!(found[0].left.start_line, 2);
        assert_eq!(found[0].right.start_line, 12);
    }

    #[test]
    fn structurally_different_functions_are_not_reported() {
        let source = r#"
            fn sum_positive(values: &[i32]) -> i32 {
                let mut total = 0;
                for value in values {
                    if *value > 0 {
                        total += *value;
                    }
                }
                total
            }

            fn describe(flag: bool) -> String {
                match flag {
                    true => String::from("yes"),
                    false => String::from("no"),
                }
            }
        "#;
        let parsed = parse::extract("mixed.rs", source).expect("fixture should parse");
        assert_eq!(parsed.len(), 2);
        assert!(detect(&parsed, &opts(0.75)).is_empty());
    }

    // ---- N61: the intra-pair overlap filter ----

    #[test]
    fn a_single_method_impl_is_not_a_candidate_against_its_own_method() {
        // `Impl(F)` vs `F` is δ = 1 — ~0.95 at the default floors, so without
        // the overlap filter every single-method `impl` reports itself.
        let source = r#"
            struct S;
            impl S {
                fn render(&self, values: &[i32]) -> i32 {
                    let mut total = 0;
                    for value in values {
                        if *value > 0 {
                            total += *value;
                        }
                    }
                    total
                }
            }
        "#;
        let parsed = parse::extract("impl.rs", source).expect("fixture should parse");
        let kinds: Vec<FragmentKind> = parsed.iter().map(|e| e.fragment.kind).collect();
        assert_eq!(
            kinds,
            vec![FragmentKind::ImplBlock, FragmentKind::Method],
            "the fixture must actually produce the ancestor/descendant pair"
        );
        assert!(detect(&parsed, &opts(0.75)).is_empty());
        // …and not merely because the pair scores low: it scores ~0.95.
        let score = similarity(
            ted::distance(
                &PreparedTree::new(&parsed[0].tree),
                &PreparedTree::new(&parsed[1].tree),
            ),
            parsed[0].fragment.node_count,
            parsed[1].fragment.node_count,
        );
        assert!(score > 0.9, "expected a high artifact score, got {score}");
    }

    #[test]
    fn a_nested_closure_is_not_a_candidate_against_its_enclosing_fn() {
        let source = r#"
            fn wrapper(values: &[i32]) -> i32 {
                let scan = |xs: &[i32]| {
                    let mut total = 0;
                    for value in xs {
                        if *value > 0 {
                            total += *value;
                        }
                    }
                    total
                };
                scan(values)
            }
        "#;
        let parsed = parse::extract("closure.rs", source).expect("fixture should parse");
        let kinds: Vec<FragmentKind> = parsed.iter().map(|e| e.fragment.kind).collect();
        assert_eq!(kinds, vec![FragmentKind::Function, FragmentKind::Closure]);
        assert!(detect(&parsed, &opts(0.75)).is_empty());
    }

    #[test]
    fn two_disjoint_in_file_clones_still_pair() {
        // The filter must not over-reach: same file is not enough, the spans
        // must intersect.
        let source = r#"
            fn sum_positive(values: &[i32]) -> i32 {
                let mut total = 0;
                for value in values {
                    if *value > 0 {
                        total += *value;
                    }
                }
                total
            }

            fn add_upbeat(numbers: &[i32]) -> i32 {
                let mut running = 7;
                for number in numbers {
                    if *number > 3 {
                        running += *number;
                    }
                }
                running
            }
        "#;
        let parsed = parse::extract("same_file.rs", source).expect("fixture should parse");
        let found = detect(&parsed, &opts(0.75));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].score, 1.0);
    }

    #[test]
    fn overlap_skips_only_the_pair_and_never_ends_the_row() {
        // `outer` overlaps `inner`, and both are clones of `elsewhere`. Were
        // the skip a `break`, the row would stop at the overlapping partner and
        // lose the pair that follows it in node-count order.
        let gates = DetectOptions {
            threshold: 0.75,
            min_lines: 0,
            min_nodes: 0,
        };
        let input = [
            analyzed("a.rs", 1, 10, sample_tree()),
            analyzed("a.rs", 2, 5, sample_tree()),
            analyzed("b.rs", 1, 4, sample_tree()),
        ];
        let found = detect(&input, &gates);
        let pairs: Vec<String> = found
            .iter()
            .map(|c| format!("{:?}|{:?}", c.left.canonical_key(), c.right.canonical_key()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                format!("{:?}|{:?}", ("a.rs", 1, 10), ("b.rs", 1, 4)),
                format!("{:?}|{:?}", ("a.rs", 2, 5), ("b.rs", 1, 4)),
            ]
        );
        assert_eq!(found, reference_detect(&input, &gates));
    }

    #[test]
    fn the_overlap_predicate_is_symmetric_and_file_scoped() {
        let touching = analyzed("a.rs", 1, 10, sample_tree()).fragment;
        let inside = analyzed("a.rs", 4, 6, sample_tree()).fragment;
        let abutting = analyzed("a.rs", 10, 12, sample_tree()).fragment;
        let after = analyzed("a.rs", 11, 12, sample_tree()).fragment;
        let other_file = analyzed("b.rs", 4, 6, sample_tree()).fragment;

        for (a, b, expected) in [
            (&touching, &inside, true),
            (&touching, &abutting, true), // sharing a single line is overlap
            (&touching, &after, false),
            (&touching, &other_file, false),
            (&touching, &touching, true),
        ] {
            assert_eq!(spans_overlap(a, b), expected, "{a:?} vs {b:?}");
            assert_eq!(spans_overlap(b, a), expected, "{b:?} vs {a:?}");
        }
    }

    // ---- T11: the admissible size-ratio pre-filter ----

    /// The pre-filter as the call site in [`detect`] applies it: keep the pair
    /// when its best possible score still reaches the threshold (N57).
    fn admits(nodes_a: usize, nodes_b: usize, threshold: f64) -> bool {
        best_possible_score(nodes_a, nodes_b) >= threshold
    }

    /// A tree of exactly `nodes` nodes: a `Function` root over a plain block
    /// filled with leaves (`nodes >= 2`).
    fn tree_of(nodes: usize) -> NormTree {
        let leaves = (0..nodes - 2).map(|_| NormTree::leaf(Label::Let)).collect();
        NormTree::new(FUNCTION, vec![NormTree::new(BLOCK, leaves)])
    }

    /// The pre-filter's claim: `sim` can never exceed `min/max`. Checked
    /// against the real TED + similarity composition, not against the algebra.
    #[test]
    fn similarity_never_exceeds_the_size_ratio_bound() {
        for nodes_a in 2..12usize {
            for nodes_b in 2..12usize {
                let (a, b) = (tree_of(nodes_a), tree_of(nodes_b));
                let score = similarity(
                    ted::distance(&PreparedTree::new(&a), &PreparedTree::new(&b)),
                    nodes_a,
                    nodes_b,
                );
                let bound = nodes_a.min(nodes_b) as f64 / nodes_a.max(nodes_b) as f64;
                assert!(
                    score <= bound + 1e-12,
                    "{nodes_a}x{nodes_b}: score {score} exceeds bound {bound}"
                );
            }
        }
    }

    /// Prune-soundness: whenever the predicate prunes, the pair's best possible
    /// score is genuinely below the threshold — so no would-be match is lost.
    #[test]
    fn a_pruned_pair_could_never_have_passed_the_threshold() {
        for nodes_a in 2..12usize {
            for nodes_b in 2..12usize {
                for threshold in [0.0, 0.25, 0.5, 0.75, 0.9, 1.0] {
                    if admits(nodes_a, nodes_b, threshold) {
                        continue;
                    }
                    let (a, b) = (tree_of(nodes_a), tree_of(nodes_b));
                    let score = similarity(
                        ted::distance(&PreparedTree::new(&a), &PreparedTree::new(&b)),
                        nodes_a,
                        nodes_b,
                    );
                    assert!(
                        score < threshold,
                        "{nodes_a}x{nodes_b} pruned at {threshold} but scores {score}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_predicate_keeps_a_pair_whose_bound_equals_the_threshold() {
        // 3/4 == 0.75 exactly: the gate is `>=`, so this pair must survive.
        assert!(admits(3, 4, 0.75));
        assert!(admits(4, 3, 0.75));
        assert!(admits(10, 10, 1.0));
        // Just below the boundary is prunable.
        assert!(!admits(2, 4, 0.75));
        assert!(!admits(9, 10, 1.0));
    }

    #[test]
    fn the_predicate_is_symmetric_and_admits_everything_at_threshold_zero() {
        for (a, b) in [(1usize, 9usize), (4, 5), (7, 7), (0, 6)] {
            assert_eq!(admits(a, b, 0.6), admits(b, a, 0.6), "{a}x{b}");
            assert!(admits(a, b, 0.0), "{a}x{b} at threshold 0");
        }
    }

    /// N52, **defensive/synthetic coverage of the predicate's contract** — not
    /// a production-reachable CLI path. No parse path can yield a zero-node
    /// fragment: `parse::build` sets `Fragment::node_count` from
    /// `NormTree::node_count`, which is `1 + descendants`, and `--min-nodes 0`
    /// merely relaxes a filter. The zero case is reachable only by calling the
    /// predicate (or `similarity`) directly, as this test does. What it pins is
    /// that the predicate does not divide by `max` itself and instead defers to
    /// the frozen `similarity`'s `denominator == 0` branch: strip that branch
    /// and `similarity(0, 0, 0)` is `NaN`, every comparison below goes false,
    /// and the first assertion fails.
    #[test]
    fn the_predicate_contract_admits_zero_node_inputs_defensively() {
        for threshold in [0.0, 0.5, 0.75, 1.0] {
            assert!(admits(0, 0, threshold));
        }
        // A zero-node input against a real fragment cannot score above 0.
        assert!(!admits(0, 5, 0.75));
        assert_eq!(similarity(5, 0, 5), 0.0);
    }

    /// Bhaskar's counterexample to the multiplication form of the predicate:
    /// the smaller tree embeds in the larger, so δ is exactly `max − min` and
    /// the pair scores exactly the threshold — the gate is `>=`, so it is a
    /// real match and must never be pruned. The old
    /// `min >= threshold * max` form computed `0.30948905109489055 * 685.0
    /// == 212.00000000000003` and pruned it.
    #[test]
    fn the_predicate_admits_a_pair_the_gate_accepts_only_by_equality() {
        let (min, max, delta) = (212usize, 685usize, 473usize);
        let threshold = similarity(delta, min, max);

        // The rounded product the superseded form would have compared against.
        assert!(
            (min as f64) < threshold * (max as f64),
            "the multiplication form must still be the unsafe one"
        );
        assert!(
            admits(min, max, threshold),
            "a pair scoring exactly the threshold must be admitted"
        );
        assert!(admits(max, min, threshold));

        // …and the pair really does reach that score end to end, so pruning it
        // would have dropped a genuine match.
        let (a, b) = (tree_of(min), tree_of(max));
        let scored = similarity(
            ted::distance(&PreparedTree::new(&a), &PreparedTree::new(&b)),
            min,
            max,
        );
        assert_eq!(scored, threshold);

        let gates = DetectOptions {
            threshold,
            min_lines: 0,
            min_nodes: 0,
        };
        let pair = [
            analyzed("small.rs", 1, 9, a),
            analyzed("large.rs", 20, 40, b),
        ];
        let found = detect(&pair, &gates);
        assert_eq!(found.len(), 1, "the candidate is retained at equality");
        assert_eq!(found, reference_detect(&pair, &gates));
    }

    /// The unfiltered baseline: score every pair, no pre-filter, no ordering
    /// trick. `detect` must agree with it exactly (T11 is an optimization).
    /// The N61 overlap skip is *semantics*, not an optimization, so it is part
    /// of the baseline too.
    ///
    /// N71: because both sides call the same [`spans_overlap`] helper, any
    /// differential test against this baseline bounds **T11 only** — it cannot
    /// witness an N61 defect, since an error in the helper would move both
    /// sides identically. N61's coverage is the predicate pin
    /// (`the_overlap_predicate_is_symmetric_and_file_scoped`) plus the three
    /// behavioural tests over real extracted fragments.
    fn reference_detect(analyzed: &[Analyzed], opts: &DetectOptions) -> Vec<Candidate> {
        let kept: Vec<&Analyzed> = analyzed
            .iter()
            .filter(|item| passes_floors(&item.fragment, opts))
            .collect();

        let mut candidates = Vec::new();
        for (index, item_a) in kept.iter().enumerate() {
            for item_b in kept.iter().skip(index + 1) {
                if spans_overlap(&item_a.fragment, &item_b.fragment) {
                    continue;
                }
                let delta = ted::distance(
                    &PreparedTree::new(&item_a.tree),
                    &PreparedTree::new(&item_b.tree),
                );
                let score = similarity(
                    delta,
                    item_a.fragment.node_count,
                    item_b.fragment.node_count,
                );
                if score >= opts.threshold {
                    let (left, right) = canonical_sides(&item_a.fragment, &item_b.fragment);
                    candidates.push(Candidate {
                        left: left.clone(),
                        right: right.clone(),
                        score,
                    });
                }
            }
        }
        candidates.sort_by(|a, b| {
            (a.left.canonical_key(), a.right.canonical_key())
                .cmp(&(b.left.canonical_key(), b.right.canonical_key()))
        });
        candidates
    }

    #[test]
    fn filtered_detection_is_identical_to_the_unfiltered_reference() {
        let source = r#"
            fn sum_positive(values: &[i32]) -> i32 {
                let mut total = 0;
                for value in values {
                    if *value > 0 {
                        total += *value;
                    }
                }
                total
            }

            fn add_upbeat(numbers: &[i32]) -> i32 {
                let mut running = 7;
                for number in numbers {
                    if *number > 3 {
                        running += *number;
                    }
                }
                running
            }

            fn describe(flag: bool) -> String {
                match flag {
                    true => String::from("yes"),
                    false => String::from("no"),
                }
            }

            fn tiny() -> u8 { 1 }
        "#;
        let mut fixtures = parse::extract("fixtures.rs", source).expect("fixture should parse");
        // Plus synthetic fragments spanning a wide size range, where the
        // pre-filter actually bites.
        for (index, nodes) in [2usize, 3, 5, 8, 13, 21].into_iter().enumerate() {
            fixtures.push(analyzed(
                "synthetic.rs",
                index * 10 + 1,
                index * 10 + 9,
                tree_of(nodes),
            ));
        }

        // Round thresholds plus awkward, non-exactly-representable ones taken
        // from `similarity` itself at the fixtures' own sizes — the class the
        // multiplication form of the predicate mis-pruned.
        let awkward = [
            similarity(19, 2, 21),
            similarity(13, 8, 21),
            similarity(8, 5, 13),
            similarity(5, 3, 8),
            similarity(3, 2, 5),
            similarity(1, 2, 3),
            similarity(4, 8, 13),
            similarity(7, 13, 21),
        ];
        for threshold in [0.0, 0.1, 0.5, 0.75, 0.9, 0.99, 1.0]
            .into_iter()
            .chain(awkward)
        {
            let gates = DetectOptions {
                threshold,
                min_lines: 0,
                min_nodes: 0,
            };
            assert_eq!(
                detect(&fixtures, &gates),
                reference_detect(&fixtures, &gates),
                "pre-filter changed the result at threshold {threshold}"
            );
        }
    }
}
