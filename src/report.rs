//! Report adapter: byte-parity text/json rendering via `serde`/`serde_json`.
//! Confines serialization to this module.
//!
//! The core `model` types carry no serde derives (N4); this module owns its own
//! `Serialize` DTOs and maps `model::Candidate` onto them. DTO field order is
//! load-bearing — serde emits struct fields in declaration order, and the JSON
//! key order is part of the dry4go byte-parity contract.
//!
//! Candidates are rendered in the order given: `detect` has already sorted them
//! deterministically, so this module never re-sorts.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{Candidate, Fragment};

/// Output format selected by the CLI (`--format text|json`, `--json`, `--text`).
///
/// Re-exported from the crate root as part of the [`crate::run`] façade (N36).
/// It deliberately carries no `clap` derive — the binary owns its own
/// `ValueEnum` and maps onto this (N37), keeping `clap` out of the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable `DUPLICATE score=…` blocks.
    Text,
    /// Pretty-printed JSON (2-space indent).
    Json,
}

/// Render detected candidates in the requested format, trailing newline included.
///
/// Text rendering is infallible; JSON rendering bubbles a serialization failure
/// as [`Error::Render`] rather than panicking (no `unwrap`/`expect` on a
/// reachable path).
pub(crate) fn render(candidates: &[Candidate], format: Format) -> Result<String> {
    match format {
        Format::Text => Ok(render_text(candidates)),
        Format::Json => render_json(candidates),
    }
}

/// Text format: one 3-line block per candidate, blank-line separated.
fn render_text(candidates: &[Candidate]) -> String {
    if candidates.is_empty() {
        return "No duplicate candidates found.\n".to_string();
    }

    let blocks: Vec<String> = candidates
        .iter()
        .map(|candidate| {
            format!(
                "DUPLICATE score={:.2}\n  {}\n  {}",
                candidate.score,
                span(&candidate.left),
                span(&candidate.right),
            )
        })
        .collect();

    format!("{}\n", blocks.join("\n\n"))
}

/// JSON format: `{ "candidates": [ … ] }`, pretty-printed with a 2-space indent.
fn render_json(candidates: &[Candidate]) -> Result<String> {
    let report = Report {
        candidates: candidates.iter().map(CandidateDto::from).collect(),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|err| Error::Render {
        message: err.to_string(),
    })?;
    Ok(format!("{json}\n"))
}

/// `path:start-end`, the text format's fragment locator.
fn span(fragment: &Fragment) -> String {
    format!(
        "{}:{}-{}",
        fragment.path, fragment.start_line, fragment.end_line
    )
}

/// JSON root — a single `candidates` array (empty serializes as `[]`).
#[derive(Serialize)]
struct Report<'a> {
    candidates: Vec<CandidateDto<'a>>,
}

/// One candidate pair. Field order is the JSON key order (byte-parity).
#[derive(Serialize)]
struct CandidateDto<'a> {
    /// Raw, unrounded similarity (2-dp rounding is text-only).
    score: f64,
    left: LocationDto<'a>,
    right: LocationDto<'a>,
    /// Derived from `left.node_count` (N8 — not mirrored on `Candidate`).
    left_nodes: usize,
    /// Derived from `right.node_count` (N8).
    right_nodes: usize,
}

/// A fragment's location. Field order is the JSON key order (byte-parity).
#[derive(Serialize)]
struct LocationDto<'a> {
    file: &'a str,
    start_line: usize,
    end_line: usize,
}

impl<'a> From<&'a Candidate> for CandidateDto<'a> {
    fn from(candidate: &'a Candidate) -> Self {
        Self {
            score: candidate.score,
            left: LocationDto::from(&candidate.left),
            right: LocationDto::from(&candidate.right),
            left_nodes: candidate.left.node_count,
            right_nodes: candidate.right.node_count,
        }
    }
}

