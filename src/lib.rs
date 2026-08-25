//! `dry4rust` — a tree-edit-distance duplicate-code detector for Rust codebases.
//!
//! Dependencies flow **adapters → core**, never the reverse:
//!
//! - **core** (pure, std-only, side-effect-free): `tree`, `ted`, `similarity`,
//!   `dedup`, `detect`, `model`, `error`.
//! - **adapters** (each confines one third-party crate): `parse` (`syn`),
//!   `discovery` (`ignore`), `report` (`serde`).
//!
//! The public surface is one façade (N36): [`run`] with [`RunOptions`] /
//! [`RunOutput`] / [`Format`], plus the [`error`] types. Everything else
//! (`Candidate`, `Fragment`, `discover_rust_files`, `detect`, `parse`,
//! `render`, …) stays `pub(crate)`: the binary is a separate crate and drives
//! the whole pipeline through [`run`] alone.

pub mod error;

pub(crate) mod dedup;
pub(crate) mod detect;
pub(crate) mod model;
pub(crate) mod similarity;
pub(crate) mod ted;
pub(crate) mod tree;

pub(crate) mod discovery;
pub(crate) mod parse;
pub(crate) mod report;

use std::path::PathBuf;

pub use report::Format;

use crate::error::{Error, Result};
use crate::model::{Analyzed, Fragment};

/// The largest fragment [`run`] will ever admit, whatever
/// [`RunOptions::max_nodes`] asks for.
///
/// `ted`'s DP cells are `u32` (N23/N26) and every cell is bounded by the two
/// trees' combined node count, so the engine's standing requirement is
/// `n + m <= u32::MAX`. Admitting only fragments of at most `u32::MAX / 2`
/// nodes makes that hold for **every** admitted pair — including a pair of
/// two maximal fragments — by construction rather than by assertion, so the
/// invariant survives in release where `debug_assert!` is compiled out.
///
/// The clamp is not a second, hidden ceiling: [`run`] applies it *before*
/// filtering and reports the clamped value in the `warning: skipping …`
/// diagnostic, so the ceiling stated to the user is always the one enforced.
/// It is unreachable from the binary (whose ceiling is 2000 nodes) and from
/// any run that fits in memory; it exists solely to bound the public field.
pub const MAX_SUPPORTED_NODES: usize = u32::MAX as usize / 2;

/// Everything a [`run`] needs: what to scan, the three gates, the oversized-
/// fragment ceiling, and the format.
///
/// Defaults live in the binary's `clap` layer (N29), not here — this struct is
/// deliberately `Default`-free so there is a single source for `0.85/4/20/text`.
///
/// **Pre-1.0 unstable (N41):** the crate has a single consumer (its own
/// binary), so fields are added here as the pipeline grows rather than hidden
/// behind a builder or `#[non_exhaustive]`. Treat any added field as a breaking
/// change for out-of-tree callers until 1.0.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Files and/or directories to scan.
    pub paths: Vec<PathBuf>,
    /// Minimum similarity (inclusive) for a pair to be reported; `[0, 1]`.
    pub threshold: f64,
    /// Minimum source lines a fragment must span to be considered.
    pub min_lines: usize,
    /// Minimum normalized-tree nodes a fragment must have to be considered.
    pub min_nodes: usize,
    /// Maximum normalized-tree nodes a fragment may have to be considered
    /// (inclusive). Anything larger is skipped with a diagnostic — see
    /// [`run`]'s oversized-fragment policy (N23/N30).
    ///
    /// Values above [`MAX_SUPPORTED_NODES`] are clamped to it — the field is
    /// public and unvalidated, so the clamp is what keeps `ted`'s `u32` DP
    /// cells in range in release builds too.
    pub max_nodes: usize,
    /// Output format.
    pub format: Format,
}

