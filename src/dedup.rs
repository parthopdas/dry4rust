//! Containment-dedup policy over candidate pairs (core; pure, std-only).
//!
//! A3 (restated at T9 per N68). Dedup implements exactly one relation:
//! **pair-vs-pair** containment. A pair `P` *dominates* a pair `Q` when, side
//! for side, `Q.left ⊆ P.left` **and** `Q.right ⊆ P.right`; the dominated `Q`
//! is suppressed so the maximal parent wins.
//!
//! The *other* containment relation — **intra-pair**, one pair whose left
//! contains its own right (a fragment against its ancestor, e.g. a
//! single-method `impl` vs that method) — is **not dedup's business**. It is
//! removed at admission, pre-TED, by `detect`'s `spans_overlap` (N61). Dedup
//! never sees such a pair and could not reach it. Conflating the two relations
//! is what let the T8 bug through, so they are kept nominally distinct here.
//!
//! Equality is allowed on a side, and that is load-bearing: two copies of a
//! single-method `impl` produce four findings — `impl↔impl`, `m↔m`, and two
//! `impl↔m` crosses — and the crosses are *strict* on one side and **equal**
//! on the other. Only equality-on-a-side suppresses them (N68).

use crate::model::{Candidate, Fragment};
use std::cmp::Ordering;

/// Suppresses every candidate dominated by another, keeping maximal parents.
///
/// Input order is preserved for the survivors, so the caller's canonical
/// `(left, right)` ordering (which `detect` establishes) carries through
/// unchanged — dedup only ever *removes* pairs.
///
/// **Canonical assignment is a precondition, not something re-derived here.**
/// `Candidate::left` is the canonically-smaller `(path, start_line, end_line)`
/// fragment and `right` the larger (`model`), assigned by `detect` for every
/// pair it emits. That is what makes the side-for-side test correct: an
/// unordered pair `{A, B}` has exactly one representation, so a pair `(A, B)`
/// and a pair `(B, A)` cannot both exist and no side-swapped comparison is
/// needed. Dedup neither re-derives nor re-orders the sides.
///
/// Dominance is a strict partial order — span containment is transitive, and
/// mutual containment forces span equality, which clause 4's score-then-
/// node-count-then-position tie-break breaks. The survivors are therefore
/// exactly the maximal elements, computed in one pass with no cascade: if `P`
/// dominates `Q` and `R` dominates `P`, then `R` dominates `Q` too, so a
/// suppressed pair can never be the only thing suppressing another.
///
/// The **rendered** result is a function of the candidate set, for every score
/// `similarity` can produce. Clause 4's tie-break compares every field `report`
/// emits — score, then the two `node_count`s (rendered as
/// `left_nodes`/`right_nodes`) — before it falls back to incoming position; see
/// [`dominates`] for exactly when that fallback is reached, what may still
/// differ between the survivor and the pair it suppressed, and why `NaN` — the
/// one score that would break the claim — is unreachable.
pub(crate) fn dedup(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut kept = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let dominated = candidates
            .iter()
            .enumerate()
            .any(|(other_index, other)| dominates(other, other_index, candidate, index));
        if !dominated {
            kept.push(candidate.clone());
        }
    }
    kept
}

