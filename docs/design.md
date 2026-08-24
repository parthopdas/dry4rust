# Design

## System overview

`dry4rust` is a command-line duplicate-code detector for Rust codebases. It scans `.rs` files, extracts
code fragments, measures pairwise structural similarity, and reports near-duplicate **candidate pairs**.
It is a **pure reporter**: it prints findings and exits 0 on any successful run (a non-zero code is
reserved for usage/internal errors) — it does not gate builds or mutate the tree.

Users: developers and CI authors who want to surface copy-paste/near-clone fragments. Invocation:
`dry4rust [options] [paths...]`.

Its user experience is a deliberate **dry4go "skin"** — the CLI surface and output bytes match dry4go —
but the **core is different by design**: similarity is computed with **tree-edit distance (TED)** over a
normalized AST, not dry4go's fingerprint-set Jaccard. Parity is on *surface*, not *semantics*; the score
and threshold numbers intentionally diverge (see `docs/features/001-duplicate-code-detection.md`, O1/D1).

## Architecture

One **lib crate** (all logic) + one **bin crate** (CLI shell). Clean-ish internal boundaries inside the
lib; dependencies flow **adapters → core**, never the reverse.

- **Core (pure, std-only, side-effect-free):**
  - `tree` — normalized label-tree model + node counting.
  - `ted` — Zhang–Shasha / APTED-family unit-cost tree-edit distance.
  - `similarity` — normalization `sim = 1 − 2δ/(|T₁|+|T₂|+δ)` → [0,1].
  - `dedup` — containment-dedup policy over candidate pairs.
  - `detect` — orchestration: pairwise compare, filters, threshold gate, deterministic ordering.
  - `model` — domain types (`Fragment`, `Candidate`); `error` — `thiserror` types.
- **Adapters (each confines one third-party dependency):**
  - `parse` — `syn` → normalized tree + fragment extraction (**`syn` confined here**).
  - `discovery` — file walking via the `ignore` crate (**`ignore` confined here**).
  - `report` — text/json rendering via `serde`/`serde_json` (**serialization confined here**).
- **Composition root (`lib`):** the crate root wires the pipeline together — it is the only place that
  reads files from disk (discover → `read_to_string` → parse → detect → render) and exposes the single
  public façade `run(&RunOptions) -> Result<RunOutput>`. It owns the per-file skip-with-diagnostic policy;
  everything below it stays IO-free.
- **Bin crate:** `cli` (clap parsing; **`clap` confined here**) + `main` (`anyhow` wiring, exit codes).

Rule: core modules import no third-party crate and perform no IO; adapters depend on core; the bin
depends on lib. This keeps the scoring math testable in isolation and the TED engine swappable.

## Key components

- **Discovery** (`discovery`): walks the given paths, honors `.gitignore` via the `ignore` crate, scans
  `*.rs`, skips `/target`. Emits normalized (`/`-separated) file paths. (Deliberate divergence from
  dry4go's fixed skip-set.)
- **Parse & extract** (`parse`): parses each file with `syn`; lowers **EXTENDED** granularity fragments —
  free functions, methods (inherent + trait-impl + trait-default bodies), `impl` block bodies, closures,
  and free `{}` blocks — into normalized label trees. Identifiers/literals are canonicalized so renamed
  (Type-2) clones match; structure is preserved. Records each fragment's node count and line span.
- **TED + similarity** (`ted`, `similarity`): unit-cost tree-edit distance per pair, normalized to a
  [0,1] score. `left_nodes`/`right_nodes` are the two fragments' normalized-tree node counts.
- **Detect** (`detect`): applies `--min-lines`/`--min-nodes` floors, an admissible size-ratio pre-filter,
  then TED; keeps pairs at or above `--threshold`; assigns canonical left/right; orders results
  deterministically.
- **Dedup** (`dedup`): removes redundant nested findings — a nested pair contained on **both** sides by an
  outer pair is suppressed in favor of the maximal parent; identical spans de-duplicated; one-sided
  containment keeps both.
- **Report** (`report`): renders byte-parity output —
  - text: `DUPLICATE score=0.89` + two 2-space-indented `path:start-end` lines (score 2 dp);
  - json: `{ "candidates": [ { "score": <raw f64>, "left": {"file","start_line","end_line"},
    "right": {...}, "left_nodes": <int>, "right_nodes": <int> } ] }`.
- **CLI** (`cli`/`main`): flags `--threshold`, `--min-lines`, `--min-nodes`, `--format text|json`, aliases
  `--json`/`--text`, positional `[paths...]`; TED-appropriate defaults; exit 0 on success.

## Cross-cutting concerns

- **Determinism (required):** paths normalized to `/`; line counts on normalized line endings; candidates
  sorted by canonical `(left,right)` key before emit; never serialize from an unordered map. Output is
  stable across OS and across runs.
- **Error handling:** typed `thiserror` errors in the lib (no `unwrap`/`expect`/`panic!` on reachable
  paths); the bin bubbles via `anyhow` and maps to a non-zero exit only for usage/internal failures.
- **Config:** all behavior via CLI flags; no secrets, no network, no persistence.
- **Performance:** O(n²) pairs × super-quadratic TED is the main scaling risk; mitigated by the node
  floor, the admissible size-ratio pre-filter, and confining TED so it stays swappable.
- **Observability:** stdout is the report; diagnostics (e.g. skipped unparsable files) go to stderr and
  never perturb stdout bytes.

## Conventions

Mirror of `.github/copilot-instructions.md` → Project profile:

- Rust **edition 2021**, MSRV **1.74** (pinned in CI).
- `thiserror` in the lib; `anyhow` only in the bin.
- No `unwrap`/`expect`/`panic!`/panicking-index on reachable lib paths.
- Least-privilege visibility (default private; prefer `pub(crate)`; no blanket re-exports).
- Adapters (`syn`/`ignore`/`clap`/`serde`) confined to their modules; core is pure std-only.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must be clean.
- Deterministic, cross-OS-stable output is a hard requirement.
- Topology is **candidate pairs** — no clustering/groups in v1.
