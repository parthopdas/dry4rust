//! Domain types (`Fragment`, `Analyzed`, `Candidate`) (core; pure, std-only).
//!
//! These types carry no third-party derives — serialization lives in the
//! `report` adapter, which owns its own DTOs and maps from these (keeps serde
//! confined). `parse` constructs `Fragment`/`Analyzed`, `detect` constructs
//! `Candidate` (N11: the `dead_code` bridge is no longer needed).

use crate::tree::NormTree;

/// What kind of code element a [`Fragment`] was extracted from.
///
/// EXTENDED granularity grows this in S2 (closures, `impl` bodies, free
/// blocks); S1 only distinguishes free functions from methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FragmentKind {
    /// A free function (`fn foo() { .. }`).
    Function,
    /// A method (inherent, trait-impl, or trait-default body).
    Method,
}

/// An extracted code fragment: a span of source lowered to a normalized tree.
///
/// `node_count` is filled once `parse` (T3) lowers the fragment to its
/// normalized label tree; discovery/model itself never populates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fragment {
    /// Source file path, normalized to `/`-separated form.
    pub(crate) path: String,
    /// First source line of the fragment (1-based, inclusive).
    pub(crate) start_line: usize,
    /// Last source line of the fragment (1-based, inclusive).
    pub(crate) end_line: usize,
    /// Node count of the fragment's normalized tree (filled by `parse`).
    pub(crate) node_count: usize,
    /// Number of source lines the fragment spans.
    pub(crate) line_count: usize,
    /// The kind of element this fragment was extracted from.
    pub(crate) kind: FragmentKind,
}

impl Fragment {
    /// Canonical ordering key `(path, start_line, end_line)`.
    ///
    /// The deterministic-output rule sorts fragments and candidate sides by
    /// this key so results are stable across OS and runs.
    pub(crate) fn canonical_key(&self) -> (&str, usize, usize) {
        (self.path.as_str(), self.start_line, self.end_line)
    }
}

/// A fragment paired with its normalized tree.
///
/// The tree is working state for TED/detect and is deliberately kept off
/// [`Fragment`]; the two travel together here instead. It lives in core (N12)
/// so `detect` consumes only core types — the `parse` adapter constructs it.
#[derive(Debug, Clone)]
pub(crate) struct Analyzed {
    /// The extracted fragment (path/span/counts/kind).
    pub(crate) fragment: Fragment,
    /// The fragment's normalized label tree.
    pub(crate) tree: NormTree,
}

/// A detected near-duplicate pair of fragments with its similarity score.
///
/// `left`/`right` are assigned by [`Fragment::canonical_key`] order (smaller =
/// left) so a pair's presentation is traversal-independent. Node counts are
/// **not** mirrored here (N8) — the report derives them from
/// `left.node_count` / `right.node_count`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    /// The canonically-smaller fragment of the pair.
    pub(crate) left: Fragment,
    /// The canonically-larger fragment of the pair.
    pub(crate) right: Fragment,
    /// Similarity in `[0, 1]` (`sim = 1 − 2δ/(|T₁|+|T₂|+δ)`).
    pub(crate) score: f64,
}
