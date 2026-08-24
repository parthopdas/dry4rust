//! Domain types (`Fragment`, `Candidate`) (core; pure, std-only).
//!
//! These types carry no third-party derives — serialization lives in the
//! `report` adapter, which owns its own DTOs and maps from these (keeps serde
//! confined). Constructed by later S1 tasks (`parse` → `Fragment`, `detect` →
//! `Candidate`); `#![allow(dead_code)]` bridges the bottom-up build until then.
#![allow(dead_code)]

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

/// A detected near-duplicate pair of fragments with its similarity score.
///
/// `left`/`right` are assigned by [`Fragment::canonical_key`] order (smaller =
/// left) so a pair's presentation is traversal-independent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Candidate {
    /// The canonically-smaller fragment of the pair.
    pub(crate) left: Fragment,
    /// The canonically-larger fragment of the pair.
    pub(crate) right: Fragment,
    /// Similarity in `[0, 1]` (`sim = 1 − 2δ/(|T₁|+|T₂|+δ)`).
    pub(crate) score: f64,
    /// Node count of `left`'s normalized tree.
    pub(crate) left_nodes: usize,
    /// Node count of `right`'s normalized tree.
    pub(crate) right_nodes: usize,
}
