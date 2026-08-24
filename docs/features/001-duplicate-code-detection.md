# Feature: Duplicate Code Detection (TED core, dry4go UX skin)
**Branch:** vibe/001-duplicate-code-detection
**Status:** Planning

## Requirements

A Rust CLI that finds near-duplicate code fragments within a Rust codebase and reports them as
candidate pairs.

- **Core algorithm:** tree-edit distance (Zhang–Shasha / APTED family) over a normalized `syn`-derived
  AST, similarity normalized to [0,1]. Explicitly chosen over dry4go's Jaccard; this breaks
  core-arithmetic parity AND parity on the score/threshold numbers (see O1, D1).
- **UX is a dry4go "skin" — parity on surface, NOT semantics:**
  - CLI: `dry4rust [options] [paths...]` with flags `--threshold`, `--min-lines`, `--min-nodes`,
    `--format text|json`, and aliases `--json` (=`--format json`), `--text` (=`--format text`).
  - Defaults are TED-appropriate: `--threshold 0.75` (see A2), `--min-lines 4`, `--min-nodes 20`.
  - Output is byte-parity with dry4go:
    - text: `DUPLICATE score=0.89` then two 2-space-indented `path:start-end` lines; score at 2 dp.
    - json: `{ "candidates": [ { "score": <raw f64>, "left": {"file","start_line","end_line"},
      "right": {...}, "left_nodes": <int>, "right_nodes": <int> } ] }`; score is raw f64;
      `left_nodes`/`right_nodes` = normalized-tree node counts of each fragment.
  - Topology = candidate PAIRS only. No clustering/union-find groups.
- **Granularity = EXTENDED:** free functions; methods (inherent + trait-impl + trait default bodies);
  `impl` block bodies; closures; free `{}` blocks. Containment-dedup policy applied (A3).
- **Discovery:** honor `.gitignore` via the `ignore` crate; scan `*.rs`; skip `/target`.
- **Behavior:** pure reporter — always exit 0 on a successful run; non-zero reserved for usage/internal
  errors. Deterministic, cross-OS-stable output.
- **Layout:** one lib crate + one bin crate. Edition 2021. `thiserror` in lib, `anyhow` in bin, `clap`
  for CLI. `cargo fmt --check` + `clippy -D warnings` clean.

Out of scope for v1: baseline/allowlist, exit-code gate, clustering, cross-language.

## Design Options (Ox)

### O1 — Tree-edit distance (Zhang–Shasha / APTED) over normalized `syn` AST  — **CHOSEN**
- Description: Parse each fragment with `syn`, lower to a normalized label tree (structural node kinds;
  identifiers/literals canonicalized so Type-2 renamed clones match), compute unit-cost TED pairwise,
  normalize to a [0,1] similarity, gate by `--threshold`.
- Pros: Structure-aware (robust to reordering/renaming that fools token bags); tunable, explainable
  score; `left_nodes`/`right_nodes` fall out of the same normalized tree.
- Cons: Super-quadratic per-pair cost; O(n²) pairs → real perf risk (R1); score numbers and threshold
  diverge from dry4go (D1); normalization choice is a semantic commitment (R3).

### O2 — Jaccard over token/shingle bags (dry4go parity)  — considered, DEFERRED (D1)
- Description: Reproduce dry4go's fingerprint-set Jaccard for full core-arithmetic + number parity.
- Pros: Byte-and-number parity with dry4go; cheap; simple.
- Cons: The human explicitly wants TED's structural semantics; parity on numbers is a non-goal here.

### O3 — Hybrid (Jaccard pre-filter → TED refine)  — considered, DEFERRED
- Description: Cheap Jaccard/size pre-filter to prune pairs, TED only on survivors.
- Pros: Best perf/precision tradeoff at scale.
- Cons: Premature for v1 (YAGNI); the admissible size-ratio pre-filter (S3) captures most of the win
  with far less machinery and zero false pruning.

**Recommended: O1 — matches the human's explicit choice of structural TED semantics; O2/O3 recorded for
traceability. The size-ratio pre-filter idea from O3 is retained as an admissible optimization in S3.**

## Slices (Sx)

A slice is defined in `docs/meta-design.md` — each is independently runnable/verifiable end-to-end.

