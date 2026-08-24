# Feature: Duplicate Code Detection (TED core, dry4go UX skin)
**Branch:** vibe/001-duplicate-code-detection
**Status:** WIP — **S1 DONE**; T11 + T8 landed. Next: T8b (N61) → scoring pass → T9/T10 → D5 → T12

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
| S3 | Performance & scaling: admissible size-ratio pre-filter + guardrail benchmark; (optional, de-scopable). | S1 (T11); S1+S2 (T12) — see N50(b) |

## Tasks (Tx)

One or more tasks per slice. Unit tests live beside pure core modules; integration tests drive the
built binary against fixture trees.

| #   | Slice | Task | Status | Commit |
|-----|-------|------|--------|--------|
| T1  | S1 | Scaffold lib+bin crates (edition 2021), deps (`syn`, `ignore`, `clap`, `thiserror`, `anyhow`, `serde`, `serde_json`), module skeleton, `error` types. No behavior. | Done | 0411756 |
| T2  | S1 | Domain model (`Fragment`, `Candidate`) + discovery adapter (`ignore` crate: walk `*.rs`, honor `.gitignore`, skip `/target`, normalize paths to `/`). **Unit:** filtering + path normalization. **Integration:** temp tree with `.gitignore`. | Done | 9c60aba |
| T3  | S1 | Parse adapter (`syn` → normalized label tree) + fragment extraction for free functions & methods (inherent/trait-impl/trait-default); node counting; identifier/literal canonicalization. **Unit:** source→fragments + node counts. | Done | 4c55c8f |
| T4  | S1 | TED engine (Zhang–Shasha, unit cost) + similarity normalization (pure, std-only). **Unit:** known small trees→known δ; identical→1.0; disjoint→0.0; symmetry. | Done | 411b0a5 |
| T5  | S1 | Detect orchestration: pairwise compare, apply min-lines/min-nodes filters + threshold gate, canonical `(left,right)` ordering, deterministic candidate ordering. **Unit:** filter application + determinism. | Done | 4570f12 |
| T6  | S1 | Report adapter: text (2 dp) + json (raw f64, exact key order) byte-parity. **Unit:** golden-string assertions for both formats. | Done | ffa28a3 |
| T7  | S1 | CLI wiring (`clap`): flags/aliases, TED defaults, format selection, `anyhow` error mapping, exit 0 on success / non-zero on usage+internal. **Integration:** run binary on fixture dir, assert text & json stdout. | Done | be5128b |
| T7b | S1 | Close S1 test gaps before stamping S1 done: **N43** CRLF-invariance (`\n` vs `\r\n` fixture → byte-identical report), **N44** `.gitignore`/`target/` skipping exercised through `run()`, **N45** exit-1 at the binary level. Also **N42** design.md drift (name lib.rs composition-root role). | Done | 400243a |
| T8  | S2 | Extend extraction to `impl` block bodies, closures, free `{}` blocks. **Unit:** nested-fragment extraction + node counts. Ships the N23/N30 ceiling, N26 `u32` cells, N41, N56–N58. | Done | 5d04900 |
| T8b | S2 | **N61 intra-pair containment filter** (found at T8 review): drop pairs whose fragments share a path and whose spans overlap, at admission, pre-TED. Without it every single-method `impl` reports against its own method at ~0.95. Also N63/N66/N67. **Must precede the scoring pass (N62).** | Pending | - |
| T9  | S2 | Containment-dedup policy (maximal-parent-wins, both-sides; identical-span dedup; deterministic tie-break). **Unit:** both-sided suppression; one-sided keep; identical-span dedup. | Pending | - |
| T10 | S2 | **Integration:** fixture with duplicated nested closures/blocks → assert only maximal pairs reported. | Pending | - |
| T11 | S3 | Admissible size-ratio pre-filter (`sim ≤ min(n₁,n₂)/max(n₁,n₂)`): prune pairs below `--threshold` before TED — provably never drops a real match. **Unit:** prune-soundness (a would-be match is never pruned). **Landed as `similarity(max−min,min,max) >= threshold` — see N52 restated.** | Done | bb38a40 |
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

### T4 review notes (Anders — APPROVE-WITH-NOTES; nothing blocked T5)

T4 verified PASS by Bhaskar (full gate, 45 tests). Zhang–Shasha math + all hand-derived fixture δ values
independently re-derived and confirmed; one similarity `usize`-overflow defect found and fixed (all
arithmetic now f64). API: `ted::PreparedTree::new/len/…`, `distance`, `distance_trees`;
`similarity::similarity`, `similarity_prepared`. N17 SATISFIED by `PreparedTree` (per-fragment caching;
`len()==node_count` invariant tested).
- **N21 (layering — take at T5):** `similarity.rs` is the frozen-formula module (A2/R3); `similarity_prepared`
  couples it to `PreparedTree`, so swapping the TED engine (R1 hedge "keep TED swappable") would force an edit
  to the frozen module. Keep `similarity(delta, nodes_a, nodes_b)` as the only public seam; move
  `similarity_prepared` into `detect` where engine + formula compose. 5-line move now.
