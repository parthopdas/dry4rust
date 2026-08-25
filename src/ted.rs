//! Unit-cost tree-edit distance (Zhang–Shasha) (core; pure, std-only).
//!
//! Computes the classic ordered tree-edit distance δ between two
//! [`NormTree`]s under unit costs: insert = 1, delete = 1, relabel = 0 when the
//! two nodes' [`Label`]s are equal and 1 otherwise. Children order is
//! significant (the normalized tree is source-ordered).
//!
//! # Prepare once, compare many (N17)
//!
//! Zhang–Shasha needs three per-tree arrays: the left-to-right **post-order**
//! node list, the **leftmost-leaf-descendant** index `l(i)` of every node, and
//! the **keyroots**. Detect (T5) runs this over O(n²) fragment pairs, so those
//! arrays are derived **once per tree** into a [`PreparedTree`] and reused
//! across every pair; [`distance`] takes two already-prepared trees.

use crate::tree::{Label, NormTree};

/// A [`NormTree`] flattened into the arrays Zhang–Shasha indexes.
///
/// `labels` and `l` are parallel over the tree's left-to-right post-order:
/// entry `i` describes the `i`-th node visited. Labels are **copied** (N75:
/// `Label` is `Copy` — small, heap-free values) so a prepared tree is
/// self-contained — T5 keeps one per fragment and reuses it across every pair.
#[derive(Debug, Clone)]
pub(crate) struct PreparedTree {
    /// Node labels in post-order.
    labels: Vec<Label>,
    /// `l[i]` — post-order index of node `i`'s leftmost leaf descendant.
    l: Vec<usize>,
    /// Ascending post-order indices of the keyroots: the root plus every node
    /// that is not the leftmost child of its parent (equivalently, the largest
    /// index for each distinct `l` value).
    keyroots: Vec<usize>,
}

impl PreparedTree {
    /// Flattens `tree` into its post-order / `l()` / keyroot arrays.
    pub(crate) fn new(tree: &NormTree) -> Self {
        let mut labels = Vec::new();
        let mut l = Vec::new();
        flatten(tree, &mut labels, &mut l);

        // The keyroots are exactly the largest post-order index per `l` value;
        // walking in increasing order and overwriting yields them sorted.
        let mut largest_per_l: Vec<Option<usize>> = vec![None; l.len()];
        for (index, &leftmost) in l.iter().enumerate() {
            if let Some(slot) = largest_per_l.get_mut(leftmost) {
                *slot = Some(index);
            }
        }
        let mut keyroots: Vec<usize> = largest_per_l.into_iter().flatten().collect();
        // Ascending order is required: the DP for a keyroot pair reads subtree
        // distances settled by smaller keyroots.
        keyroots.sort_unstable();

        Self {
            labels,
            l,
            keyroots,
        }
    }

    /// Number of nodes — always equal to [`NormTree::node_count`] of the tree
    /// this was prepared from. Similarity needs it as |T|.
    pub(crate) fn len(&self) -> usize {
        self.labels.len()
    }
}

/// Appends `tree`'s nodes to `labels` in left-to-right post-order, recording
/// each node's leftmost-leaf-descendant index in `l`. Returns that index for
/// the subtree root.
fn flatten(tree: &NormTree, labels: &mut Vec<Label>, l: &mut Vec<usize>) -> usize {
    let mut leftmost = None;
    for child in &tree.children {
        let child_leftmost = flatten(child, labels, l);
        leftmost.get_or_insert(child_leftmost);
    }
    let index = labels.len();
    labels.push(tree.label);
    let leftmost = leftmost.unwrap_or(index);
    l.push(leftmost);
    leftmost
}

/// Unit-cost tree-edit distance δ between two prepared trees.
///
/// # Invariant (load-bearing for `detect`'s pre-filter — N56)
///
/// Under unit costs, δ is never smaller than the two trees' size difference:
///
/// > `δ >= |len(a) as isize - len(b) as isize|`
///
/// (every surplus node must be inserted or deleted, at cost 1 each), and never
/// larger than `len(a) + len(b)`. The lower bound is what makes
/// [`crate::detect`]'s size-ratio pre-filter *admissible*: it prunes on the
/// score this engine would produce at the smallest feasible δ. **Any
/// replacement TED engine (R1 keeps this module swappable) must preserve that
/// inequality** — breaking it silently turns the pre-filter into a source of
/// dropped matches, not just a slower or differently-scored run.
///
/// The upper bound is why the DP cells below are `u32` (N23/N26): every value
/// is at most `n + m`, so the matrices need half the memory a `usize` cell
/// would take. That representation carries a standing requirement —
/// `n + m <= u32::MAX` — which [`crate::run`] discharges structurally by
/// admitting no fragment larger than [`crate::MAX_SUPPORTED_NODES`]
/// (`u32::MAX / 2`). The representation is internal — the returned δ stays
/// `usize`.
pub(crate) fn distance(a: &PreparedTree, b: &PreparedTree) -> usize {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return n + m;
    }
    // Every cell is bounded by δ ≤ n + m. The façade admits no fragment above
    // `crate::MAX_SUPPORTED_NODES` (= u32::MAX / 2), so `n + m` fits a `u32`
    // for every reachable pair — in release as well as debug. This assertion
    // only catches an in-crate caller that bypassed that clamp.
    debug_assert!(
        n.saturating_add(m) <= u32::MAX as usize,
        "ted: {n} + {m} nodes overflows the u32 cell representation"
    );

    // `tree_dist[i * m + j]` — distance between the subtrees rooted at node `i`
    // of `a` and node `j` of `b`. Filled keyroot pair by keyroot pair in
    // ascending order, so every value a later pair reads is already final.
    let mut tree_dist = vec![0u32; n * m];
    for &i in &a.keyroots {
        for &j in &b.keyroots {
            forest_distance(a, b, i, j, m, &mut tree_dist);
        }
    }
    get(&tree_dist, (n - 1) * m + (m - 1)) as usize
}

