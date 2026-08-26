//! Similarity normalization `sim = 1 − 2δ/(|T₁|+|T₂|+δ)` → [0,1] (core; pure, std-only).
//!
//! Turns the unit-cost tree-edit distance δ from [`crate::ted`] into the score
//! the report emits. The formula is **frozen** (A2, R3): changing it changes
//! the meaning of every reported `score` and of the `--threshold` default.
//!
//! Properties: identical trees (δ = 0) score exactly `1.0`; the score falls
//! monotonically as δ grows and reaches `0.0` when δ equals |T₁|+|T₂| (delete
//! everything, insert everything); it is symmetric in the two node counts; and
//! it is capped by the two trees' size ratio (below). The value is returned raw
//! — rounding for display is the report adapter's job (T6).
//!
//! # Property: the size-ratio identity (N93)
//!
//! Evaluated at the smallest δ two trees of `min`/`max` nodes can have — pure
//! insertion of the surplus, `δ = max − min` (an invariant of
//! [`crate::ted::distance`]) — the formula collapses:
//!
//! > `similarity(max − min, min, max) = 1 − (max − min)/max = min/max`
//!
//! (the denominator is `min + max + (max − min) = 2·max`). So **`sim ≤
//! n_small/n_large` is an identity, not an approximation**: a threshold is
//! *identically* a cap on how far apart two fragments' node counts may be, and
//! that reading is exact for every pair of counts.
//!
//! Its consumer is `detect`'s size-ratio pre-filter, which **cites this
//! property** rather than re-deriving it — and which evaluates the bound by
//! calling this function, so the pre-filter's boundary is bit-identical to the
//! gate's rather than merely algebraically equal. The identity is also what
//! makes that pre-filter **optimal among count-only filters**, not merely
//! admissible: the bound is *attained* by real trees (one tree containing the
//! other's shape edits the surplus in at unit cost), so any filter seeing only
//! the two node counts that pruned more would prune a pair that can genuinely
//! reach the threshold.
//!
//! # Contract: weak monotone non-increase in δ (N56)
//!
//! Monotonicity is not merely a property of the current formula — it is a
//! **contract with a named dependent**: `detect`'s size-ratio pre-filter prunes
//! a pair by evaluating this function at the smallest feasible δ and comparing
//! the result against the threshold. That is sound only while
//! `δ₁ <= δ₂ ⟹ similarity(δ₁, ..) >= similarity(δ₂, ..)` holds *in f64*, not
//! just in exact arithmetic. Changing this function so that it can rise with δ
//! anywhere is therefore a **pruning-soundness change — genuine matches would
//! be dropped before TED ever runs — not just an R3 score change.**

/// Normalized similarity in `[0,1]` for an edit distance and two node counts.
///
/// `delta` is the unit-cost tree-edit distance, `nodes_a` / `nodes_b` the two
/// trees' node counts. Two empty trees (both counts and δ zero) are identical
/// and score `1.0`; the clamp is a belt-and-braces guard — with unit costs
/// δ ≤ |T₁|+|T₂| already keeps the ratio within `[0,1]`.
pub(crate) fn similarity(delta: usize, nodes_a: usize, nodes_b: usize) -> f64 {
    let delta = delta as f64;
    let denominator = nodes_a as f64 + nodes_b as f64 + delta;
    if denominator == 0.0 {
        return 1.0;
    }
    (1.0 - (2.0 * delta) / denominator).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ted::{self, PreparedTree};
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

    /// Local compose of TED + the formula — detect (T5) owns this composition
    /// in production; `similarity.rs` exposes only the frozen formula (N21).
    fn similarity_prepared(a: &PreparedTree, b: &PreparedTree) -> f64 {
        similarity(ted::distance(a, b), a.len(), b.len())
    }

    /// Largest tolerated difference when comparing two computed scores.
    const EPSILON: f64 = 1e-12;

    #[test]
    fn identical_trees_score_exactly_one() {
        assert_eq!(similarity(0, 7, 7), 1.0);
        assert_eq!(similarity(0, 1, 1), 1.0);
        assert_eq!(similarity(0, 0, 0), 1.0);
    }

    #[test]
    fn known_values_match_the_frozen_formula() {
        // 1 − 2·2/(4+5+2) = 1 − 4/11
        assert!((similarity(2, 4, 5) - (1.0 - 4.0 / 11.0)).abs() < EPSILON);
        // 1 − 2·4/(4+1+4) = 1 − 8/9
        assert!((similarity(4, 4, 1) - (1.0 - 8.0 / 9.0)).abs() < EPSILON);
        // 1 − 2·10/(10+10+10) = 1/3
        assert!((similarity(10, 10, 10) - 1.0 / 3.0).abs() < EPSILON);
    }

    #[test]
    fn maximally_different_trees_score_zero() {
        // δ = |T₁|+|T₂| — delete every node, insert every node.
        assert_eq!(similarity(20, 10, 10), 0.0);
        assert_eq!(similarity(2, 1, 1), 0.0);
    }

    #[test]
    fn is_symmetric_in_the_node_counts() {
        for (delta, a, b) in [(3usize, 4usize, 9usize), (1, 2, 30), (7, 11, 5)] {
            assert_eq!(similarity(delta, a, b), similarity(delta, b, a));
        }
    }

    #[test]
    fn stays_within_zero_and_one_and_is_never_nan() {
        for delta in 0..40usize {
            for nodes_a in 0..8usize {
                for nodes_b in 0..8usize {
                    let score = similarity(delta, nodes_a, nodes_b);
                    assert!(!score.is_nan());
                    assert!((0.0..=1.0).contains(&score));
                }
            }
        }
    }

    #[test]
    fn extreme_inputs_neither_overflow_nor_leave_the_range() {
        for (delta, nodes_a, nodes_b) in [
            (1usize, usize::MAX, 0usize),
            (usize::MAX, usize::MAX, usize::MAX),
        ] {
            let score = similarity(delta, nodes_a, nodes_b);
            assert!(score.is_finite() && (0.0..=1.0).contains(&score));
        }
    }

    #[test]
    fn scores_prepared_trees_end_to_end() {
        // Function(Block(Let, Return)) — 4 nodes.
        let tree = NormTree::new(
            FUNCTION,
            vec![NormTree::new(
                BLOCK,
                vec![NormTree::leaf(Label::Let), NormTree::leaf(Label::Return)],
            )],
        );
        let same = PreparedTree::new(&tree);
        assert_eq!(similarity_prepared(&same, &PreparedTree::new(&tree)), 1.0);

        // Against a single unrelated leaf: δ = 4 (one relabel, three deletions).
        let leaf = PreparedTree::new(&NormTree::leaf(Label::Literal));
        let unrelated = similarity_prepared(&same, &leaf);
        assert!((unrelated - (1.0 - 8.0 / 9.0)).abs() < EPSILON);
        // Structurally disjoint shapes trend to zero, and identical is the max.
        assert!(unrelated < 0.25);
        assert!(unrelated < similarity_prepared(&same, &PreparedTree::new(&tree)));
        assert_eq!(unrelated, similarity_prepared(&leaf, &same));
    }
}