/// The result of a successful [`run`], split by output stream.
///
/// Keeping both streams as data (rather than writing them here) leaves `run`
/// side-effect-free and directly assertable; IO happens at the binary's edge.
///
/// **Pre-1.0 unstable (N41):** as with [`RunOptions`], fields may be added
/// (D3's exit-code gate would want a candidate count) without a major bump
/// before 1.0.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The exact bytes to write to stdout — trailing newline included.
    pub report: String,
    /// One-line diagnostics for stderr, in deterministic discovery order.
    /// They never perturb [`RunOutput::report`].
    pub diagnostics: Vec<String>,
    /// Counters describing the work this run did. Diagnostic only — nothing
    /// here is rendered, so [`RunOutput::report`] is unaffected.
    pub stats: RunStats,
}

/// How much work a [`run`] did: the size of the pair loop's input, how many
/// pairs reached TED, and what dedup removed.
///
/// **Why this is public (T12).** The performance envelope is a curve in **F**
/// with `min_nodes` as its input (N96), the per-finding cost multiplier is a
/// TED-evaluation count (N83), and the pre/post-dedup ratio is a corpus
/// statistic D5 could not report because nothing exposed the pre-dedup count
/// (N84c). None of the three is derivable from [`RunOutput::report`], so the
/// measurement needs exactly these four counters — and nothing more.
///
/// They are **counters, not output**: `report` does not render them, so N81's
/// rule (a new *rendered* field must join `dedup`'s clause-4 tie-break) does
/// not apply — there is no per-candidate field here to tie-break on, and the
/// report stays a function of the surviving candidate set alone. Each counter
/// is a deterministic function of the scanned tree and the options, like the
/// report itself.
///
/// **Pre-1.0 unstable (N41)**, as with [`RunOptions`]/[`RunOutput`].
#[derive(Debug, Clone, Copy)]
pub struct RunStats {
    /// Fragments that cleared `min_lines`/`min_nodes` and entered the O(F²)
    /// pair loop — the **F** the envelope is a function of.
    pub fragments: usize,
    /// Pairs actually scored with TED: the ones the size-ratio pre-filter
    /// admitted and the overlap filter kept.
    pub ted_evaluations: usize,
    /// Candidates `detect` produced, before `dedup`.
    pub candidates_before_dedup: usize,
    /// Candidates that survived `dedup` — the number the report renders.
    pub candidates_after_dedup: usize,
}

/// Run the whole pipeline: discover → read + parse → detect → dedup → render.
///
/// Per-file read/parse failures do **not** abort the run (N39, grounded in A8's
/// pure-reporter rule): the file is skipped, a deterministic one-line
/// diagnostic is recorded in [`RunOutput::diagnostics`], and stdout bytes are
/// identical to a run over the same tree without that file. Only a discovery
/// failure (e.g. an unreadable root) or a rendering failure fails the run.
///
/// The same skip-with-diagnostic policy covers **oversized fragments**
/// (N23/N30): TED is super-quadratic in the node counts and the size-ratio
/// pre-filter cannot help two similar-sized giants, so a fragment above
/// [`RunOptions::max_nodes`] is dropped here — before `detect` sees it, so no
/// pair containing it is ever scored — with its own deterministic diagnostic.
/// The ceiling lives in this composition root, not in `detect`, because core is
/// side-effect-free (A5) and this is where the skip policy already lives. The
/// requested ceiling is clamped to [`MAX_SUPPORTED_NODES`] first, and the
/// clamped value is both the one enforced and the one named in the diagnostic.
pub fn run(options: &RunOptions) -> Result<RunOutput> {
    let files = discovery::discover_rust_files(&options.paths)?;
    let ceiling = effective_ceiling(options.max_nodes);

    let mut analyzed: Vec<Analyzed> = Vec::new();
    let mut diagnostics: Vec<String> = Vec::new();
    for path in &files {
        match std::fs::read_to_string(path) {
            Err(err) => diagnostics.push(skipped(path, &err.to_string())),
            Ok(source) => match parse::extract(path, &source) {
                Ok(found) => {
                    for item in found {
                        if item.fragment.node_count > ceiling {
                            diagnostics.push(oversized(&item.fragment, ceiling));
                        } else {
                            analyzed.push(item);
                        }
                    }
                }
                // `extract` only ever yields `Parse`; the fallback keeps the
                // match total without repeating the path the variant embeds.
                Err(Error::Parse { message, .. }) => diagnostics.push(skipped(path, &message)),
                Err(other) => diagnostics.push(skipped(path, &other.to_string())),
            },
        }
    }

    let detected = detect::detect_counted(
        &analyzed,
        &detect::DetectOptions {
            threshold: options.threshold,
            min_lines: options.min_lines,
            min_nodes: options.min_nodes,
        },
    );
    let candidates_before_dedup = detected.candidates.len();
    let candidates = dedup::dedup(detected.candidates);
    let stats = RunStats {
        fragments: detected.fragments,
        ted_evaluations: detected.ted_evaluations,
        candidates_before_dedup,
        candidates_after_dedup: candidates.len(),
    };

    Ok(RunOutput {
        report: report::render(&candidates, options.format)?,
        diagnostics,
        stats,
    })
}