/// Prepares both trees and returns their unit-cost tree-edit distance.
///
/// Test-only convenience for one-off comparisons (N25): detect prepares once
/// per fragment and calls [`distance`] instead.
#[cfg(test)]
pub(crate) fn distance_trees(a: &NormTree, b: &NormTree) -> usize {
    distance(&PreparedTree::new(a), &PreparedTree::new(b))
}

/// The Zhang–Shasha forest-distance DP for the keyroot pair `(i, j)`, writing
/// the subtree distances it settles into `tree_dist` (row stride `m`).
fn forest_distance(
    a: &PreparedTree,
    b: &PreparedTree,
    i: usize,
    j: usize,
    m: usize,
    tree_dist: &mut [u32],
) {
    let li = get(&a.l, i);
    let lj = get(&b.l, j);
    // Row/column 0 is the empty forest, so the matrix is one larger than the
    // node ranges `li..=i` and `lj..=j`; forest index `x` is node `li + x - 1`.
    let rows = i - li + 2;
    let cols = j - lj + 2;
    let mut fd = vec![0u32; rows * cols];

    for x in 1..rows {
        let deletions = get(&fd, (x - 1) * cols) + 1;
        set(&mut fd, x * cols, deletions);
    }
    for y in 1..cols {
        let insertions = get(&fd, y - 1) + 1;
        set(&mut fd, y, insertions);
    }

    for x in 1..rows {
        for y in 1..cols {
            let node_a = li + x - 1;
            let node_b = lj + y - 1;
            let delete = get(&fd, (x - 1) * cols + y) + 1;
            let insert = get(&fd, x * cols + (y - 1)) + 1;

            if get(&a.l, node_a) == li && get(&b.l, node_b) == lj {
                // Both forests are single trees: the third option is a relabel,
                // and the result is also the two subtrees' distance.
                let relabel =
                    get(&fd, (x - 1) * cols + (y - 1)) + relabel_cost(a, b, node_a, node_b);
                let best = delete.min(insert).min(relabel);
                set(&mut fd, x * cols + y, best);
                set(tree_dist, node_a * m + node_b, best);
            } else {
                // Otherwise: strip the rightmost tree off each forest and reuse
                // that pair's already-settled subtree distance.
                let rest_x = get(&a.l, node_a) - li;
                let rest_y = get(&b.l, node_b) - lj;
                let split = get(&fd, rest_x * cols + rest_y) + get(tree_dist, node_a * m + node_b);
                set(&mut fd, x * cols + y, delete.min(insert).min(split));
            }
        }
    }
}

/// 0 when the two nodes carry equal labels, 1 otherwise.
fn relabel_cost(a: &PreparedTree, b: &PreparedTree, node_a: usize, node_b: usize) -> u32 {
    match (a.labels.get(node_a), b.labels.get(node_b)) {
        (Some(left), Some(right)) if left == right => 0,
        _ => 1,
    }
}

/// Non-panicking read; every index used above is in range by construction.
fn get<T: Copy + Default>(cells: &[T], index: usize) -> T {
    debug_assert!(index < cells.len(), "ted: read index {index} out of range");
    cells.get(index).copied().unwrap_or_default()
}