/// Whether `outer` (at position `outer_index`) dominates `inner` (at
/// `inner_index`) — A3 clause 2, with clause 4's tie-break.
///
/// There is deliberately **no separate `P ≠ Q` guard**. A pair contains itself
/// on both sides with identical keys, so a self-comparison always lands in the
/// identical-span branch, where every value comparison is `Equal` and the final
/// `outer_index < inner_index` is strict ⇒ `false`. Irreflexivity is a property
/// of the tie-break itself, so there is no redundant clause to mutate — an
/// earlier `outer_index != inner_index` guard was exactly that, and no test
/// could distinguish it from the tie-break.
///
/// Clause 4 is reachable from the real pipeline: two *distinct* fragments can
/// share a line span (an `impl` and its only method when they share a closing
/// line), so `detect` can emit several pairs with identical keys. Without a
/// tie-break they would dominate each other and all be suppressed, losing the
/// finding entirely.
///
/// The tie-break is **value-based first**, over every field the report renders
/// (`report`: text = score + spans; json = score + spans + `left_nodes` /
/// `right_nodes`): highest score wins, then the higher `(left, right)` node
/// counts — which are genuinely different for same-span candidates, since a
/// brace-sharing `impl` has exactly one node more than its only method
/// (`parse`). Position is the **last resort** and is reached only when two
/// candidates share span keys *and* their scores do not compare `Greater` or
/// `Less` *and* their node counts are equal.
///
/// For every score `similarity` can actually produce — all of them finite —
/// "does not compare `Greater` or `Less`" means *equal*, so position is only
/// ever reached between candidates that render identically: the survivor and
/// the pair it suppresses may then still differ in `Fragment::kind` and
/// `line_count`, neither of which is emitted (N10/N8) — were either ever
/// emitted, it would have to join this tie-break.
///
/// `NaN` is the one score that would be incomparable *without* being
/// output-equivalent (text renders `NaN` against a finite score even at
/// identical spans and node counts), so it is excluded from that claim on
/// reachability grounds rather than folded into it: `similarity` cannot
/// produce a `NaN`. Every `usize → f64` conversion there is finite, the only
/// zero-denominator case returns `1.0` outright, the denominator is otherwise
/// strictly positive, and the result is clamped to `[0, 1]`.
fn dominates(outer: &Candidate, outer_index: usize, inner: &Candidate, inner_index: usize) -> bool {
    if !(contains(&outer.left, &inner.left) && contains(&outer.right, &inner.right)) {
        return false;
    }
    // Strictly larger on at least one side ⇒ a true parent. Otherwise the two
    // pairs cover the same spans and only the rendered values, then position,
    // separate them.
    if outer.left.canonical_key() != inner.left.canonical_key()
        || outer.right.canonical_key() != inner.right.canonical_key()
    {
        return true;
    }
    match outer.score.partial_cmp(&inner.score) {
        Some(Ordering::Greater) => return true,
        Some(Ordering::Less) => return false,
        // Equal — or incomparable (`NaN`), which no pipeline score is.
        _ => {}
    }
    match (outer.left.node_count, outer.right.node_count)
        .cmp(&(inner.left.node_count, inner.right.node_count))
    {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => outer_index < inner_index,
    }
}