impl<'a> From<&'a Fragment> for LocationDto<'a> {
    fn from(fragment: &'a Fragment) -> Self {
        Self {
            file: fragment.path.as_str(),
            start_line: fragment.start_line,
            end_line: fragment.end_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FragmentKind;

    fn fragment(path: &str, start_line: usize, end_line: usize, node_count: usize) -> Fragment {
        Fragment {
            path: path.to_string(),
            start_line,
            end_line,
            node_count,
            line_count: end_line - start_line + 1,
            kind: FragmentKind::Function,
        }
    }

    fn candidate(score: f64) -> Candidate {
        Candidate {
            left: fragment("a.rs", 1, 5, 12),
            right: fragment("b.rs", 10, 14, 13),
            score,
        }
    }

    fn text(candidates: &[Candidate]) -> String {
        match render(candidates, Format::Text) {
            Ok(rendered) => rendered,
            Err(err) => panic!("text rendering must not fail: {err}"),
        }
    }

    fn json(candidates: &[Candidate]) -> String {
        match render(candidates, Format::Json) {
            Ok(rendered) => rendered,
            Err(err) => panic!("json rendering must not fail: {err}"),
        }
    }

    #[test]
    fn text_empty_reports_no_candidates() {
        assert_eq!(text(&[]), "No duplicate candidates found.\n");
    }

    #[test]
    fn text_single_candidate_is_a_three_line_block() {
        assert_eq!(
            text(&[candidate(0.89)]),
            "DUPLICATE score=0.89\n  a.rs:1-5\n  b.rs:10-14\n"
        );
    }

    #[test]
    fn text_multiple_candidates_are_blank_line_separated() {
        let candidates = vec![
            candidate(0.89),
            Candidate {
                left: fragment("c.rs", 1, 3, 7),
                right: fragment("d.rs", 7, 9, 7),
                score: 1.0,
            },
        ];
        assert_eq!(
            text(&candidates),
            "DUPLICATE score=0.89\n  a.rs:1-5\n  b.rs:10-14\n\nDUPLICATE score=1.00\n  c.rs:1-3\n  d.rs:7-9\n"
        );
    }

    #[test]
    fn text_renders_score_to_exactly_two_decimals() {
        // Pins Rust's `{:.2}` behavior: 0.895_f64 is stored slightly ABOVE
        // 0.895, so it rounds UP to 0.90.
        assert_eq!(
            text(&[candidate(0.895)]),
            "DUPLICATE score=0.90\n  a.rs:1-5\n  b.rs:10-14\n"
        );
        assert_eq!(
            text(&[candidate(1.0)]),
            "DUPLICATE score=1.00\n  a.rs:1-5\n  b.rs:10-14\n"
        );
        assert_eq!(
            text(&[candidate(0.5)]),
            "DUPLICATE score=0.50\n  a.rs:1-5\n  b.rs:10-14\n"
        );
        assert_eq!(
            text(&[candidate(0.756_789)]),
            "DUPLICATE score=0.76\n  a.rs:1-5\n  b.rs:10-14\n"
        );
    }

    #[test]
    fn text_renders_candidates_in_slice_order() {
        // `detect` already sorts; the adapter must not re-order.
        let candidates = vec![
            Candidate {
                left: fragment("z.rs", 1, 2, 5),
                right: fragment("z.rs", 4, 5, 5),
                score: 0.8,
            },
            candidate(0.89),
        ];
        assert_eq!(
            text(&candidates),
            "DUPLICATE score=0.80\n  z.rs:1-2\n  z.rs:4-5\n\nDUPLICATE score=0.89\n  a.rs:1-5\n  b.rs:10-14\n"
        );
    }

    #[test]
    fn json_empty_is_an_empty_array() {
        assert_eq!(json(&[]), "{\n  \"candidates\": []\n}\n");
    }

    #[test]
    fn json_single_candidate_pins_keys_order_and_indent() {
        assert_eq!(
            json(&[candidate(0.895)]),
            r#"{
  "candidates": [
    {
      "score": 0.895,
      "left": {
        "file": "a.rs",
        "start_line": 1,
        "end_line": 5
      },
      "right": {
        "file": "b.rs",
        "start_line": 10,
        "end_line": 14
      },
      "left_nodes": 12,
      "right_nodes": 13
    }
  ]
}
"#
        );
    }

    #[test]
    fn json_node_counts_come_from_the_fragments() {
        let rendered = json(&[Candidate {
            left: fragment("a.rs", 1, 5, 31),
            right: fragment("b.rs", 10, 14, 42),
            score: 1.0,
        }]);
        assert!(rendered.contains("\"left_nodes\": 31"));
        assert!(rendered.contains("\"right_nodes\": 42"));
    }

    #[test]
    fn json_score_is_raw_and_unrounded() {
        assert!(json(&[candidate(0.756_789)]).contains("\"score\": 0.756789"));
    }
}