/// The ceiling [`run`] actually enforces: the requested one, capped so that any
/// two admitted fragments satisfy `n + m <= u32::MAX` (see
/// [`MAX_SUPPORTED_NODES`]).
fn effective_ceiling(max_nodes: usize) -> usize {
    max_nodes.min(MAX_SUPPORTED_NODES)
}

/// The one-line stderr diagnostic for a file skipped mid-run (N39).
fn skipped(path: &str, reason: &str) -> String {
    format!("warning: skipping {path}: {reason}")
}

/// The one-line stderr diagnostic for a fragment above the node ceiling
/// (N23/N30). Same shape as [`skipped`], located to the fragment's span in the
/// report's own `path:start-end` form.
fn oversized(fragment: &Fragment, ceiling: usize) -> String {
    skipped(
        &format!(
            "{}:{}-{}",
            fragment.path, fragment.start_line, fragment.end_line
        ),
        &format!(
            "{} nodes exceeds the {ceiling}-node ceiling",
            fragment.node_count
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clamp is what keeps `ted`'s `u32` cells in range in **release**
    /// builds, where `debug_assert!` is gone: whatever a caller puts in the
    /// public `max_nodes` field, two admitted fragments can never sum past
    /// `u32::MAX`. Deleting the `.min(..)` in `effective_ceiling` fails this.
    #[test]
    fn the_enforced_ceiling_keeps_every_admitted_pair_within_a_u32() {
        for requested in [
            0,
            2000,
            MAX_SUPPORTED_NODES - 1,
            MAX_SUPPORTED_NODES,
            MAX_SUPPORTED_NODES + 1,
            u32::MAX as usize,
            usize::MAX,
        ] {
            let ceiling = effective_ceiling(requested);
            // Two maximal admitted fragments — the worst pair `ted` can see.
            assert!(
                ceiling.saturating_mul(2) <= u32::MAX as usize,
                "ceiling {ceiling} admits a pair that overflows the u32 cells"
            );
        }
    }

    /// Truthfulness (the clamp must not become a hidden second ceiling): the
    /// diagnostic names the ceiling that was enforced, and requests at or below
    /// the cap are passed through untouched.
    #[test]
    fn the_enforced_ceiling_is_the_one_reported() {
        assert_eq!(effective_ceiling(2000), 2000);
        assert_eq!(effective_ceiling(usize::MAX), MAX_SUPPORTED_NODES);

        let fragment = Fragment {
            path: "src/a.rs".to_string(),
            start_line: 1,
            end_line: 9,
            kind: model::FragmentKind::Function,
            node_count: 42,
            line_count: 9,
        };
        assert_eq!(
            oversized(&fragment, effective_ceiling(usize::MAX)),
            format!(
                "warning: skipping src/a.rs:1-9: 42 nodes exceeds the {MAX_SUPPORTED_NODES}-node ceiling"
            )
        );
    }
}