/// Whether `outer` spans `inner`: same file, and `inner`'s line span lies
/// within `outer`'s. Equality is containment (A3 clause 2).
fn contains(outer: &Fragment, inner: &Fragment) -> bool {
    outer.path == inner.path
        && outer.start_line <= inner.start_line
        && inner.end_line <= outer.end_line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{detect, DetectOptions};
    use crate::model::FragmentKind;

    /// A fragment with the given span. Node counts matter to dedup only as
    /// clause 4's tie-break between identical-span candidates (they are
    /// rendered as `left_nodes`/`right_nodes`); dominance itself reasons purely
    /// about `(path, start_line, end_line)`.
    fn fragment(path: &str, start_line: usize, end_line: usize) -> Fragment {
        Fragment {
            path: path.to_string(),
            start_line,
            end_line,
            node_count: 20,
            line_count: end_line - start_line + 1,
            kind: FragmentKind::Function,
        }
    }

    /// A candidate over two synthetic spans. `detect` assigns the sides
    /// canonically; the helper asserts the caller did the same, so no test can
    /// accidentally pin behavior on a non-canonical input.
    fn pair(left: (&str, usize, usize), right: (&str, usize, usize)) -> Candidate {
        let left = fragment(left.0, left.1, left.2);
        let right = fragment(right.0, right.1, right.2);
        assert!(
            left.canonical_key() <= right.canonical_key(),
            "test fixture is not in canonical side order"
        );
        Candidate {
            left,
            right,
            score: 0.96,
        }
    }

    /// Candidates in the canonical `(left, right)` key order `detect` emits.
    fn sorted(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
        candidates.sort_by(|a, b| {
            (a.left.canonical_key(), a.right.canonical_key())
                .cmp(&(b.left.canonical_key(), b.right.canonical_key()))
        });
        candidates
    }

    fn spans(candidates: &[Candidate]) -> Vec<(&str, usize, usize, &str, usize, usize)> {
        candidates
            .iter()
            .map(|candidate| {
                let (left_path, left_start, left_end) = candidate.left.canonical_key();
                let (right_path, right_start, right_end) = candidate.right.canonical_key();
                (
                    left_path,
                    left_start,
                    left_end,
                    right_path,
                    right_start,
                    right_end,
                )
            })
            .collect()
    }

    /// A3 clause 2, the plain case: the nested pair is strictly inside the
    /// outer pair on both sides, so the maximal parent wins.
    #[test]
    fn both_sided_strict_containment_suppresses_the_nested_pair() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let inner = pair(("a.rs", 6, 12), ("b.rs", 6, 12));

        let kept = dedup(sorted(vec![outer.clone(), inner]));

        assert_eq!(spans(&kept), spans(&[outer]));
    }

    /// A3 clause 2's **equality-on-a-side** — the N68 cross. `impl↔m` is
    /// strictly contained on one side and *equal* on the other; if equality
    /// did not count as containment this would read as clause 3 ("keeps
    /// both") and the artifact would survive. Both orientations are pinned.
    #[test]
    fn containment_on_one_side_and_equality_on_the_other_suppresses_the_cross() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        // Equal left, contained right.
        let cross_right = pair(("a.rs", 5, 13), ("b.rs", 6, 12));
        // Contained left, equal right.
        let cross_left = pair(("a.rs", 6, 12), ("b.rs", 5, 13));

        let kept = dedup(sorted(vec![outer.clone(), cross_right, cross_left]));

        assert_eq!(spans(&kept), spans(&[outer]));
    }

    /// A3 clause 3: contained on one side, **disjoint** on the other. The
    /// second pair is a genuine finding about a different fragment, not a
    /// nested echo — it must survive.
    #[test]
    fn containment_on_one_side_and_disjointness_on_the_other_keeps_both() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let elsewhere = pair(("a.rs", 6, 12), ("b.rs", 40, 50));

        let input = sorted(vec![outer, elsewhere]);
        let kept = dedup(input.clone());

        assert_eq!(spans(&kept), spans(&input));
        assert_eq!(kept.len(), 2);
    }

    /// A3 clause 3, the other half: contained on one side, **partially
    /// overlapping** on the other. Partial overlap is containment in neither
    /// direction, so neither pair dominates and both survive.
    #[test]
    fn containment_on_one_side_and_partial_overlap_on_the_other_keeps_both() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let straddling = pair(("a.rs", 6, 12), ("b.rs", 10, 20));

        let input = sorted(vec![outer, straddling]);
        let kept = dedup(input.clone());

        assert_eq!(spans(&kept), spans(&input));
    }

    /// A3 clause 4: identical spans on both sides ⇒ collapse to one, keeping
    /// the **highest-scoring** pair. Mutual containment must not annihilate
    /// both, and the survivor must not depend on the incoming order — so the
    /// same two candidates are fed in both orders. The weaker-scoring pair is
    /// given the *higher* node counts, which pins score **above** node count in
    /// the tie-break order.
    #[test]
    fn identical_span_pairs_collapse_to_the_highest_scoring() {
        let best = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let mut weaker = Candidate {
            score: 0.80,
            ..best.clone()
        };
        weaker.left.node_count += 1;
        weaker.right.node_count += 1;

        for input in [
            vec![best.clone(), weaker.clone()],
            vec![weaker.clone(), best.clone()],
        ] {
            let kept = dedup(input);

            assert_eq!(kept.len(), 1);
            // Node counts point at `weaker`, so only the score ordering can
            // have picked this survivor — score is pinned above node count.
            assert_eq!(kept[0].score, best.score);
        }
    }

    /// A3 clause 4, the tie *below* score: same spans, **equal** scores, but
    /// different node counts — exactly the brace-sharing `impl↔impl` vs `m↔m`
    /// shape, where the `impl` carries one node more per side than its only
    /// method (`parse`). Node counts are rendered (`left_nodes`/`right_nodes`),
    /// so the survivor must be picked by value, not by incoming position; both
    /// orders are fed and must give the same, richer, survivor. Each side is
    /// also varied on its own, since each is a separate rendered field.
    #[test]
    fn identical_span_pairs_with_equal_scores_collapse_to_the_higher_node_counts() {
        for (extra_left, extra_right) in [(1, 1), (1, 0), (0, 1)] {
            let leaner = pair(("a.rs", 1, 7), ("b.rs", 1, 7));
            let mut richer = leaner.clone();
            richer.left.node_count += extra_left;
            richer.right.node_count += extra_right;
            assert_eq!(
                richer.score, leaner.score,
                "the fixture must tie on score, or the node-count tie-break is never reached"
            );

            for input in [
                vec![richer.clone(), leaner.clone()],
                vec![leaner.clone(), richer.clone()],
            ] {
                let kept = dedup(input);

                assert_eq!(kept.len(), 1);
                assert_eq!(
                    (kept[0].left.node_count, kept[0].right.node_count),
                    (richer.left.node_count, richer.right.node_count),
                    "the identical-span survivor must not depend on input order"
                );
            }
        }
    }

    /// Clause 4 is **reachable from `detect`**, not a defensive branch: an
    /// `impl` and its only method share a line span when they share a closing
    /// line, so the four cross-file pairings of two such files all carry the
    /// *same* `(left, right)` key. Without clause 4's tie-break they would
    /// dominate one another and every one of them would be suppressed, losing
    /// the finding outright.
    #[test]
    fn identical_span_pairs_are_reachable_from_detect_and_collapse_to_one() {
        const BRACE_SHARING_IMPL: &str =
            "impl Counter { fn classify(&mut self, flag: bool) -> String {
    self.seen += 1;
    match flag {
        true => String::from(\"yes\"),
        false => String::from(\"no\"),
    }
} }
";
        let mut analyzed = crate::parse::extract("a.rs", BRACE_SHARING_IMPL).expect("parse a.rs");
        analyzed.extend(crate::parse::extract("b.rs", BRACE_SHARING_IMPL).expect("parse b.rs"));

        let found = detect(
            &analyzed,
            &DetectOptions {
                threshold: 0.75,
                min_lines: 4,
                min_nodes: 20,
            },
        );

        // `impl↔impl`, `m↔m` and the two crosses — all four on one key.
        assert_eq!(spans(&found), vec![("a.rs", 1, 7, "b.rs", 1, 7); 4]);

        let kept = dedup(found);
        assert_eq!(spans(&kept), vec![("a.rs", 1, 7, "b.rs", 1, 7)]);
    }

    /// N80 — the one-pass design pinned. `dedup` scans **every** other
    /// candidate, suppressed ones included, so a chain `A ⊃ B ⊃ C` collapses to
    /// `A` alone without a cascade or a fixed point: `C` is suppressed by `A`
    /// directly, not only by the already-suppressed `B`. Every permutation is
    /// fed, since an implementation that consulted only the survivors *so far*
    /// would keep an inner pair whose container comes later in the input.
    #[test]
    fn a_three_deep_containment_chain_collapses_to_the_outermost() {
        let outer = pair(("a.rs", 1, 20), ("b.rs", 1, 20));
        let middle = pair(("a.rs", 5, 15), ("b.rs", 5, 15));
        let inner = pair(("a.rs", 8, 10), ("b.rs", 8, 10));

        for input in [
            vec![outer.clone(), middle.clone(), inner.clone()],
            vec![outer.clone(), inner.clone(), middle.clone()],
            vec![middle.clone(), outer.clone(), inner.clone()],
            vec![middle.clone(), inner.clone(), outer.clone()],
            vec![inner.clone(), middle.clone(), outer.clone()],
            vec![inner.clone(), outer.clone(), middle.clone()],
        ] {
            let kept = dedup(input);

            assert_eq!(
                spans(&kept),
                spans(std::slice::from_ref(&outer)),
                "only the outermost pair may survive a containment chain"
            );
        }
    }

    /// N80, the mixed chain: containment *and* the clause 4 tie-break in one
    /// pass. Two identical-span pairs tie on span and are separated only by
    /// score; the **loser** strictly contains a third pair. The loser is itself
    /// suppressed, so if suppression were allowed to propagate — if a
    /// suppressed pair stopped suppressing, or a pair could only be suppressed
    /// by a survivor found earlier — the inner pair would leak into the output.
    /// It must not: the tie-break winner spans exactly what the loser spans, so
    /// it suppresses the inner pair itself. Every permutation is fed.
    #[test]
    fn the_tie_break_winner_suppresses_what_the_losing_twin_contained() {
        let winner = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let loser = Candidate {
            score: 0.80,
            ..winner.clone()
        };
        let contained = pair(("a.rs", 6, 12), ("b.rs", 6, 12));

        for input in [
            vec![winner.clone(), loser.clone(), contained.clone()],
            vec![loser.clone(), winner.clone(), contained.clone()],
            vec![contained.clone(), loser.clone(), winner.clone()],
            vec![contained.clone(), winner.clone(), loser.clone()],
            vec![loser.clone(), contained.clone(), winner.clone()],
            vec![winner.clone(), contained.clone(), loser.clone()],
        ] {
            let kept = dedup(input);

            assert_eq!(kept.len(), 1);
            assert_eq!(spans(&kept), spans(std::slice::from_ref(&winner)));
            // Pins *which* twin survived: the contained pair is gone because
            // the tie-break winner suppresses it, not because the loser did.
            assert_eq!(kept[0].score, winner.score);
        }
    }

    /// Containment is per-file: identical line spans in *different* files are
    /// never contained, so nothing is suppressed.
    #[test]
    fn spans_in_different_files_are_never_contained() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        // Same line numbers, different files on both sides.
        let elsewhere = pair(("c.rs", 6, 12), ("d.rs", 6, 12));

        let input = sorted(vec![outer, elsewhere]);
        let kept = dedup(input.clone());

        assert_eq!(spans(&kept), spans(&input));
    }

    /// Irreflexivity. A pair contains itself on both sides, so `dominates`
    /// reaches clause 4's tie-break against itself, where all three steps run:
    /// score compares `Equal`, node counts compare `Equal`, and the final
    /// `outer_index < inner_index` is **strict** at an equal position ⇒
    /// `false`. That last strict comparison is the *only* reason nothing
    /// suppresses itself (there is no separate `P ≠ Q` guard) — relax it to a
    /// non-strict one and every pair suppresses itself, and this test says so.
    #[test]
    fn a_pair_does_not_dominate_itself() {
        let only = pair(("a.rs", 5, 13), ("b.rs", 5, 13));

        assert!(!dominates(&only, 0, &only, 0));
        assert_eq!(dedup(vec![only]).len(), 1);
    }

    /// Determinism (golden rule): dedup is a pure function of the pair *set*,
    /// and survivors keep the caller's canonical order. The same pairs fed in
    /// a different order yield the same output, in the same order.
    #[test]
    fn output_order_is_stable_and_independent_of_input_order() {
        let outer = pair(("a.rs", 5, 13), ("b.rs", 5, 13));
        let nested = pair(("a.rs", 6, 12), ("b.rs", 6, 12));
        let unrelated = pair(("x.rs", 1, 9), ("y.rs", 1, 9));
        let disjoint = pair(("a.rs", 6, 12), ("b.rs", 40, 50));

        let forward = dedup(sorted(vec![
            outer.clone(),
            nested.clone(),
            unrelated.clone(),
            disjoint.clone(),
        ]));
        let reversed = dedup(sorted(vec![disjoint, unrelated, nested, outer]));

        assert_eq!(spans(&forward), spans(&reversed));
        // Non-vacuous: three of four survive, in canonical order.
        assert_eq!(forward.len(), 3);
        assert_eq!(spans(&forward), spans(&sorted(forward.clone())));
    }

    /// T10's counts at the core seam (N68/N79). Two files, each holding one
    /// single-method `impl`, duplicated between them: `detect` emits **four**
    /// findings — `impl↔impl`, the two `impl↔m` crosses and `m↔m` — and dedup
    /// leaves **one**, the maximal `impl↔impl`. The end-to-end bytes are
    /// pinned in `tests/facade.rs`; this pins the pre-dedup count, which the
    /// façade cannot observe.
    #[test]
    fn two_copied_single_method_impls_go_from_four_findings_to_one() {
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
        let mut analyzed = crate::parse::extract("a.rs", SINGLE_METHOD_IMPL).expect("parse a.rs");
        analyzed.extend(crate::parse::extract("b.rs", SINGLE_METHOD_IMPL).expect("parse b.rs"));

        let found = detect(
            &analyzed,
            &DetectOptions {
                threshold: 0.75,
                min_lines: 4,
                min_nodes: 20,
            },
        );

        assert_eq!(
            spans(&found),
            vec![
                ("a.rs", 5, 13, "b.rs", 5, 13),
                ("a.rs", 5, 13, "b.rs", 6, 12),
                ("a.rs", 6, 12, "b.rs", 5, 13),
                ("a.rs", 6, 12, "b.rs", 6, 12),
            ],
            "pre-dedup: impl↔impl, two impl↔m crosses, m↔m"
        );

        let kept = dedup(found);
        assert_eq!(spans(&kept), vec![("a.rs", 5, 13, "b.rs", 5, 13)]);
    }
}
