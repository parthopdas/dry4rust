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
  reads file **contents** from disk (discover → `read_to_string` → parse → detect → dedup → render) and
  exposes the crate's only public façade `run(&RunOptions) -> Result<RunOutput>` (plus `RunOptions`/`RunOutput`/
  `Format` and the `error` types; everything else is `pub(crate)`). It owns the per-file
  skip-with-diagnostic policy. Below it, **core** is IO-free; `discovery` is the only other module that
  touches the filesystem (traversal). It also owns the **oversized-fragment ceiling** (`max_nodes`):
  a fragment above it is dropped before `detect` with its own deterministic stderr diagnostic, so a
  pair of giants can never dominate a run.
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
  (Type-2) clones match; structure is preserved, including `fn` qualifiers, receiver form
  (`self`/`&self`/`&mut self`, taken from the effective self type so `self: &Self` ≡ `&self`; other
  typed receivers are `Owned`), and the block's terminating semicolon. Records each fragment's node
  count and line span.
- **TED + similarity** (`ted`, `similarity`): unit-cost tree-edit distance per pair, normalized to a
  [0,1] score. `left_nodes`/`right_nodes` are the two fragments' normalized-tree node counts.
  **Known limitation — what the score is evidence of.** Identifiers, literals and types are canonicalized
  away, and macro token streams are never parsed (a macro invocation is a single leaf), so a score
  measures the *structure surrounding* the erased content, not the text a reader would point at. Two
  fragments differing only in their literal or macro payloads therefore score as identical, and a
  genuine duplicate may be reported with a score and node counts that describe its enclosing shape
  rather than the duplicated content — a real finding, arrived at for an adjacent reason. Treat a
  finding as a pointer to a pair of locations, not as an explanation of why they match. The size floors
  do not address this: they change *which* fragments are scored, not what the score is evidence of.
  Partially un-erasing the label model would, and is rejected — it is a breaking change to the meaning
  of `score`.
- **Detect** (`detect`): applies `--min-lines`/`--min-nodes` floors, an admissible size-ratio pre-filter,
  and an **intra-pair overlap filter** — a pair whose two fragments share a path and whose line spans
  intersect is dropped at admission, before TED. EXTENDED extraction emits nested fragments, so the pair
  set otherwise contains every fragment against its own ancestor (a single-method `impl` vs that method
  scores ~0.95 by construction); such a pair is a report artifact, not a clone. This *intra-pair*
  containment is a different relation from `dedup`'s *pair-vs-pair* containment and is not reachable by
  it. Legitimate in-file clones have disjoint spans and are unaffected. Surviving pairs get TED; pairs at
  or above `--threshold` are kept; each is assigned canonical left/right and ordered deterministically.
- **Dedup** (`dedup`): removes redundant nested findings — pair `P` dominates pair `Q` when, side-for-side
  after `detect`'s canonical assignment, `Q.left ⊆ P.left` **and** `Q.right ⊆ P.right` (same path, span
  containment, **equality allowed on a side**) and `P ≠ Q` (irreflexive via the positional tie-break —
  no separate guard); `Q` is suppressed in favor of the maximal
  parent. Equality-on-a-side is what removes the cross-granularity `impl↔method` crosses. Identical-span
  pairs (reachable when two distinct fragments share a line span) collapse to one, picked by the values
  the report renders — **highest score**, then the higher `left_nodes`/`right_nodes` — falling back to
  incoming position only when spans tie, the two scores compare neither `Greater` nor `Less`, and both
  node counts tie. For every score `similarity` can produce — all of them finite — that middle
  condition *means* the scores are equal, so the surviving and suppressed pairs render identically
  (they may still differ in the unemitted `kind`/`line_count`). `NaN` is the one score that would
  reach the fallback while still rendering differently, and it is excluded on reachability rather
  than folded in: `similarity` cannot produce one. Containment on one side with **disjoint or partially
  overlapping** spans on the other keeps both. This *pair-vs-pair* relation is distinct from `detect`'s
  *intra-pair* overlap filter and never overlaps with it (A3). **Cost, deliberately post-TED (N83):**
  nesting depth `d` at a clone site costs `d²` TED evaluations to yield one surviving finding, every one
  of them paid before dedup runs — and the filter cannot move pre-TED, because a dominated pair must
  survive if its dominator fails the score gate (sizing the envelope is T12's).
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
  Exit-code contract (pinned by `tests/cli.rs`): **0** — any successful run, including "no duplicates"
  and runs that skipped files; **1** — whole-run failure (e.g. an unreadable root); **2** — `clap` usage
  error.
- **Config:** all behavior via CLI flags; no secrets, no network, no persistence.
- **Performance:** O(n²) pairs × super-quadratic TED is the main scaling risk; mitigated by the node
  floor, the admissible size-ratio pre-filter, the oversized-fragment ceiling, and confining TED so it
  stays swappable.
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