- **N22 (two sources of |T| — pin at T5):** `similarity_prepared` normalizes by `PreparedTree::len()`; report
  (N8) emits `Fragment.node_count`. They must never diverge. Add a test over parsed `Extracted` asserting
  `PreparedTree::new(&e.tree).len() == e.fragment.node_count`. Consider renaming `PreparedTree::len()` →
  `node_count()`.
- **N23 (unbounded n·m matrix — PRODUCT DECISION):** nothing caps fragment size; S2's `impl` bodies can be
  thousands of nodes → two 5k-node fragments = ~25M-cell usize matrix (~200 MB), and Rust alloc failure
  ABORTS (unhandleable). T11 ratio pre-filter does NOT help (two similar-sized giants pass). Decide:
  (a) node ceiling that skips-with-stderr-diagnostic (deterministic, fits A8), or (b) accept + document.
  Cheap win regardless: cells fit in `u32` (δ ≤ n+m) → halves memory.
- **N24 (per-pair scratch — record only, T12 candidate):** `tree_dist` + forest matrices are re-allocated per
  pair; a caller-owned reusable scratch buffer (`distance_with_scratch`) is the next perf lever after T11.
  YAGNI for S1; ensure T12 benchmark measures this.
- **N25 (dead surface — resolve at T5, ties N11):** `distance_trees` has no production consumer (tests only).
  At T5 delete it or make it `#[cfg(test)]`, else `ted.rs`'s `#![allow(dead_code)]` can't drop by end of S1.
- **N26 (fail-fast — cheap, do at T5):** `get`/`set` silently return `0`/drop on out-of-range index (honors
  no-panic rule) but would turn a future indexing bug into a silently-wrong score. Add `debug_assert!` in
  both: fail-fast in dev/test, unchanged panic-free release.