| Slice | Outcome | Depends on |
|-------|---------|------------|
| S1 | **Runnable CLI**: discover `.rs` (honor `.gitignore`, skip `/target`) → parse → extract free functions + methods → TED + similarity → threshold/min-lines/min-nodes gate → byte-parity text & json → exit 0. Deterministic output. | - |
| S2 | Extend extraction to `impl` block bodies, closures, free `{}` blocks; apply containment-dedup policy. | S1 |
| S3 | Performance & scaling: admissible size-ratio pre-filter + guardrail benchmark; (optional, de-scopable). | S1, S2 |

## Tasks (Tx)

One or more tasks per slice. Unit tests live beside pure core modules; integration tests drive the
built binary against fixture trees.

| #   | Slice | Task | Status | Commit |
|-----|-------|------|--------|--------|
| T1  | S1 | Scaffold lib+bin crates (edition 2021), deps (`syn`, `ignore`, `clap`, `thiserror`, `anyhow`, `serde`, `serde_json`), module skeleton, `error` types. No behavior. | Done | 0411756 |
| T2  | S1 | Domain model (`Fragment`, `Candidate`) + discovery adapter (`ignore` crate: walk `*.rs`, honor `.gitignore`, skip `/target`, normalize paths to `/`). **Unit:** filtering + path normalization. **Integration:** temp tree with `.gitignore`. | Done | 9c60aba |
| T3  | S1 | Parse adapter (`syn` → normalized label tree) + fragment extraction for free functions & methods (inherent/trait-impl/trait-default); node counting; identifier/literal canonicalization. **Unit:** source→fragments + node counts. | Done | (next) |
| T4  | S1 | TED engine (Zhang–Shasha, unit cost) + similarity normalization (pure, std-only). **Unit:** known small trees→known δ; identical→1.0; disjoint→0.0; symmetry. | Pending | - |
| T5  | S1 | Detect orchestration: pairwise compare, apply min-lines/min-nodes filters + threshold gate, canonical `(left,right)` ordering, deterministic candidate ordering. **Unit:** filter application + determinism. | Pending | - |
| T6  | S1 | Report adapter: text (2 dp) + json (raw f64, exact key order) byte-parity. **Unit:** golden-string assertions for both formats. | Pending | - |
| T7  | S1 | CLI wiring (`clap`): flags/aliases, TED defaults, format selection, `anyhow` error mapping, exit 0 on success / non-zero on usage+internal. **Integration:** run binary on fixture dir, assert text & json stdout. | Pending | - |
| T8  | S2 | Extend extraction to `impl` block bodies, closures, free `{}` blocks. **Unit:** nested-fragment extraction + node counts. | Pending | - |
| T9  | S2 | Containment-dedup policy (maximal-parent-wins, both-sides; identical-span dedup; deterministic tie-break). **Unit:** both-sided suppression; one-sided keep; identical-span dedup. | Pending | - |
| T10 | S2 | **Integration:** fixture with duplicated nested closures/blocks → assert only maximal pairs reported. | Pending | - |
| T11 | S3 | Admissible size-ratio pre-filter (`sim ≤ min(n₁,n₂)/max(n₁,n₂)`): prune pairs below `--threshold` before TED — provably never drops a real match. **Unit:** prune-soundness (a would-be match is never pruned). | Pending | - |
| T12 | S3 | Perf guardrail benchmark on a medium fixture; document complexity envelope. **Integration/bench.** | Pending | - |

## Risks (Rx)

- **R1 (perf):** O(n²) pairs × super-quadratic TED (APTED ~O(n³) worst) → slow on large repos. Mitigate:
  `min-nodes` floor, size-ratio pre-filter (S3), size bucketing; keep TED confined so it's swappable.
- **R2 (calibration):** `0.75` default is provisional; precision/recall unvalidated without dogfood data
  (D5). Mitigate: `--threshold` is exposed.
- **R3 (semantic commitment):** the normalization formula fixes the meaning of `score` and
  `left_nodes`/`right_nodes`; changing it later is a breaking output change. Mitigate: document + freeze.
- **R4 (parse robustness):** exotic Rust (macros, `cfg`, raw idents) may fail/skew `syn` parsing. Decide
  skip-with-warning vs error; keep the decision deterministic (same files skipped every run).
- **R5 (dedup ambiguity):** one-sided containment cases risk over/under-reporting; policy must be precise
  (A3) and unit-pinned.
- **R6 (determinism):** path separators, line endings, and unordered map iteration threaten cross-OS
  stability. Mitigate: normalize paths to `/`, normalize line endings, sort before emit, never serialize
  from an unordered map.
