//! Detection orchestration: pairwise compare, filters, threshold gate, and
//! deterministic ordering (core; pure, std-only).
//!
//! The compute seam between `parse` (which produces [`Analyzed`]) and `report`:
//! fragments below the `--min-lines`/`--min-nodes` floors are dropped first,
//! every surviving tree is prepared **once** (N17), each remaining pair is
//! scored with `ted` + `similarity` composed here (N21), and pairs at or above
//! the threshold become [`Candidate`]s ordered by canonical `(left, right)`
//! key. Containment/identical-span dedup is a separate policy (`dedup`).
//!
//! Bridged with `#![allow(dead_code)]` until the CLI (T7) wires the pipeline.
#![allow(dead_code)]

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

/// Scores every admissible fragment pair and returns the candidates that meet
/// `opts.threshold`, sorted by canonical `(left, right)` key.
///
/// Self-pairs are never produced and each unordered pair is scored once. The
/// result is a pure function of the input set — input order does not affect it.
pub(crate) fn detect(analyzed: &[Analyzed], opts: &DetectOptions) -> Vec<Candidate> {
    let mut kept: Vec<&Analyzed> = analyzed
        .iter()
        .filter(|item| passes_floors(&item.fragment, opts))
        .collect();
    kept.sort_by(|a, b| a.fragment.canonical_key().cmp(&b.fragment.canonical_key()));

    // Prepare once per fragment, not once per pair (N17).
    let prepared: Vec<PreparedTree> = kept
        .iter()
        .map(|item| PreparedTree::new(&item.tree))
        .collect();

    let mut candidates = Vec::new();
    for (index, (item_a, tree_a)) in kept.iter().zip(&prepared).enumerate() {
        for (item_b, tree_b) in kept.iter().zip(&prepared).skip(index + 1) {
            let delta = ted::distance(tree_a, tree_b);
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
    candidates
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
            Label::Function,
            vec![NormTree::new(
                Label::Block(BlockKind::Plain),
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
        assert_eq!(parsed.len(), 3);
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
}
