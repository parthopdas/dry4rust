//! Command-line interface parsing for the `dry4rust` binary.
//! Confines the `clap` dependency to this module.
//!
//! The surface is the dry4go skin: `dry4rust [options] [paths...]` with
//! `--threshold`, `--min-lines`, `--min-nodes`, `--format text|json` and the
//! `--json` / `--text` aliases. All defaults (`0.85`, `4`, `20`, `text`, `.`)
//! live here (N29) so `--help` renders them from a single source, and the
//! `--format` value enum is local (N37) so `clap` never reaches the `report`
//! adapter.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use dry4rust::{Format, RunOptions};

/// The oversized-fragment ceiling handed to `run` (N23/N30).
///
/// TED is super-quadratic in the node counts, so a pair of very large
/// fragments — which the size-ratio pre-filter cannot prune, being
/// similar-sized — can dominate a whole run. `2000` nodes is far above any
/// hand-written fragment we expect to compare: re-measured on the S1 dogfood
/// after EXTENDED extraction (T8), the largest fragment in this repo is **439**
/// nodes (a single function; the largest `impl` block, whose tree is roughly
/// the sum of its methods', is 244), so the ceiling has ~4.5× headroom while
/// keeping the worst admitted pair's DP matrix at ~4M `u32` cells (N66).
/// Dropping an oversized fragment is also **benign for an `impl` block**: its
/// methods are extracted and compared individually, so the code inside it is
/// still covered — only the whole-block view is lost. It is deliberately
/// **not** a CLI flag: the dry4go skin has no such option, and a policy nobody
/// has needed to tune does not warrant one (YAGNI). Kept here so all defaults
/// stay in the clap layer (N29).
///
/// **T12 measured the worst pair this admits (N70/N66), and it is expensive.**
/// Two fragments at the ceiling cost **~0.7–1.3 s** for one TED when they are
/// flat (a long run of sibling statements) but **~50–70 s** when they are
/// deeply nested at the same node count — Zhang–Shasha costs
/// `O(n₁·n₂·min(depth₁,leaves₁)·min(depth₂,leaves₂))`, so shape, not node
/// count alone, sets the price. N66 said "if a single admitted maximal pair
/// costs seconds, 2000 is too generous"; that condition is met.
///
/// **The human ruled: the ceiling stays 2000 in v1 (D9).** It is not an open
/// question. No lower value defends itself — cost is
/// `n₁·n₂·min(d,l)₁·min(d,l)₂` and in the pathological family depth scales
/// *with* `n`, so every defensible replacement lands at or below the largest
/// legitimate fragment we have measured (439 nodes, above), and lowering would
/// drop large **flat** fragments, which are the cheap ones. The correct
/// instrument prices **shape**, not node count, and is deferred (D9): depth is
/// not computed anywhere today. Revisit trigger: T14's `(nodes, depth)`
/// histogram showing real admitted fragments that are both deep and large, or
/// a user report of a single pair costing minutes.
const MAX_NODES: usize = 2000;

/// Parse the process arguments into library run options.
///
/// Exits the process with clap's usage error (code 2) on a bad flag or an
/// out-of-range `--threshold`.
pub(crate) fn options() -> RunOptions {
    RunOptions::from(Cli::parse())
}

/// `dry4rust [options] [paths...]`.
#[derive(Debug, Parser)]
#[command(
    name = "dry4rust",
    version,
    about = "Tree-edit-distance duplicate-code detector for Rust codebases"
)]
struct Cli {
    /// Files or directories to scan.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Minimum similarity, in `0.0..=1.0`, for a pair to be reported.
    // `0.85` per D5 (was `0.75`). Under A2's normalization a threshold `t` is
    // identically a cap on size ratio (`sim ≤ n_small / n_large` — the same
    // identity T11's pre-filter uses), so `0.85` admits at most a 17.6%
    // node-count difference where `0.75` admitted 33%. Kept out of the doc
    // comment so `--help` stays the dry4go skin; see D5 for the derivation,
    // the confidence, and the revisit trigger.
    #[arg(long, default_value_t = 0.85, value_parser = threshold)]
    threshold: f64,

    /// Minimum source lines a fragment must span to be considered.
    #[arg(long, default_value_t = 4)]
    min_lines: usize,