- **R7 (granularity blow-up):** closures + free blocks multiply fragment count → worsens R1 and adds
  containment noise. Mitigated by dedup (T9) and the pre-filter (T11); closures/blocks are the first
  de-scope lever (D7).

## Assumptions (Ax)

- **A1:** Granularity = EXTENDED — free functions; methods (inherent + trait-impl + trait-default bodies);
  `impl` block bodies; closures; free `{}` blocks.
- **A2:** TED default `--threshold 0.75` with metric normalization
  `sim = 1 − 2δ/(|T₁|+|T₂|+δ)`, δ = unit-cost (ins/del/relabel = 1) tree-edit distance.
- **A3:** Containment-dedup = maximal-parent-wins when containment holds on **both** sides; identical-span
  pairs de-duplicated; one-sided containment keeps both; deterministic `(path,start,end)` tie-break.
- **A4:** Discovery honors **in-tree** `.gitignore` via `ignore` (machine-global/parent gitignores
  disabled for cross-machine determinism — see N5); scans `*.rs`; **skips any directory named `target`
  at any depth** (accepted over root-only `/target` — correct for nested-workspace `target/` dirs; N7).
- **A5:** Layout = 1 lib + 1 bin. Adapters (`syn`/`ignore`/`clap`/`serde`) confined to their modules;
  core (`tree`/`ted`/`similarity`/`dedup`/`detect`) is pure std-only.
- **A6:** Label model = structural `syn` node kind with identifiers/literals canonicalized, so Type-2
  (renamed) clones match; structure is preserved.
- **A7:** Output is deterministic/cross-OS-stable: paths normalized to `/`, line counts on normalized line
  endings, candidates sorted by canonical `(left,right)` key before emit.
- **A8:** Pure reporter — always exit 0 on a successful run; non-zero reserved for usage/internal errors.
- **A9:** `--min-lines 4` and `--min-nodes 20` retained from dry4go (structural floors are algorithm-
  agnostic, so parity here is honest).

## Deferrals (Dx)

- **D1:** Jaccard-parity (dry4go core arithmetic) dropped in favor of TED — score/threshold numbers
  intentionally diverge. Traceable core decision.
- **D2:** Baseline / allowlist — out of v1.
- **D3:** Exit-code gate (fail-on-findings) — deferred; v1 is a pure reporter (A8).
- **D4:** Clustering / union-find groups — deferred; pairs only.
- **D5:** Threshold calibration + dogfood run — deferred; `0.75` is provisional.
- **D6:** 4-crate workspace — deferred (YAGNI); revisit only if core is reused externally.
- **D7:** Closures + free `{}` blocks — in scope for v1 (S2), but recorded as the first de-scope lever if
  R1/R7 (perf/noise) prove unacceptable post-dogfood.
- **D8:** S3 (pre-filter + benchmark) is optional and may be trimmed if timeline is tight — being an
  *admissible* optimization, deferring it changes only speed, never results.

## Notes & Decisions

- The size-ratio pre-filter is **admissible**: since δ ≥ ||T₁|−|T₂||, the normalization gives
  `sim ≤ min(|T₁|,|T₂|)/max(|T₁|,|T₂|)`, so any pair whose smaller tree is below `threshold × larger`
  cannot pass and is safely skipped before running TED. Results are identical with or without it.
- Left/right within a pair are assigned by canonical `(path,start_line,end_line)` order (smaller = left)
  so output is stable regardless of traversal order.

### T1 review notes (Anders — approve-with-suggestions, non-blocking)

- **N1 (core-purity unenforced):** adapters→core is convention + `pub(crate)` + clippy only (single-package
  per D6). Cheap later guardrail: a test/CI grep asserting core modules (`tree/ted/similarity/dedup/detect/
  model/error`) contain no `use syn|ignore|clap|serde`. Optional, don't gold-plate.
- **N2 (parse span, for T3):** fold `syn::Error` span (line/col) into `Error::Parse` message at construction
  so location isn't lost despite the unstructured `String`.
- **N3:** consider `#[non_exhaustive]` on `pub enum Error` (variants grow in T2–T7). Zero-cost future-proofing.
- **N4 (serde seam, for T2+T6 — most important):** keep `#[derive(Serialize)]` OFF core `model` types;
  `report` owns its own Serialize DTOs and maps from `model`, so serde stays confined to `report` (A5).

### T2 review notes (Anders — approve-with-suggestions)

- **N5 (determinism) — FIXED:** `WalkBuilder` now disables `git_global`/`git_exclude`/`parents`; discovery
  is a pure function of the scanned tree. Regression test added.