/// Non-panicking write; every index used above is in range by construction.
fn set<T>(cells: &mut [T], index: usize, value: T) {
    debug_assert!(index < cells.len(), "ted: write index {index} out of range");
    if let Some(cell) = cells.get_mut(index) {
        *cell = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::BlockKind;

    /// Plain-`fn` and plain-block labels: these trees are distance fixtures, so
    /// the qualifier and tail flags are irrelevant and spelled once.
    const FUNCTION: Label = Label::Function {
        is_async: false,
        is_const: false,
        is_unsafe: false,
    };
    const BLOCK: Label = Label::Block {
        kind: BlockKind::Plain,
        tail: false,
    };

    /// `Function( Block( Let, Return ) )` — 4 nodes.
    fn sample() -> NormTree {
        NormTree::new(
            FUNCTION,
            vec![NormTree::new(
                BLOCK,
                vec![NormTree::leaf(Label::Let), NormTree::leaf(Label::Return)],
            )],
        )
    }

    #[test]
    fn prepared_len_matches_node_count() {
        let tree = sample();
        assert_eq!(PreparedTree::new(&tree).len(), tree.node_count());
        assert_eq!(PreparedTree::new(&tree).len(), 4);

        let leaf = NormTree::leaf(Label::Literal);
        assert_eq!(PreparedTree::new(&leaf).len(), leaf.node_count());
    }

    #[test]
    fn post_order_and_leftmost_leaf_arrays_are_classic() {
        // Post-order of `Function(Block(Let, Return))` is Let, Return, Block,
        // Function; the leftmost leaf of Block and Function is Let (index 0).
        let prepared = PreparedTree::new(&sample());
        assert_eq!(
            prepared.labels,
            vec![Label::Let, Label::Return, BLOCK, FUNCTION,]
        );
        assert_eq!(prepared.l, vec![0, 1, 0, 0]);
        // Keyroots: the root (3) and `Return` (1), which is not a leftmost child.
        assert_eq!(prepared.keyroots, vec![1, 3]);
    }

    #[test]
    fn identical_trees_have_zero_distance() {
        assert_eq!(distance_trees(&sample(), &sample()), 0);
        let leaf = NormTree::leaf(Label::Literal);
        assert_eq!(distance_trees(&leaf, &leaf), 0);
    }

    #[test]
    fn relabel_costs_one_only_when_labels_differ() {
        // Two single-node trees: equal labels cost 0, different labels cost 1.
        let literal = NormTree::leaf(Label::Literal);
        assert_eq!(distance_trees(&literal, &NormTree::leaf(Label::Literal)), 0);
        assert_eq!(distance_trees(&literal, &NormTree::leaf(Label::Path)), 1);
    }

    #[test]
    fn one_leaf_insertion_costs_one() {
        // Block(Let) → Block(Let, Return): insert one leaf.
        let smaller = NormTree::new(BLOCK, vec![NormTree::leaf(Label::Let)]);
        let larger = NormTree::new(
            BLOCK,
            vec![NormTree::leaf(Label::Let), NormTree::leaf(Label::Return)],
        );
        assert_eq!(distance_trees(&smaller, &larger), 1);
        assert_eq!(distance_trees(&larger, &smaller), 1);
    }

    #[test]
    fn multi_operation_case_matches_hand_computation() {
        // Function(Block(Let, Return))            — 4 nodes
        // Function(Block(Path, Return, Literal))  — 5 nodes
        // One relabel (Let → Path) + one insertion (Literal) = 2.
        let other = NormTree::new(
            FUNCTION,
            vec![NormTree::new(
                BLOCK,
                vec![
                    NormTree::leaf(Label::Path),
                    NormTree::leaf(Label::Return),
                    NormTree::leaf(Label::Literal),
                ],
            )],
        );
        assert_eq!(distance_trees(&sample(), &other), 2);
    }

    #[test]
    fn deep_versus_single_leaf_matches_hand_computation() {
        // Function(Params(Param, Param)) — 4 nodes — vs the single leaf Literal:
        // relabel the root (1) + delete the other three (3) = 4.
        let deep = NormTree::new(
            FUNCTION,
            vec![NormTree::new(
                Label::Params,
                vec![NormTree::leaf(Label::Param), NormTree::leaf(Label::Param)],
            )],
        );
        assert_eq!(distance_trees(&deep, &NormTree::leaf(Label::Literal)), 4);
    }

    #[test]
    fn distance_is_symmetric() {
        let pairs = [
            (
                sample(),
                NormTree::new(
                    Label::Params,
                    vec![NormTree::leaf(Label::Param), NormTree::leaf(Label::Param)],
                ),
            ),
            (sample(), NormTree::leaf(Label::Literal)),
            (
                NormTree::new(Label::Call, vec![NormTree::leaf(Label::Path)]),
                NormTree::new(
                    Label::MethodCall,
                    vec![
                        NormTree::leaf(Label::Path),
                        NormTree::new(Label::Call, vec![NormTree::leaf(Label::Literal)]),
                    ],
                ),
            ),
        ];
        for (left, right) in &pairs {
            assert_eq!(distance_trees(left, right), distance_trees(right, left));
        }
    }

    #[test]
    fn distance_never_exceeds_the_combined_node_counts() {
        let left = sample();
        let right = NormTree::new(
            Label::Match,
            vec![NormTree::leaf(Label::Path), NormTree::leaf(Label::MatchArm)],
        );
        assert!(distance_trees(&left, &right) <= left.node_count() + right.node_count());
    }
}
