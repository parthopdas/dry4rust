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
use crate::model::Analyzed;

/// Everything a [`run`] needs: what to scan, the three gates, and the format.
///
/// Defaults live in the binary's `clap` layer (N29), not here — this struct is
/// deliberately `Default`-free so there is a single source for `0.75/4/20/text`.
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
    /// Output format.
    pub format: Format,
}

/// The result of a successful [`run`], split by output stream.
///
/// Keeping both streams as data (rather than writing them here) leaves `run`
/// side-effect-free and directly assertable; IO happens at the binary's edge.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The exact bytes to write to stdout — trailing newline included.
    pub report: String,
    /// One-line diagnostics for stderr, in deterministic discovery order.
    /// They never perturb [`RunOutput::report`].
    pub diagnostics: Vec<String>,
}

/// Run the whole pipeline: discover → read + parse → detect → render.
///
/// Per-file read/parse failures do **not** abort the run (N39, grounded in A8's
/// pure-reporter rule): the file is skipped, a deterministic one-line
/// diagnostic is recorded in [`RunOutput::diagnostics`], and stdout bytes are
/// identical to a run over the same tree without that file. Only a discovery
/// failure (e.g. an unreadable root) or a rendering failure fails the run.
pub fn run(options: &RunOptions) -> Result<RunOutput> {
    let files = discovery::discover_rust_files(&options.paths)?;

    let mut analyzed: Vec<Analyzed> = Vec::new();
    let mut diagnostics: Vec<String> = Vec::new();
    for path in &files {
        match std::fs::read_to_string(path) {
            Err(err) => diagnostics.push(skipped(path, &err.to_string())),
            Ok(source) => match parse::extract(path, &source) {
                Ok(mut found) => analyzed.append(&mut found),
                // `extract` only ever yields `Parse`; the fallback keeps the
                // match total without repeating the path the variant embeds.
                Err(Error::Parse { message, .. }) => diagnostics.push(skipped(path, &message)),
                Err(other) => diagnostics.push(skipped(path, &other.to_string())),
            },
        }
    }

    let candidates = detect::detect(
        &analyzed,
        &detect::DetectOptions {
            threshold: options.threshold,
            min_lines: options.min_lines,
            min_nodes: options.min_nodes,
        },
    );

    Ok(RunOutput {
        report: report::render(&candidates, options.format)?,
        diagnostics,
    })
}

/// The one-line stderr diagnostic for a file skipped mid-run (N39).
fn skipped(path: &str, reason: &str) -> String {
    format!("warning: skipping {path}: {reason}")
}