- **N6 (`Error::Io.path` semantics) — FIXED:** real path in `path`, path-free inner message in `source`,
  no Display stutter. Regression test added.
- **N7 (`target` pruning scope) — DECIDED:** any-depth `target` pruning accepted; A4 updated. (Driver call
  in hands-free mode; flag to human.)
- **N8 (DRY, for T6):** `Candidate.left_nodes/right_nodes` duplicate `Fragment.node_count`; drop them and
  derive at report time, or document as a deliberate denormalized output mirror. Resolve at/before T6.
- **N9 (for T3):** pin `line_count` semantics — `end_line − start_line + 1` vs logical LOC — so `--min-lines`
  gates on a defined value and dry4go parity stays honest.
- **N10:** `FragmentKind.kind` has no consumer through S2 yet — confirm one (e.g. stderr diagnostics) or
  treat as speculative.
- **N11 (record only):** `hidden(true)` default skips dot-dirs; `canonical_key()` returns a borrow so T5
  should sort via `sort_by(|a,b| a.canonical_key().cmp(&b.canonical_key()))`; no-args→default `.` is a T7
  concern; remove `model.rs` `#![allow(dead_code)]` by end of S1.

### T3 review notes (Anders — APPROVE-WITH-NOTES; nothing blocked T4)

T3 verified PASS by Bhaskar (full gate, 29 tests) after two over-normalization fix rounds (initial
structural-marker rewrite, then one-sided ranges + async capture + a dishonest `Expr::Infer` test).
- **N12 (dependency flow, decide before T5 — most important):** `Extracted{fragment,tree}` lives in the
  `parse` adapter, but its natural consumer is core `detect` — that would make **core import an adapter**,
  violating the one hard layering rule. Move the pairing type into core (`model`) with `parse` constructing
  it, or have `detect` take `(&Fragment, &NormTree)` pairs. Cheap now, structural later.
- **N13 (contract wording vs behavior):** types are erased everywhere (`Param`, `ReturnType` presence-only,
  `as T`, `PatType`, turbofish) but A6/R3 + `tree.rs` doc say only "identifiers and literal values". Amend
  wording to "identifiers, **types**, and literal values" so the frozen text matches frozen behavior.
- **N14 (asymmetry — product decision):** block flavors are preserved but `fn` qualifiers are not —
  `async fn`/`const fn`/`unsafe fn` and receiver form (`self`/`&self`/`&mut self`) lower identically.
  Decide: preserve (flags on `Label::Function` / receiver-param label) or declare out of scope.
- **N15 (statement semicolon — product decision):** `Stmt::Expr(expr, semi)` drops the semi, so `{ x }` and
  `{ x; }` (value vs unit) lower identically. Preserve or record as a known limitation beside macros.
- **N16 (comment vs code):** `Label::Item` doc says a nested item "is a fragment in its own right", but
  `collect_items` never descends into fn bodies, so an inner `fn`/in-body `impl` is never extracted. Fix at
  T8 or correct the comment now.
- **N17 (T4 design constraint):** Zhang–Shasha needs post-order + leftmost-leaf-descendant + keyroot arrays;
  derive them **once per fragment** (cached with the tree), not per pair — O(n²) pairs make per-pair
  flattening dominate. `NormTree` is a fine source of truth. Add a `node_count == post_order.len()` invariant
  test at T4.
- **N18 (typing):** `Binary(&'static str)`/`Unary(&'static str)` with `_ => "?"` is a stringly-typed hole that
  silently aliases future operators. Prefer small `BinOpKind`/`UnOpKind` enums — restores `Copy`, gives
  compiler exhaustiveness, lets `Label` derive `Hash` for T4 memo keys.
- **N19 (precision risk — feeds D5):** macro-as-leaf cuts both ways for the gate — macro-heavy fragments can
  shrink below `--min-nodes 20` (false negatives) while unrelated macro-heavy bodies look identical (false
  positives). Explicit input to D5 threshold calibration.
- **N20 (robustness, low priority):** `lower_expr`/`lower_pat`/`node_count` + `NormTree`'s recursive `Drop`
  are unbounded-depth recursion on a lib path; pathological nesting aborts. `syn` likely fails first — record
  only, don't pre-optimize.

Anders also re-scoped priors: **N8** resolve at **T5** (not T6 — where `Candidate` is first built);
**N10** decide by T6 (diagnostics or delete); **N11** on track (dead_code allows fall out once T5 wires
`parse → detect`).