- **N27 (N18 re-scoped UP — scoring-correctness, resolve before D5):** hashing is off the table (TED uses
  `PartialEq` only), so N18's real substance is sharper: `Binary(&'static str)`/`Unary(&'static str)` with
  `_ => "?"` makes two DIFFERENT unhandled operators compare EQUAL → TED charges relabel 0 → inflated score
  (silent false positive, grows as `syn` adds operators). Replace with `BinOpKind`/`UnOpKind` enums for
  compiler exhaustiveness. Bonus: all `Label` payloads are `Copy` today → `Label` could derive `Copy`/`Hash`,
  making the flatten a trivial memcpy. Feeds D5/R2.

Anders end-to-end test asks for T5: (i) the N22 `len()==node_count` invariant over parsed fixtures;
(ii) `parse → prepare → distance → similarity` on two real Type-2 renamed fns asserting `score == 1.0`
(the feature's core claim, not yet proven end-to-end); (iii) a T11 prune-soundness test (filtered vs
un-filtered candidate sets identical).

### T5 review notes (Anders — APPROVE-WITH-NOTES; nothing blocked T6)

T5 verified PASS by Bhaskar (full gate, 54 tests). Resolved priors: **N12** (`Analyzed{fragment,tree}` moved
to core `model`; `parse::Extracted` deleted; core imports no adapter deps), **N8** (`Candidate{left,right,
score}` — node counts derived from `Fragment.node_count`), **N21** (`similarity_prepared` removed from
production; `similarity(delta,a,b)` sole seam), **N25** (`ted::distance_trees` now `#[cfg(test)]`), **N26**
(`debug_assert!` in ted get/set), **N11** (dead_code allow off `model`; tree/parse/ted/similarity/detect
allows justified until T7). End-to-end Type-2 `score==1.0` claim now proven.
- **N28 (T7 — threshold validation):** `DetectOptions` has no bound on `threshold`; a NaN/`2.0` threshold
  yields a silent empty exit-0 report (`score >= NaN` is false). Fix at T7 with a clap `value_parser` range
  `0.0..=1.0` (rejects NaN → usage error → non-zero exit, per A8) — NOT a core clamp.
- **N29 (T7 — defaults home):** put defaults `0.75/4/20` in the clap layer (single source; `--help` renders
  them), NOT `impl Default for DetectOptions`. If core must own the numbers, use `pub(crate) const
  DEFAULT_*` referenced by clap `default_value_t` — never both. Pin with a T7 integration test.
- **N30 (N23 placement — important):** if the human picks the node-ceiling option, it CANNOT live in `detect`
  (core is side-effect-free, A5). Put the ceiling filter + stderr diagnostic UPSTREAM in the T7 wiring, which
  drops oversized `Analyzed` before calling `detect` — zero change to detect's shape, deterministic, fits A8.
- **N31 (record):** the `kept.sort_by(canonical_key)` pre-sort is NOT load-bearing for determinism (the final
  `(left_key,right_key)` sort is already a total order → permutation-invariant). This frees T11/T12 to iterate
  in SIZE order (bucketing, early break) without touching the determinism contract. Document which (keep as
  cheap belt-and-braces or drop) so nobody assumes determinism depends on it.
- **N32 (T11 — style):** express the size-ratio pre-filter as a named pure predicate beside `passes_floors`
  (e.g. `ratio_admits(n_a, n_b, threshold)`) so T11's prune-soundness test targets a named fn.
- **N33 (record only, T12):** `PreparedTree` clones labels, so detect holds both every `NormTree` and its
  prepared copy (~2× peak). Lever if T12 shows it matters: take `Vec<Analyzed>` by value, destructure into
  `(fragments, prepared)` so trees drop before pairing. Don't change signature speculatively.
- **N34 (record only):** each `Candidate` deep-clones two `Fragment`s (owned `String` path). Fine at
  post-threshold cardinality; if S2 blows candidate counts up, `Rc<Fragment>`/index handles are the lever.
- **N35 (ledger correction — T3 notes still open):** four T3 notes have NO recorded resolution:
  **N13** (`tree.rs` doc still says "identifiers … and literal values" but types are erased — cheap doc fix,
  do at T6), **N16** (`Label::Item` comment vs `collect_items` not descending — cheap doc fix, T6), and the
  product decisions **N14** (fn qualifiers/receiver form) + **N15** (statement semicolon) — decide beside
  N27/N23 before D5.

Open-notes ledger after T5: **N10** (FragmentKind — decide at T6: stderr diagnostic or delete), **N13/N16**
(cheap doc fixes at T6), **N14/N15/N23/N27** (product/scoring decisions before D5), **N28/N29/N30** (T7),
**N31/N32/N33/N34** (record/T11/T12).

### T6 review notes (Anders — APPROVE-WITH-NOTES; nothing blocked T7)

T6 verified PASS by Bhaskar (full gate, 63 tests; byte-parity text+JSON golden strings). Closed: **N4**
(DTOs private to report, model serde-free), **N13** (tree.rs contract now "identifiers, types, and literal
values"), **N16** (Item non-extraction stated as a non-goal, not T8). `render(&[Candidate], Format) ->
Result<String>`; `Error::Render{message}` added (serde_json error folded to String — same confinement
pattern as `Error::Parse`).
- **N10 — DECIDED (driver ratifies Anders' option c): KEEP `FragmentKind`.** It IS consumed — `parse` tests
  assert `Vec<FragmentKind>` to pin Function vs Method extraction (and T8 quadruples extraction complexity;
  D7 de-scope lever filters on it). At T7, instead of a module-level allow, put a NARROW field-level
  `#[allow(dead_code)]` on `Fragment::kind` + `FragmentKind` with the justification "extraction provenance:
  asserted by parse tests, deliberately not emitted (dry4go parity has no kind field); D7 filters on it".
  This lets N11 complete honestly (all module-level allows off at T7).
- **N3 — RE-RAISED (take or close at T7):** `Error` has grown 2→3 variants; it's public. Add
  `#[non_exhaustive]` (one attribute) at T7 before the surface stabilizes, or explicitly close as "won't do,
  single-consumer crate".
- **N36 (T7 — do first, façade):** `render`/`Format`/`detect`/`parse` are all `pub(crate)`; `main` is a
  SEPARATE crate and can't call them (`lib.rs` exports only `error` + `discover_rust_files`). Widen the public
  surface as ONE façade: a single `pub fn run(...) -> error::Result<String>` in `lib.rs` owning discover →
  parse → detect → render, with `Format` + an options struct + `Error` the only other public items. Keep
  `Candidate`/`Fragment` `pub(crate)`. Better integration seam than spawning the binary.
- **N37 (T7 — confinement trap):** do NOT `#[derive(clap::ValueEnum)]` on `report::Format` (leaks clap into
  the report adapter, breaks A5 like serde would have). Define a separate `ValueEnum` in `cli.rs`, map onto
  `report::Format`; the mapping also folds `--json`/`--text` aliases.
- **N38 (T7 — output discipline):** `render` already emits the exact trailing newline, so `main` must
  `write_all` on a LOCKED stdout, never `println!` (double newline; `println!` also panics on EPIPE →
  `dry4rust | head` exits 101). Treat `BrokenPipe` as quiet success. One lock + one write = atomic stdout,
  no stderr interleaving.
- **N39 — DECIDED (driver call, grounded in A8): SKIP + DIAGNOSTIC + EXIT 0.** R4 (unparsable/unreadable file
  policy) had no recorded decision; A8 already dictates "pure reporter — always exit 0 on a successful run;
  non-zero reserved for usage/internal errors". So at T7: a per-file `Error::Parse`/`Error::Io` is caught in
  the pipeline, a DETERMINISTIC one-line diagnostic goes to stderr, the run continues and exits 0. stdout
  bytes must be unaffected by a skipped file; skip set + message order must be deterministic (R6). Reserve
  non-zero for usage errors (bad flag / out-of-range threshold) and whole-run failures. (Flagged to human.)
- **N40 (record only):** `render_json` materializes `Vec<CandidateDto>` before serializing; irrelevant at
  post-gate cardinality — noted so nobody "optimizes" it and disturbs the byte contract.
- **N23 — DECIDED for S1 (driver call): ACCEPT + DOCUMENT; revisit at S2/T8.** The unbounded n·m TED matrix
  only bites with large fragments; S1 extracts functions/methods (typically small), so no node ceiling in S1
  (YAGNI). It becomes real when T8 adds `impl` bodies — decide the ceiling (option a: upstream skip+stderr
  per N30) THEN. Cheap S1-agnostic win available anytime: TED cells fit in `u32` (δ ≤ n+m). (Flagged to
  human.)

Open-notes ledger after T6: **N3** (T7), **N14/N15/N27** (scoring/product, before D5), **N23** (revisit at
T8), **N28/N29/N30/N36/N37/N38/N39** (T7 — implement now), **N31/N32/N33/N34/N40** (record/T11/T12).
T7 forces: N36 (façade), N37 (Format mapping), N38 (stdout write), N39 (R4 policy — decided), N28
(threshold range), N29 (defaults in clap), N11 completion (module-level allows off → N10 field-level allow),
N3 (non_exhaustive).

### T7 review notes (Anders — APPROVE-WITH-NOTES; **S1 DONE: yes, conditionally**)

T7 verified PASS by Bhaskar (full gate, 85 tests; S1 acceptance cruxes — skipped-file stdout-invariance +
exact end-to-end text/JSON golden bytes — both tested). Façade `run(&RunOptions)->Result<RunOutput{report,
diagnostics}>`; public surface exactly {error, discover_rust_files, Format, RunOptions, RunOutput, run}.
**Closed by T7:** N3, N10 (as decided), **N11 (FULLY closed** — zero module-level allows; only the two
narrow justified `FragmentKind`/`Fragment::kind` allows remain), N28, N29, N36, N37, N38, N39. **N30**
dormant (prescribes ceiling placement IF N23 picks one).

**Dogfood signal (D5/R1):** `dry4rust src` (debug) found 3 real pairs incl. two `fragment(...)` test builders
at 0.87 — 0.75 fired on genuine copy-paste, no obvious false positives. BUT: debug timing (~2 min) is NOT
usable for perf decisions (N47 — re-run `--release` first, likely 3–10 s); and the flagged pairs are
macro-heavy TEST bodies (exactly what N19 predicted — macros lower to leaves).

- **N41 (façade stability — one-line decision):** `RunOptions` WILL gain a field at T8 (N23/N30 ceiling);
  `RunOutput` likely gains one at D3 (exit-gate needs a candidate count, not just a String — note the public
  seam is a *renderer*, not a *detector*). Declare both structs pre-1.0 unstable in doc comments (single
  consumer) rather than building a builder/`#[non_exhaustive]`. Also decide T12's measurement seam
  (recommend: bench end-to-end through `run` — TED dominates, IO is noise).
- **N42 (design.md drift — SSOT fix):** `lib.rs` is now the composition root that performs file IO — a fourth
  role design.md's Architecture section doesn't name (only core/adapters/bin). Add it (golden rule #1).
- **N43 (S1 determinism gap — DO BEFORE STAMPING S1):** CRLF-invariance is claimed by A7/R6 but UNTESTED.
  `read_to_string` preserves `\r\n` (works correct-by-accident). One test: `\n` vs `\r\n` fixture →
  byte-identical report. Highest-value remaining S1 test (CI runs Windows w/ autocrlf).
- **N44 (S1 coverage gap — DO BEFORE STAMPING S1):** `.gitignore`/`target/` skipping is unit-tested in
  discovery but NEVER exercised through `run`, despite being in S1's acceptance sentence. Tree with a dup pair
  + a `target/` copy + a gitignored copy → assert stdout has exactly one DUPLICATE block.
- **N45 (exit-code gap):** exit 1 (whole-run failure) untested at the BINARY level; only 0 and 2 pinned.
- **N46 (record only):** a broken STDERR pipe returns Err→exit 1, asymmetric with stdout's BrokenPipe
  tolerance. Cosmetic/rare; noted so it isn't "fixed" accidentally.
- **N47 (blocks R1 reasoning):** re-run dogfood in `--release` before any perf decision. Debug figure is not
  evidence.

**Anders sequencing recommendation:** **T11 (size-ratio prefilter) BEFORE T8**, then T8 → T9/T10 → T12.
Rationale: T11 is *admissible* (provably identical results — can't invalidate prior work or calibration),
small, and its payoff is largest exactly when T8 widens the size spread — so it must be in place BEFORE T8,
not after. Take N23's free win alongside: TED cells fit in `u32` (δ ≤ n+m), halving matrix memory. **N27 is
now BLOCKING before D5** — the `_ => "?"` operator fallback inflates scores, so calibrating a threshold
against current scores calibrates the wrong number.

**Open ledger after T7:** N14/N15/**N27** (scoring/product, before D5; N27 now blocking) · N23 (revisit at
T8, jointly w/ T11) · N31/N32/N33/N34/N40 (record/T11/T12) · N41–N47 (see above; N43/N44 before stamping S1).

**Product decisions — RESOLVED (human confirmed 2026-08-24, "all as per Anders' reco"):**
1. **Slice reorder: YES — T11 before T8.** New order: **T7b → T11 → T8 → T9/T10 → T12.** Prefilter is
   admissible (identical results), so it lands before T8 widens the size spread.
2. **N23 at T8: skip + stderr diagnostic (per N30).** Oversized-node pair emits a deterministic
   `warning: skipping …` and is dropped from candidates; run continues, stdout stays deterministic. Take the
   free N26/N23 `u32`-cell win (δ ≤ n+m) alongside.
3. **N27: fix BEFORE D5.** Operator fallback `_ => "?"` replaced with distinct `BinOp`/`UnOp` labels so scores
   reflect real operators before any threshold calibration.
4. **N14/N15: preserve the distinctions (resolve at the scoring pass, before D5).** Carry `async`/`const`/
   `unsafe fn` qualifiers + receiver form and statement-terminating semicolon into the tree so they don't
   collapse into false matches; folded into the same pre-D5 scoring pass as N27.
5. **Test-code noise: v1 reports everything** (matches dry4go). `--exclude` stays deferred (D2).
6. **T7b: land N43/N44/N45 before stamping S1 done.** Next task.

### T7b review notes (Anders — APPROVE-WITH-NOTES; **S1 DONE — CONFIRMED**)

T7b verified PASS by Bhaskar (full gate; 88 tests). **Closed: N42, N43, N44, N45.** No production code
changed — the three tests were gaps in *evidence*, not in behaviour, and all three pass without fixes
(CRLF is invariant because the report emits only normalized paths + line numbers, never source bytes;
that is now pinned rather than accidental). **S1 (Runnable CLI) is DONE**: every clause of S1's acceptance
sentence — gitignore/`target` skipping, byte-parity text+JSON, determinism, exit codes — is exercised
end-to-end. Remaining notes are doc-precision/test-tightening and cannot alter S1 behaviour.

- **N48 (design.md wording — SSOT accuracy, fix with T11):** the new Composition-root bullet says
  "everything below it stays IO-free" — **false**: `discovery` performs filesystem IO (traversal) by
  construction; the rule that actually holds is *core* is IO-free. Reword to: reads file **contents**
  (the only `read_to_string`); below it, **core** is IO-free and `discovery` is the only other module
  touching the filesystem. Also "the single public façade" is imprecise — see N49.
- **N49 (public surface drift):** `discover_rust_files` is `pub` **only** so `tests/discovery.rs` can call
  it — a test-driven public item that contradicts N36 (one façade) and least-privilege. Either fold that
  coverage into the existing `discovery` unit tests + `tests/facade.rs` and make it `pub(crate)`
  (preferred), or record it in design.md as a deliberate second public item. Decide before the surface
  hardens (ties N41).
- **N50 (SSOT gaps, cheap):** (a) the exit-code contract **0 / 1 / 2** is now *pinned by tests* and
  documented only in `main.rs` — promote a one-line table into design.md's Error-handling bullet;
  (b) the Slices table still says S3 depends on "S1, S2", stale under the confirmed T11-before-T8 order —
  mark T11 as depending on S1 only. *(b applied at T7b.)*
- **N51 (test-tightening, non-blocking, land opportunistically):** N44's test asserts *shape* (one
  `DUPLICATE`, forbidden substrings absent) where it could assert the **clean-baseline bytes** — same
  pattern already used by the skipped-file test, strictly stronger. N45 should also assert stderr is
  non-empty (exit 1 with no message would pass today). N43 would be non-vacuous *locally* if the LF side
  asserted it contains `DUPLICATE` (today it relies on the neighbouring golden test to catch a
  both-sides-empty regression).
- **N52 — ⚠ RETRACTED AS UNSOUND; see "T11 review notes" for the restated note.** ~~guard `max_nodes == 0`
  … prune only when `min < threshold × max` … compare via `(min as f64) >= threshold * (max as f64)`.~~
  Both halves were wrong — do not implement either.
- **N53 (T11 payoff, recommended):** N31 already established the pre-sort is not load-bearing for
  determinism — so iterate fragments in **node-count order** and `break` the inner loop once the ratio
  can no longer admit. That turns the pre-filter from O(n²) predicate evaluations into an early exit,
  which is where the real T11 win lives. Final `(left,right)` sort keeps output identical.
- **N55 (sequencing warning for the pre-D5 scoring pass):** N27 + N14/N15 **change node counts and
  scores** — the golden fixtures (`left_nodes: 23`, `score=1.00`, `score=0.87` dogfood) will churn. Land
  the scoring pass as ONE commit, re-baseline the dogfood **after** it, and treat any earlier calibration
  number as void. T11's prune-soundness test is score-agnostic and unaffected — another reason T11 first
  is right.

**Sequencing — unchanged and confirmed: T11 → T8 (+N23 ceiling per N30, +N26/N23 `u32` cells) →
N27/N14/N15 scoring pass → T9/T10 → D5 calibration → T12.** Do N47 (`--release` dogfood) at the *start*
of T11 so the pre-filter has a before/after number; N41 (pre-1.0-unstable doc comments on
`RunOptions`/`RunOutput`) belongs with T8, which adds the ceiling field. N46 stays record-only.

**Open ledger after T7b:** N14/N15/**N27** (scoring, before D5; N27 blocking) · N23/N30 (T8, w/ free `u32`
win) · N31/N32/N33/N34/N40 (record/T11/T12) · N41 (with T8) · N46 (record) · N47 (start of T11) ·
**N48/N49/N50(a)** (SSOT fixes, with T11) · N51 (test tightening) · N52/N53 (T11) · N55 (scoring-pass churn).

### T11 review notes (Anders — APPROVE-WITH-NOTES; nothing blocks T8)

T11 verified PASS by Bhaskar (full gate; 94 tests) after one FAIL–fix cycle. **N47 release timings:
0.99s → 0.20s (defaults), 1.22s → 0.24s (stress) ≈ 4.7–5.5×;** the old "~2 min" debug figure is void.

**N52 — RETRACTED AND RESTATED (my note was unsound; Bhaskar caught it).** The original N52 read:
*"guard `max_nodes == 0` … before dividing; and pin the boundary … prune only when `min < threshold × max`
… compare via `(min as f64) >= threshold * (max as f64)`."* **Both halves were wrong.** Do not implement
either. Superseded by:

> **N52 (restated, T11 — the admissible pre-filter, as landed).** Express the pre-filter as a named pure
> predicate beside `passes_floors` (N32) that **evaluates the frozen `similarity` itself at the minimal
> feasible delta**: `similarity(max − min, min, max) >= threshold`. Do **not** compare the algebraically
> equal ratio `min/max` against the threshold in any multiplied or divided form. Rationale:
> `threshold * (max as f64)` is a *separately rounded* product and is not conservative — for
> `min = 212, max = 685, threshold = similarity(473, 212, 685)` it yields `212.00000000000003`, pruning a
> pair the `score >= threshold` gate accepts by exact equality; empirically this dropped three genuine
> equality matches at threshold `0.6666666666666667`. Calling `similarity` makes the pre-filter boundary
> **bit-identical** to the gate rather than merely close, so no epsilon or slack term is needed.
> Soundness: δ ≥ max − min under unit costs, and `similarity` is weakly monotone non-increasing in δ *in
> f64* (both `2δ` and `min+max+δ` are exactly representable at every reachable node count, so the
> division is the correctly-rounded image of an exactly-increasing quantity; correct rounding, `1.0 − r`
> and `clamp` are all monotone). Hence `score >= threshold ⟹ bound >= threshold`, for all counts and all
> thresholds. **No `max == 0` guard**: it was unfalsifiable and redundant — `similarity(0,0,0)` takes the
> frozen function's own `denominator == 0` branch and returns `1.0`. Zero-node fragments are **not**
> production-reachable (`NormTree::node_count` is `1 + descendants`; `--min-nodes 0` only relaxes a
> filter, it cannot conjure one) — the original N52's parenthetical claiming otherwise was also wrong.
> The zero case is defensive contract coverage only and must be labelled as such in its test.

*Process point, recorded deliberately:* a review note prescribed a **literal expression** for a
numerically delicate predicate. That was the error — the note should have prescribed the *invariant*
("never prune on equality; the pre-filter boundary must coincide with the gate boundary") and left the
expression to implementation and verification. Future review notes: specify contracts, not float
expressions.

- **N56 (do with T8 — the pre-filter's two load-bearing invariants are undocumented at their source).**
  The soundness argument now lives entirely in `detect`'s doc comment, i.e. in the consumer, while both
  premises live elsewhere and are stated nowhere as contracts: (a) `ted::distance` never documents
  `δ ≥ | |T₁| − |T₂| |` under unit costs — add it to `distance`'s doc as an invariant any replacement
  engine must preserve (R1 keeps TED swappable, so this is the exact thing a swap would silently
  break); (b) `similarity`'s module doc mentions monotonicity as a property — promote it to a stated
  contract naming its dependent ("`detect`'s size-ratio pre-filter relies on weak monotone
  non-increase in δ; changing this is a pruning-soundness change, not just an R3 score change").
  Docs only; no code motion, no new module.
- **N57 (naming, cheap, T8).** `size_ratio_admits` no longer computes a ratio. Either rename to
  `could_reach_threshold`, or — preferred — split the value out: `fn best_possible_score(nodes_a,
  nodes_b) -> f64` returning `similarity(max − min, min, max)`, with the call site reading
  `best_possible_score(a, b) >= opts.threshold`. Names the *bound* as a first-class value, makes the
  parallel with the gate visually exact, and gives T12 something to instrument. Non-blocking.
- **N58 (fail-fast on the N53 coupling — do with T8).** The `break` is sound **only** because `kept` is
  sorted with `node_count` as the primary key; nothing enforces that adjacency. A future re-sort (or a
  return to N31's canonical-key-only order) silently converts the optimization into a correctness bug
  that only large corpora would reveal. Add an N26-style `debug_assert!` that `kept`'s node counts are
  non-decreasing before the loop: free in release, loud in dev/test.
- **N59 (N47 scope — bounds what T12 may claim).** ~4.7–5.5× is a **constant-factor** result on a single
  small corpus; the pre-filter's asymptotics are unexercised at this n. It does **not** discharge R1.
  Re-scope T12: a scaling curve (time vs fragment count across synthesized corpora of growing n) plus the
  documented complexity envelope — not a single-point stopwatch. Re-take numbers after T8 (R7 multiplies
  fragment count) and after the scoring pass; today's figures are a T11 checkpoint, not a T12 baseline.
- **N60 (test-doc precision, record only).** `similarity_never_exceeds_the_size_ratio_bound` asserts
  against `min/max` with a `1e-12` epsilon while production carries none. Fine — it pins the *algebraic*
  claim, not the predicate — but label it so. The predicate's own soundness is pinned by
  `a_pruned_pair_could_never_have_passed_the_threshold` and the equality counterexample test, both
  epsilon-free.
- **N30 (wording touch-up, with T8):** reads "the T7 wiring"; that is now `lib::run` (composition root per
  N42/N48). Placement decision unchanged.

**Architecture:** `detect` calling `similarity` to evaluate a bound adds no new dependency edge and does
not blur the core seam — `detect` already owns the ted+similarity composition (N21), and `similarity` is
a pure function evaluated at a hypothetical δ. Only the bound's *provenance* is under-expressed (N56).

**`tests/discovery.rs` deletion: right call.** N49 offered two doors and Dave took the preferred one; an
integration test cannot survive `pub(crate)` without re-widening the very surface it was flagged for.
Coverage is intact and in places stronger (the N44 test is now byte-equality against a clean baseline).

**Sequencing — confirmed unchanged: T8 (+N23/N30 ceiling in `lib::run`, +N26/N23 `u32` cells, +N41
pre-1.0-unstable doc comments, +N56/N57/N58) → N27/N14/N15 scoring pass (one commit, N55) → T9/T10 →
D5 calibration → T12 (re-scoped per N59).** Rejected alternative: T9 immediately after T8 — worse, the
scoring pass would churn T9/T10's golden fixtures a second time.

**Landed with T11 and closed:** N47, N48, N49, N50(a), N51, N52 (restated), N53.

**Open ledger after T11:** N14/N15/**N27** (scoring pass, before D5; N27 blocking) · N23/N30 (T8) ·
N31/N33/N34/N40 (record/T12) · N41 (with T8) · N46 (record) · N55 (scoring-pass churn) · **N56/N57/N58**
(with T8) · N59 (rescopes T12) · N60 (record).

### T8 review notes (Anders — APPROVE-WITH-NOTES)

Extraction rewrite, `MAX_SUPPORTED_NODES`, `u32` DP cells, N41/N56/N57/N58: all sound and
correctly placed. Ratified without change: the silent clamp on `max_nodes` (observationally
inert — no fragment reaches 2.1 B nodes; an error variant for an unreachable condition is the
wrong trade under N41); **no `--max-nodes` flag** (dry4go parity + YAGNI; revisit only if N66
finds real fragments near the ceiling); skipping assoc consts/types; trait definitions out;
in-body nested items opaque; `Impl(Function…)` tree shape — the root is 1 node, so it does
**not** make same-arity `impl`s trivially resemble each other.

- **N61 (correctness — blocks clean scoring; A3 scope gap).** `detect` scores every unordered
  pair, including a fragment against its own **ancestor**. For a single-method `impl`:
  `Impl(F)` (n+1 nodes) vs `F` (n nodes), δ = 1, `sim = 1 − 1/(n+1)` = **0.95 at the
  `min_nodes = 20` floor** — admitted by the pre-filter, passed by the 0.75 gate. Every
  `impl Display`/`Default`/`From` in the tree emits a DUPLICATE against its own method. Same
  for any parent/child pair with close node counts (fn whose body is one free block; closure
  that is nearly its whole fn). **T9 will not remove this:** A3 is specified as *pair-vs-pair*
  containment ("nested pair contained on **both** sides"); this is *intra-pair* containment —
  one pair whose left contains its right. R7 assumed dedup covered it; the written policy
  doesn't reach it. **Fix:** in `detect`, at admission and **pre-TED**, drop pairs whose two
  fragments share a path and whose line spans overlap. Perf win too; keeps `dedup` purely
  pair-vs-pair. Legitimate in-file clones have disjoint spans and are unaffected. Add the rule
  to `design.md` → Detect, and state the two containment notions distinctly in A3.
- **N62 (sequencing — supersedes the N55 ordering).** N61's filter must land **before** the
  N27/N14/N15 scoring pass, not with T9. A corpus where every single-method `impl` contributes
  a ~0.95 pair poisons any distribution-based calibration. New order:
  **T8b (N61 overlap filter) → N27/N14/N15 scoring pass → T9/T10 → D5 → T12.**
- **N63 (pin the assoc-const/type judgement).** Skipping them is ratified, but it is a
  *semantic commitment* of R3 weight: two `impl`s differing only in associated consts/types
  now lower identically. Pin it with an explicit parse test asserting equal trees, so the
  choice is deliberate rather than incidental.
- **N64 (record-only — flavored-block blind spot).** `unsafe`/`async`/`try`/`const` blocks are
  not extracted. `async { … }` is `ExprAsync`, not `ExprClosure`, so in async-heavy code a
  repeated task body is invisible unless its enclosing fn matches. Conservative for R7 and
  correct for v1; revisit post-D5 if dogfooding shows async clone sites being missed.
- **N65 (record-only — asymmetry).** `impl S { … }` emits an `ImplBlock` wrapper fragment;
  `trait T { … }` emits none, only its default-body methods. Defensible (trait defaults are
  the only code), but the two constructs are now scored at different granularities. Note it in
  A1 so it reads as a decision.
- **N66 (do with T8b — stale ceiling justification).** `cli::MAX_NODES`'s doc cites the S1
  dogfood's largest fragment, measured on the **function-only** set. ImplBlock fragments are
  ≈ the sum of their methods, so the ceiling is far more reachable under EXTENDED. Re-measure
  the max fragment node count on the dogfood post-T8 and restate. Also record the mitigating
  property: dropping an oversized ImplBlock is **benign** — its methods are still compared
  individually. Fold the 2000×2000 worst-pair wall-clock into N59's envelope measurement; if a
  single admitted maximal pair costs seconds, 2000 is too generous.
- **N67 (trivial).** `src/lib.rs` `run` doc: the line "The ceiling lives in this composition
  root, not in `detect`, because core is" is duplicated (lines 115–116).

**Tests to add (with T8b unless noted):**
- `detect`: a single-method `impl` yields **no** candidate against its own method; a nested
  closure yields none against its enclosing fn; two disjoint in-file clones still pair.
- `parse` (T8, now): assoc-const/type invariance (N63); trait def → N method fragments, no
  `ImplBlock`; closure body / `else` / match-arm blocks **not** emitted while a free `{}`
  statement **is**; nested closure-in-closure → both emitted; in-body nested `fn` → not
  emitted and lowers to a `Label::Item` leaf; output sorted by `canonical_key`.
- Integration: fixture with one single-method `impl` plus one real cross-file clone → exactly
  one finding.
- Scoring pass: emit a post-floor fragment-kind histogram, so we can see how many
  `Closure`/`Block` fragments survive `min_nodes` before choosing per-kind floors (don't add
  per-kind floors speculatively).

**Open ledger after T8:** **N61/N62** (T8b, blocking clean calibration) · N14/N15/**N27** (scoring pass,
after T8b) · N63/N66/N67 (with T8b) · N64/N65 (record) · N31/N33/N34/N40 (record/T12) · N46 (record) ·
N59 (rescopes T12) · N60 (record).