    /// Minimum normalized-tree nodes a fragment must have to be considered.
    #[arg(long, default_value_t = 20)]
    min_nodes: usize,

    /// Output format.
    #[arg(long, value_enum, default_value_t = CliFormat::Text)]
    format: CliFormat,

    /// Alias for `--format json`; takes precedence over `--format`.
    #[arg(long, conflicts_with = "text")]
    json: bool,

    /// Alias for `--format text`; takes precedence over `--format`.
    #[arg(long)]
    text: bool,
}

/// The `--format` values. Separate from `dry4rust::Format` on purpose (N37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliFormat {
    /// `DUPLICATE score=…` blocks.
    Text,
    /// Pretty-printed JSON.
    Json,
}

impl From<CliFormat> for Format {
    fn from(format: CliFormat) -> Self {
        match format {
            CliFormat::Text => Format::Text,
            CliFormat::Json => Format::Json,
        }
    }
}

impl From<Cli> for RunOptions {
    fn from(cli: Cli) -> Self {
        // Precedence: an explicit alias wins over `--format`, which falls back
        // to its default. `--json` and `--text` together is a usage error.
        let format = match (cli.json, cli.text) {
            (true, _) => Format::Json,
            (_, true) => Format::Text,
            _ => cli.format.into(),
        };
        RunOptions {
            paths: cli.paths,
            threshold: cli.threshold,
            min_lines: cli.min_lines,
            min_nodes: cli.min_nodes,
            max_nodes: MAX_NODES,
            format,
        }
    }
}

/// `--threshold` value parser: a number within `0.0..=1.0`.
///
/// The range check also rejects NaN (`contains` is false for it), so a NaN
/// threshold can never reach `detect` and silently empty the report (N28).
fn threshold(value: &str) -> Result<f64, String> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| format!("`{value}` is not a number"))?;
    if (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("`{value}` is not in range 0.0..=1.0"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> RunOptions {
        RunOptions::from(Cli::parse_from(args))
    }

    #[test]
    fn defaults_come_from_the_clap_layer() {
        let options = parse(&["dry4rust"]);
        assert_eq!(options.paths, vec![PathBuf::from(".")]);
        assert_eq!(options.threshold, 0.85);
        assert_eq!(options.min_lines, 4);
        assert_eq!(options.min_nodes, 20);
        assert_eq!(options.max_nodes, MAX_NODES);
        assert_eq!(options.format, Format::Text);
    }

    #[test]
    fn positional_paths_replace_the_default() {
        let options = parse(&["dry4rust", "src", "tests/a.rs"]);
        assert_eq!(
            options.paths,
            vec![PathBuf::from("src"), PathBuf::from("tests/a.rs")]
        );
    }

    #[test]
    fn aliases_match_the_long_form_and_take_precedence() {
        assert_eq!(
            parse(&["dry4rust", "--format", "json"]).format,
            Format::Json
        );
        assert_eq!(parse(&["dry4rust", "--json"]).format, Format::Json);
        assert_eq!(parse(&["dry4rust", "--text"]).format, Format::Text);
        // An explicit alias beats an explicit `--format`.
        assert_eq!(
            parse(&["dry4rust", "--format", "text", "--json"]).format,
            Format::Json
        );
        assert_eq!(
            parse(&["dry4rust", "--format", "json", "--text"]).format,
            Format::Text
        );
    }

    #[test]
    fn conflicting_aliases_are_a_usage_error() {
        assert!(Cli::try_parse_from(["dry4rust", "--json", "--text"]).is_err());
    }

    #[test]
    fn threshold_accepts_the_closed_unit_interval_only() {
        assert_eq!(threshold("0"), Ok(0.0));
        assert_eq!(threshold("1"), Ok(1.0));
        assert_eq!(threshold("0.75"), Ok(0.75));
        assert!(threshold("1.5").is_err());
        assert!(threshold("-0.1").is_err());
        assert!(threshold("nan").is_err());
        assert!(threshold("inf").is_err());
        assert!(threshold("abc").is_err());
    }

    #[test]
    fn out_of_range_threshold_fails_parsing() {
        assert!(Cli::try_parse_from(["dry4rust", "--threshold", "1.5"]).is_err());
        assert!(Cli::try_parse_from(["dry4rust", "--threshold", "nan"]).is_err());
    }

    #[test]
    fn the_command_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
