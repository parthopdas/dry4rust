# Feature: Duplicate Code Detection (TED core, dry4go UX skin)
**Branch:** vibe/001-duplicate-code-detection
**Status:** WIP — **S1 DONE**; T11/T8/T8b/T8c/T9/T10 landed, **S2 DONE**. **D5 decided (threshold `0.85`)**, review repairs N87–N98 landed. Next: T12 (N59/N70/N83/N96-scoped) → T14 (third-party corpus) → D5-confirm + T13 (`min_nodes`)

## Requirements

A Rust CLI that finds near-duplicate code fragments within a Rust codebase and reports them as
candidate pairs.

- **Core algorithm:** tree-edit distance (Zhang–Shasha / APTED family) over a normalized `syn`-derived
  AST, similarity normalized to [0,1]. Explicitly chosen over dry4go's Jaccard; this breaks
  core-arithmetic parity AND parity on the score/threshold numbers (see O1, D1).
- **UX is a dry4go "skin" — parity on surface, NOT semantics:**
  - CLI: `dry4rust [options] [paths...]` with flags `--threshold`, `--min-lines`, `--min-nodes`,
    `--format text|json`, and aliases `--json` (=`--format json`), `--text` (=`--format text`).
  - Defaults are TED-appropriate: `--threshold 0.85` (see A2, D5), `--min-lines 4`, `--min-nodes 20`.
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
| T8b | S2 | **N61 intra-pair containment filter** (found at T8 review): drop pairs whose fragments share a path and whose spans overlap, at admission, pre-TED. Without it every single-method `impl` reports against its own method at ~0.95. Also N63/N66/N67. **Must precede the scoring pass (N62).** | Done | 19bc77b |
| T8c | S2 | **Scoring pass (N27/N14/N15) — ONE commit (N55).** Typed `BinOp`/`UnOp` labels; `async`/`const`/`unsafe fn` qualifiers + receiver form; block-terminating semicolon. Also N71/N72/N73/N74/N76. **All three are label-only — no node counts and no goldens changed** (N55's "change node counts" was wrong, see N77b). | Done | 9837933 |
| T9  | S2 | Containment-dedup policy (maximal-parent-wins, both-sides; identical-span dedup; deterministic tie-break). **Carries the N68 A3 restatement + N65.** **Unit:** both-sided suppression; one-sided keep; identical-span dedup. **Plus the N79 negative control:** T9 must leave the dogfood output bit-identical. | Done | fa83753 |
| T10 | S2 | **Integration:** two files each holding one single-method `impl` → pre-dedup 4 findings, post-dedup 1. **Sole end-to-end evidence for N68 (N79)** — the dogfood cannot witness it. | Done | fa83753 |
| T11 | S3 | Admissible size-ratio pre-filter (`sim ≤ min(n₁,n₂)/max(n₁,n₂)`): prune pairs below `--threshold` before TED — provably never drops a real match. **Unit:** prune-soundness (a would-be match is never pruned). **Landed as `similarity(max−min,min,max) >= threshold` — see N52 restated.** | Done | bb38a40 |
| T12 | S3 | Perf guardrail benchmark on a medium fixture; document complexity envelope. **Integration/bench.** **N96 — parameterize, do not hardcode:** the envelope is driven by fragment count **F** against an **O(F²)** base rate, and `min_nodes` is what sets F. A 20→35 floor moves the envelope *quadratically*. Report the curve **as a function of F, with `min_nodes` a stated input** — if T12 pins `min_nodes = 20` and T13 later ships 35, T12's numbers are **void**, the exact failure N77(b) and N84(b) have already inflicted twice. Parameterized, a later floor change **rescales** T12 instead of voiding it. Also carries N59/N70/N75/N83, **N84(c)** (pre/post-dedup ratio — T12 already counts TED evaluations), and in its doc pass **N93** and **N86(a)**. | Pending | - |
| T13 | S2 | **`min_nodes` floor calibration (D5 spin-off).** The **dominant** false-positive class — two unrelated builder setters, two `impl Display` bodies, two arrange/act/assert tests — sits at score **1.0**. *Dominance is attributed to its evidence:* **7 of 17 findings at `0.85` are exact-`1.00` on our own corpus at `6c3d7e6`** (same standing as the `30–40` estimate below — an estimate, not a third-party measurement). **N101 — the class is not merely unreachable at `1.00`, it is dominant just below it too:** §7b's hand-label puts **8 of the 10 survivors in `[0.85, 1.00)`** in the same shape-coincidence class, so the lever is unchanged but the evidence is wider than the exact-`1.00` population alone. **Unreachability (exact):** the gate is `>=`, so **no** threshold in `[0, 1]` — **including `1.0`** — can exclude a δ=0 pair. D5's move to `0.85` removes none of them. **N94 — three levers, not one:** the class exists because **A6 erases** identifiers, literals and types, so (i) raise `min_nodes`, (ii) raise `min_lines`, (iii) partially de-erase A6. **(iii) is REJECTED on the record:** it changes the meaning of `score` and is a breaking output change under **R3**, and it re-opens the A2/A6 label-model commitment — the very thing R3 freezes. (ii) is weaker than (i) because line count is formatting-sensitive where node count is not. **(i) is the cheapest of three, not the only cure.** Estimate: `min_nodes` should be **30–40**, not 20. **Reframes N72:** closures dying 88% at the floor is not evidence the floor is brutal, it is evidence closures sit below the information threshold where "same shape" means anything. **N95 — this row may close a decision, not just move a number:** at `30–40` the closure population (already **88% annihilated at 20**) goes to ~zero and free `{}` blocks are already **0 extracted**, so EXTENDED's two weakest granularities become dead weight — **D7 stops being a de-scope lever and becomes a cleanup**, and **A1 may need amending**. Sequence T13 knowing it can **close D7 and amend A1**. **N100 — the estimate now has one measurement against it:** on §7b's own ten survivors the `min(left,right)` node counts are `20,39,39,20,20,20,26,24,31,26` (§7c); the eight coincidences span **20–39** and the sole actionable finding sits at **26, inside them**, so a `30–40` floor takes the band from 10% actionable to **0%** and still leaves **3 of the 7 exact-`1.00` findings** alive at `30` and `35` (**1** even at `40`; the gate is `>=`, so the class does not empty until **42**). The cost is qualified: the finding it removes is #7, actionable **as a location pair** but **weakly attributed** (→ R8). §7c's projected survivor sets were confirmed against real `--min-nodes` runs on the pinned corpus, so they are exact for that run. Self-corpus, N=10 — this **contests** `30–40` rather than refuting it (a categorical rejection would overstate the sample), but T13 must move the estimate or explain the sample away. **N101 — observation feeding T13 and T14, deciding nothing now:** the dominant noise carrier in this sample is not fragment *size* but **test/harness code** — 8 of §7b's 10 survivors and most of §7's 11 drops. Test functions are node-rich (§7c: the test pairs sit at 31–39, above the estimated floor), so a 35-node floor plausibly does **not** kill a 10-line arrange/act/assert test and T13's lever may miss the population N88 found. Three options, **none chosen**: (a) nothing — users pass paths; (b) a documented "point it at `src/`, not `tests/`" recipe; (c) a `--exclude` glob or `#[cfg(test)]` skip — **new surface, YAGNI-suspicious in v1**. §7c's column is what tells us whether the floor covers this population at all. Same evidence burden as D5 (N78/N84): needs a third-party corpus → blocks on **T14**. | Pending | - |
| T14 | S3 | **Acquire and pin a third-party corpus harness.** **One artifact, four consumers:** D5-confirmation, **N78**, **N84(a,d,e)** and **T13** all block on it, and it has been deferred at every gate so far. Deliverable: a named crate + **pinned `(corpus, sha, full flag set)`** per N90, plus §8's **hand-label** and **zero-labelling** protocols written down as runnable recipes (sample size, band, the "would I factor these out?" rubric, the per-KLOC cross-crate histogram at `≥0.75` / `≥0.85` / `=1.00`). **N102 — three protocol edits inside that deliverable, and they change what the labelling can conclude.** (1) **Sample across scores, not at one threshold:** N per bucket in `[0.75,0.85)`, `[0.85,0.95)`, `[0.95,1.00)` and **`=1.00`**, reporting precision **per bucket**. A precision-vs-score curve is the only thing that can *locate* a line; a single number can only be *consistent with* one — and the observed `(0.81, 0.86)` gap is **empty**, so the single-threshold label had no resolution to give. T14 must therefore also emit the **full score histogram** of the un-sampled run, to test whether that gap is a small-N artifact. (2) **The `=1.00` bucket is mandatory and is the actual hole in the record:** T13's central claim rests on **7 findings nobody has ever hand-labelled** — §7 labelled the 11 dropped, §7b the 10 survivors, and the seven exact-`1.00` were never read. (3) **Pre-register the decision rule before labelling,** replacing §8's single `<50%` criterion with a statement about the *contrast between adjacent buckets* and about where precision crosses §4's cost asymmetry. Writing that rule after seeing the curve is **fitting**, and must be called that. (4) **Third label column — `attributed`:** does the tool's evidence match the human's reason (**R8**)? Report precision two ways, *actionable* and *actionable-and-attributed*. On §7b's own data the strict number is **0 of 10**, and that is the honest headline: without this column a detector that finds the right pairs for the wrong reasons is indistinguishable from one that works. (5) **Zero-labelling protocol: keep as written, and do not add the third column** — attribution is undefined when every hit is non-actionable by construction. Add one axis instead: bucket the cross-crate histogram **by node count as well as score**, which prices T13's floor against the coincidence null mechanically, with no human in the loop. Without T14, S3 closes with four "still owed" items and **no mechanism that will ever discharge them**. | Pending | - |

## Risks (Rx)

- **R1 (perf):** O(n²) pairs × super-quadratic TED (APTED ~O(n³) worst) → slow on large repos. Mitigate:
  `min-nodes` floor, size-ratio pre-filter (S3), size bucketing; keep TED confined so it's swappable.
- **R2 (calibration):** `0.85` (D5) is a **reasoned estimate, not a measurement** — precision/recall are
  still unvalidated on a third-party corpus (N78/N84 stand). Mitigate: `--threshold` is exposed; D5
  records the cheapest decisive test and the revisit trigger.
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
- **R8 (evidence attribution, N99):** the score is not evidence of the duplication a human would cite.
  A6 erases identifiers, literals and types, **and macro bodies are never parsed** — `parse.rs:431`
  lowers *every* `Expr::Lit` to a single `Label::Literal`, and `macro_label` (`parse.rs:583`, applied at
  `parse.rs:369/433/513`) makes each macro invocation **one leaf** carrying only its delimiter and
  emptiness (`tree.rs:19`, `tree.rs:31-34`). So a 50-line `macro_rules!` body is **one node**, and
  macro-heavy code is scored almost entirely on its wrapper. **Both directions bite:** two fragments
  differing only in literal or macro payload score exactly `1.00`, *and* a real duplicate can be
  reported with a score and `left_nodes`/`right_nodes` describing something else — a true positive
  found for an **adjacent reason** (§7b #7, whose caveat this risk is the home for). This is a standing
  property of the label model, not a defect to be tuned away. **Mitigate: document it as a limitation**
  in `docs/design.md`'s score/report description (number-free, no threshold). T13's `min_nodes`/
  `min_lines` floors do **not** address it — they change *which* fragments are scored, not *what* the
  score is evidence of — and the only lever that would, partial de-erasure of A6 (N94 (iii)), is
  **rejected under R3**.

## Assumptions (Ax)

- **A1:** Granularity = EXTENDED — free functions; methods (inherent + trait-impl + trait-default bodies);
  `impl` block bodies; closures; free `{}` blocks.
  **Wrapper asymmetry (decision, N65):** `impl S { … }` emits an `ImplBlock` wrapper fragment covering the
  whole block; `trait T { … }` emits **no** wrapper — only its default-body methods. This is deliberate,
  not an oversight: an `impl` block is *entirely* code, so its wrapper is a real, copyable unit (two copied
  `impl`s are a clone of the block, not merely of its methods), whereas a `trait` definition is mostly
  *signatures* — a wrapper over it would score trait pairs on their declaration shape (arity, receiver
  forms, name-erased types) rather than on any duplicated logic, manufacturing findings that no edit could
  remove. The consequence is accepted and bounded: the two constructs are scored at different granularities,
  so a `trait`'s duplicated default bodies are reported method-by-method while a duplicated `impl` is
  reported once at the block (T9's dedup collapses its method-level echoes).
- **A2:** TED default `--threshold 0.85` (D5; was `0.75`) with metric normalization
  `sim = 1 − 2δ/(|T₁|+|T₂|+δ)`, δ = unit-cost (ins/del/relabel = 1) tree-edit distance.
- **A3 (restated at T9 per N68):** Containment-dedup operates on **pair-vs-pair** containment only.
  Two distinct containment relations exist and must not be conflated — conflating them is what let the
  N61/N68 bug through at T8:
  1. **Intra-pair containment** — *one* pair whose left contains its right (necessarily the same file:
    a fragment against its own ancestor, e.g. a single-method `impl` vs that method). This is **not
    dedup's business**. Such a pair is removed at **admission**, pre-TED, by `detect`'s `spans_overlap`
    (N61). Dedup never sees it and could not reach it.
  2. **Pair-vs-pair containment (dedup's whole job)** — pair `P` **dominates** pair `Q` iff,
    **side-for-side after canonical assignment** (`left` = the canonically-smaller `(path,start,end)`
    fragment, `right` the larger — assigned by `detect`, so both pairs are compared in the same
    orientation and `(A,B)` vs `(B,A)` cannot arise), `Q.left ⊆ P.left` **and** `Q.right ⊆ P.right`,
    where `X ⊆ Y` means *same path* and `Y.start ≤ X.start ≤ X.end ≤ Y.end` — **equality is allowed on a
    side** — and `P ≠ Q`. A dominated `Q` is **suppressed**; maximal parents win. Equality-on-a-side is
    what removes the cross-granularity crosses: two copies of a single-method `impl` yield four findings
    (`impl↔impl`, `m↔m`, and two `impl↔m` crosses at ~0.96), and the crosses are strict on one side and
    **equal** on the other, so only clause 2 suppresses them. **Accepted cost (N82):** because the
    maximal parent wins, a reported span is the *larger* one and may include wrapper lines that are
    not themselves duplicated (the `impl` header/closing brace around a duplicated method). This is a
    deliberate trade, not an oversight: the wrapper demonstrably *contains* the duplicate, and
    reporting the fragment-level pair instead would restore the redundant nested findings A3 exists
    to remove.
  3. **"Keeps both" narrows to:** containment on one side and **disjoint or partially overlapping** on
    the other. That is a genuine second finding, not an artifact, and is retained.
  4. **Identical on both sides** ⇒ collapse to one, keeping the **highest-scoring** pair. This clause
    is *reachable from `detect`*, not defensive: two **distinct** fragments can share a line span (an
    `impl` and its only method when they share a closing line), so `detect` — which emits each
    unordered *fragment* pair once — can still emit several *candidate* pairs carrying the same
    `(left.(path,start,end), right.(path,start,end))` key. Those pairs contain each other, so without
    a tie-break they would annihilate and the finding would vanish. The key cannot break its own tie,
    so the tie-break runs over the **rendered values** in order: score first, then the two node counts
    (emitted as `left_nodes`/`right_nodes` — higher wins; same-span candidates really do differ here,
    since a brace-sharing `impl` has exactly one node more per side than its only method). **Incoming
    position is the last resort** and is reached only for candidates whose spans tie, whose scores
    compare neither `Greater` nor `Less`, *and* whose two node counts tie. For **every score
    `similarity` can produce** — all of them finite — that middle condition *means* the scores are
    equal, so such candidates render identically; they can still differ in `kind` and `line_count`,
    which are deliberately not emitted (N10/N8) — so the *rendered report* is a function of the
    candidate **set**, while *which* struct instance survives is not. Were `kind` or `line_count`
    ever emitted, they would have to join the tie-break. The one exception is excluded on
    reachability, not absorbed: a `NaN` score reaches the fallback without tying, and still renders
    differently (`NaN` vs a finite score) at identical spans and node counts — but `similarity`
    cannot produce one, since every `usize → f64` conversion in it is
    finite, the sole zero-denominator case returns `1.0`, the denominator is otherwise strictly
    positive, and the result is clamped to `[0,1]`.

  Dominance so defined is a strict partial order (span containment is transitive, and mutual dominance
  forces span equality, handled by clause 4), so the surviving set is exactly its maximal elements —
  no cascade or fixed-point iteration is needed. Irreflexivity needs no separate `P ≠ Q` guard: a pair
  against itself runs all three tie-break steps — equal score, equal node counts — and the final
  positional comparison is strict at an equal position, so it is `false`.
- **A4:** Discovery honors **in-tree** `.gitignore` via `ignore` (machine-global/parent gitignores
  disabled for cross-machine determinism — see N5); scans `*.rs`; **skips any directory named `target`
  at any depth** (accepted over root-only `/target` — correct for nested-workspace `target/` dirs; N7).
- **A5:** Layout = 1 lib + 1 bin. Adapters (`syn`/`ignore`/`clap`/`serde`) confined to their modules;
  core (`tree`/`ted`/`similarity`/`dedup`/`detect`) is pure std-only.
- **A6:** Label model = structural `syn` node kind with identifiers/literals canonicalized, so Type-2
  (renamed) clones match; structure is preserved, including `fn` qualifiers, receiver form
  (`self`/`&self`/`&mut self`, taken from the effective self type so `self: &Self` ≡ `&self`; other
  typed receivers are `Owned`), and the block's terminating semicolon. (consequence: **R8**)
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
- **D5:** Threshold calibration — **DECIDED: default `0.75` → `0.85`** (ratified by the human). Recorded
  in full below (see "D5 decision record"). The corpus measurement N78/N84 require is **still owed**: this
  is a reasoned estimate, not a measurement.
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
- **N29 (T7 — defaults home):** put defaults `0.75/4/20` in the clap layer **(now `0.85/4/20` per D5)**
  (single source; `--help` renders
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
at 0.87 — 0.75 fired on genuine copy-paste, no obvious false positives. **[VOID per N77(b) / N91(c) — this
is the only earlier note that reads as *evidence for* `0.75`, and it is not: the counts predate T8's
EXTENDED extraction and T8b's overlap filter, and "no obvious false positives" was an unrecorded
eyeball, not a hand-label. Do not cite it in D5 or T13.]** BUT: debug timing (~2 min) is NOT
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

### T8b review notes (Anders — APPROVE-WITH-NOTES)

**N61 closed in code, open in docs.** `spans_overlap` at `detect` admission is the right layer:
it is an admissibility predicate over the *input pair set*, peer to `passes_floors` /
`best_possible_score`, needs only `Fragment`, keeps core pure, and spends no TED on artifacts.
`dedup` is pair-vs-pair post-scoring and could not have reached it. Ordering (size-ratio `break`
first, overlap `continue` second) leaves the N53 row-terminator decision set unchanged — pinned.
Applying the skip to `reference_detect` was correct: N61 is semantics, so it belongs in the
baseline; the differential test keeps bounding T11 alone. Ratified: 16 → 13 dogfood, all three
removals ancestor/descendant; no golden churn.

- **N68 (A3 — restate; second scope gap; do with T9, `dedup.rs` is still a stub so it is free).**
  N61 asked for A3 to distinguish the two containment relations; A3 is unchanged, and the gap has
  already bitten: two copies of a single-method `impl` in different files yield **four** findings —
  `impl↔impl`, `m↔m`, and two `impl↔m` crosses at ~0.96. Only `m↔m` is *strictly* contained on both
  sides; the crosses are strict on one side and **equal** on the other, which A3's "one-sided
  containment keeps both" currently reads as *keep the artifact*. A3 must state:
  1. **Intra-pair** containment (one pair, left contains right, same file) — not dedup's business;
     removed at admission by N61.
  2. **Pair-vs-pair** containment — P dominates Q iff, side-for-side after canonical assignment,
     `Q.left ⊆ P.left` **and** `Q.right ⊆ P.right` (same path; span containment; **equality allowed
     on a side**) and `P ≠ Q`. Q is suppressed. This is what removes the cross-granularity crosses.
  3. "Keeps both" narrows to: containment on one side, **disjoint or partially overlapping** on the
     other.
  4. Identical on both sides ⇒ de-duplicate, keep the canonically-first `(path,start,end)`.
  **T10 fixture:** two files each holding one single-method `impl` ⇒ pre-dedup 4, post-dedup **1**.
  While in A1/A3, also land N65's `impl`-emits-a-wrapper / `trait`-does-not asymmetry as a stated
  decision.
- **N69 (record-only — line-granular overlap can false-negative).** `spans_overlap` is line-based, so
  two genuinely disjoint fragments sharing one boundary line (`fn a(){…} fn b(){…}` on one line) are
  dropped. Accepted: rustfmt makes it near-unreachable, the failure is a miss not a false report, and
  byte spans cost more than the case is worth. Revisit only if D5 shows real misses.
- **N70 (N66 residual — do with T12/N59).** 439 (Function) vs 244 (ImplBlock) falsifies N66's premise
  and confirms 2000; R1 and N59's T12 scope are unchanged. But (a) the number is **corpus-specific** —
  say so where it is cited, a 900-line `impl` elsewhere is not excluded — and (b) N66's other half is
  still open: the **2000×2000 worst-admitted-pair wall-clock** was not measured. Fold it into N59's
  envelope; if one maximal admitted pair costs seconds, 2000 is too generous. T12's synthesized
  corpora should include one near-ceiling fragment.
- **N71 (trivial, with the scoring pass).** Extend `reference_detect`'s doc: because both sides call
  the same helper, this test bounds **T11 only** and does **not** cover N61 — N61's coverage is the
  predicate pin plus the three behavioral tests. Prevents a later reader trusting it for the wrong
  property.
- **N72 (scoring-pass deliverable).** Add to the N27/N14/N15 histogram a count of surviving
  **cross-granularity cross-file** pairs (`ImplBlock↔Method`, `Function↔Closure/Block`) alongside the
  per-kind counts, so T9's effect is measurable and D5 can be read against a known multiplicity.

**Calibration readiness: GO.** N61 removed *fabricated* signal — a noise floor proportional to
`impl` count, present with or without real duplication. What remains is **multiplicity around true
positives** (4× instead of 1×), clustered ~0.96, well above any candidate cut: it cannot move where
the threshold lands, only inflate raw counts. Sequencing therefore **holds unchanged**:
**N27/N14/N15 scoring pass (one commit, N55) → T9/T10 (with N68) → D5 → T12 (N59-scoped).** Any
pre-T9 dogfood count must be labelled inflated near genuine clones.

**Landed with T8b and closed:** N61 (code), N63, N66 (measurement half), N67.

**Open ledger after T8b:** N14/N15/**N27** (scoring pass, next) · N55 (churn) · **N68** (A3, with T9)
· N70 (with T12) · N71/N72 (with scoring pass) · N64/N65/N69 (record; N65 folded into N68) ·
N31/N33/N34/N40 (record/T12) · N46 (record) · N59 (rescopes T12) · N60 (record).

### T8c review notes

**Verdict: APPROVE-WITH-NOTES. N73 landed inside the T8c commit (N55 = one commit); the rest are ledger, doc and record.**

The three encodings are the right R3 re-commitment. **Label-only was correct for N15, and for a stronger reason than argued:** `left_nodes`/`right_nodes` are a *frozen R3 output field*, so the naive "wrapper node per statement" reading would not merely inflate counts — it would break the output contract, and dilute every denominator corpus-wide, to encode non-distinctions. Rejecting it is sound and nothing is quietly lost: a non-final statement expression is unit-typed either way, so only the terminating semicolon carries meaning. Cost of the real distinction stays at 1 relabel per block, independent of depth. N14/N27 likewise preserve arity, so nothing reaches the denominator. **T8c moved two scores and zero node counts — exactly the shape a correct R3 re-commitment should have.**

- **N73 (defect — FIXED inside T8c).** `syn::Receiver::reference` is `None` for *any* typed receiver, so `fn m(self: &Self)` lowered to `Owned`: equal to `fn m(self)`, unequal to `fn m(&self)` — **backwards**. Not a type-erasure tradeoff but a wrong mapping: `self: &Self` and `&self` are the *same signature* to rustc, and the code lowered identical signatures differently while collapsing different ones together. Erasure is about *which* type, never about *whether it is a reference* — `Label::Reference(Mutability)` already preserves exactly that everywhere else. **Contract (invariant, not expression):** receiver form derives from the *effective* self type — `self: &Self`/`self: &S` ≡ `&self`, `self: &mut Self` ≡ `&mut self` (mutability from the **reference**, never `receiver.mutability`, which means `mut self: T`), every other typed receiver (`Box<Self>`, `Rc<Self>`, `Arc<Self>`, `Pin<&mut Self>`) ≡ `Owned` with its type erased. Pinned in `a_typed_reference_receiver_matches_its_sugared_form`, including `mut self: &Self` ≡ `&self` (Bhaskar's gap catch — the subtlest rule, and the only one a `receiver.mutability` regression would silently pass). **The label-model commitment lives in A6, not A2** — A2 owns only the normalization formula.
- **N74 (record + doc, done).** `tail` is *syntactic*, not semantic: `fn f(){ if a { b(); } }` and `fn f(){ if a { b(); }; }` are semantically identical yet lower differently. Unavoidable mirror of N15 — value-ness needs type inference, which `parse` does not and must not have. Documented on `Label::Block` beside the macro limitation, so it reads as a known bound.
- **N75 (deferred to T12).** Every `Label` payload is now `Copy`. Deriving `Copy` on `Label` turns `PreparedTree`'s per-fragment label flatten into a memcpy — the cheap half of N33. **Not free:** it trips `clippy::clone-on-copy` at `src/ted.rs:80`, so it is a two-line change (derive + drop the `.clone()`) outside T8c's blast radius. `NormTree` cannot take `Copy` at all (`children: Vec<NormTree>`), so `Label` is the only candidate.
- **N76 (done).** `_ => Other` still aliases two future `syn` operators to one label, silently and forever. `debug_assert!(false, …)` in both wildcard arms: free in release, loud the first time a corpus produces one. Both arms verified unreachable against `syn 2.0.119`.
- **N77 (ledger correction — two prior notes rested on a false premise).**
  (a) **N27: CLOSED AS LATENT, not as a fix.** All 28 `BinOp` and 3 `UnOp` variants were already distinctly mapped at `7738f5e`; the `_ => "?"` arm was unreachable. **T7's "N27 is now BLOCKING before D5 — the fallback inflates scores" was wrong on its premise and its conclusion.** N27 moved no score on any corpus; it converted a future-aliasing risk into compiler exhaustiveness. No past calibration number was ever wrong because of it.
  (b) **N55 is half-false.** "N27 + N14/N15 **change node counts** and scores" — they do not: all three are label-only, no constructor cardinality changed, goldens are 23/23 and score 1.00 unchanged. Only N15 moved output, on exactly two dogfood pairs (δ 5→6, `1−10/51` → `1−12/52`, counts 23/23), neither crossing the threshold nor reordering. Earlier dogfood numbers *are* void — but because of T8's EXTENDED extraction and T8b's overlap filter, **not** the scoring pass.
  (c) **The pre-D5 sequencing conclusion survives, re-based.** It now rests on N15 alone, which demonstrably reaches the pipeline. Nothing else in the D5 reasoning depends on the false premise.
- **N78 (N72 reading — this bounds what D5 may calibrate on).** Pre-floor 245 → post-floor 109 (Function 84 / Method 13 / ImplBlock 6 / Closure 6 / Block 0), **zero cross-granularity survivors**. Does EXTENDED earn its keep? On this corpus: no evidence for, no evidence against — the extra granularities are 12 of 109 post-floor fragments (11%) and produced **zero findings**; closures are 88% annihilated by the global floor and free `{}` blocks are 0 *extracted*, because idiomatic Rust barely has them. Consequences: (i) **no per-kind floors** — `--min-nodes 20` already does the whole R7 job, and per-kind machinery on a corpus where nothing survives is pure YAGNI; (ii) **A1 unchanged, D7 not pulled** — keeping closures/blocks costs 6 fragments, the de-scope lever is cheap to hold, and the evidence is a single small self-corpus; (iii) **D5 must not calibrate on the dogfood alone** — 109 fragments with an unrepresentative granularity mix (0 Blocks, 6 Closures) cannot support a threshold decision. **D5 needs a second, larger, third-party corpus before `0.75` is confirmed or moved.** — **(iii) STAMPED per N92: consciously overridden by human ratification at D5; evidence still owed.** D5 moved the default to `0.85` **without** that corpus, on ratification, not on measurement. The gate was **waived deliberately, not respected and not forgotten**; the obligation survives the waiver and is carried by **T14**.
- **N79 (T9/T10 readiness — the burden of proof shifts, but the dogfood is not useless).** With zero cross-granularity survivors the dogfood **cannot witness N68's four-findings scenario**, so **T10's fixture is the sole end-to-end evidence for N68** — build it exactly as N68 prescribed (two files, one single-method `impl` each ⇒ pre-dedup 4, post-dedup 1) and treat it as a first-class acceptance artifact, not a smoke test. `dedup` is pure core, so A3's policy proof belongs in T9 **unit** tests over synthetic `Candidate`s — no corpus needed. But the dogfood upgrades from "useless" to **negative control**: because nothing in `src` is both-sided-contained, **T9 must leave the dogfood output bit-identical.** Assert it. It is the cheapest guard against A3's "equality allowed on a side" (N68 clause 2) over-reaching and suppressing legitimate exact-duplicate pairs — the one way T9 can silently do damage.

**Sequencing — unchanged: T8c (+N73) → T9/T10 (with N68, N74, N79) → D5 (N78-scoped) → T12 (N59/N70-scoped).**

**Verification baseline: 117 tests** (lib 90 + bins 7 + cli 9 + facade 11), debug and release. An earlier report of 116 was a *falsified-run* passed count, not a clean baseline.

**Landed with T8c and closed:** N27 (as latent — N77a), N14, N15, N71, N72, N73, N74, N76.

**Open ledger after T8c:** **N68** (A3 + N65, with T9) · N74/N79 (with T9) · **N78** (scopes D5) · N75 (T12) · N70 (with T12) · N77 (correction, applied) · N64/N69 (record) · N31/N33/N34/N40 (record/T12) · N46 (record) · N59 (rescopes T12) · N60 (record).
### T9/T10 review notes

**Verdict: APPROVE-WITH-NOTES. A3 as restated is correct and complete; the code implements exactly it; the strict-partial-order/one-pass argument is sound. All notes are test, doc, ledger or record.**

Verified by hand: transitivity holds across all four mixed cases (strict∘strict, strict∘tie-break, tie-break∘strict, tie-break∘tie-break) — a tie-break edge preserves the span key, so every mixed chain reduces to strict containment; within a span-group the ladder is lexicographic on `(−score, −(nₗ,nᵣ), index)` with unique indices, hence a strict total order. Survivors are the maximal elements; **no fixed-point pass is needed**. Removing the `P ≠ Q` guard was right: a clause no test can distinguish from the tie-break is a mutation-shaped liability (same family as `max == 0` at T11 and `_ => "?"` at T8c), and irreflexivity now rests on the strict `outer_index < inner_index`, which `a_pair_does_not_dominate_itself` does pin. Three unqualified rounds of FAIL on one task is the process working, not overhead.

**Clause 4's geometry is a reporting-resolution fact, not an extraction defect.** `ImplBlock` and its only method are genuinely distinct trees (one node apart, scoring differently against every other fragment), so collapsing them in `parse` would be wrong — they must stay distinct fragments. What collapses is only their *rendered projection*: A1 emits at node resolution, `report` renders at line resolution, and clause 4 reconciles that lossy projection. Same root as N69. **Dedup is the right layer** — it is the only stage that sees pairs. Rejected alternative, recorded: "skip the wrapper for a single-method `impl`" would erase the copied-block reading and move R3 node counts for no user-visible gain.

**Tie-break, ratified.** "Higher node counts win" is clause 2's maximal-parent rule applied where line spans can no longer distinguish — the geometry changes, the semantics don't. Freeze **the rule** ("the larger unit survives, consistently with maximal-parent"), pinned by `identical_span_pairs_with_equal_scores_collapse_to_the_higher_node_counts`; do **not** promote it to a new R3 field. `"the rendered report is a function of the candidate set; which instance survives is not"` is accepted as the pre-1.0 contract — stronger than it looks, since it makes byte-determinism unconditional while leaving struct-level choice free. It is conditional on `kind`/`line_count` staying unemitted (N81). Excluding `NaN` on **reachability** rather than absorbing it is the correct call: `similarity` clamps to `[0,1]`, returns `1.0` on the sole zero denominator, and every `usize → f64` there is finite.

**N65 as landed: adequate.** The rationale identifies the real harm — scoring declaration shape manufactures findings no edit can remove, and unfalsifiable output is worse than missing output.

**T10 and the façade inversion: correct.** T10 is exactly as prescribed and is a first-class acceptance artifact. The ruling on the inverted probe is right on both halves: the inversion is legitimate, and the inverted form is vacuous alone, so the comment must not claim to be N51's evidence.

- **N80 (test gap — LANDED with T9).** The one-pass design is correct *because* `any()` scans suppressed pairs too; transitivity was asserted in three doc comments and pinned by nothing. Added `a_three_deep_containment_chain_collapses_to_the_outermost` (`A ⊃ B ⊃ C`, all six permutations) and `the_tie_break_winner_suppresses_what_the_losing_twin_contained` (the tie-break *loser* strictly contains a third pair; the winner must still suppress it, all six permutations). **Verification refinement:** the *backward-looking* optimization (scan survivors only) is unsound and both new tests kill it — it retains 2–3 candidates in four of six permutations. The *forward-looking* one (`!suppressed[other_index]`, original indices preserved) is genuinely semantics-preserving and correctly survives: a non-maximal candidate always has a maximal dominator, and a maximal dominator can never itself become suppressed. Confirmed against 108,384 exhaustive finite-score cases. **The load-bearing property is full-set, order-independent consideration** — not that suppressed candidates remain eligible as dominators.
- **N81 (tripwire — LANDED).** "Rendered output is a function of the candidate set" holds only while `kind`/`line_count` stay unemitted. Cross-references now sit on `CandidateDto` and beside the JSON golden, pointing at `dedup::dominates`: any new rendered field must join clause 4's tie-break.
- **N82 (A3 — LANDED).** Dedup keeps the **larger** span, so a reported span may include wrapper lines that are not themselves duplicated. Accepted: the wrapper does contain the duplicate, and reporting the fragment-level pair instead would restore the very nested findings A3 removes.
- **N83 (T12/N59 — recorded in `design.md`).** Nesting depth d at a clone site costs **d² TED evaluations** to yield one finding, all paid before dedup — the *report* is clean, the *cost* is not. It cannot move pre-TED: a dominated pair must survive if its dominator fails the gate. Fold the multiplicity into T12's envelope.
- **N84 (scopes D5, with N78).** (a) N78 stands **unchanged** — the bit-identical negative control proves the dogfood distribution did not move, so a second, larger, third-party corpus is still required before `0.75` is confirmed. (b) New: the dogfood drifts with our own commits, so every D5 number must be a pinned `(corpus, sha)` pair, as N79's control already was. (c) Report the pre/post-dedup ratio as a corpus statistic. (d) **Partition counts test vs non-test** — arrange/act/assert repetition is idiomatic, and pooling it skews the distribution. (e) Do **not** raise the threshold to silence test clones, and do **not** add an exclude-tests flag (YAGNI, off dry4go parity, hides true positives).
- **N85 (record-only — dogfooding our own tests).** The 1.00 between `dedup.rs:324-351` and `411-451` is a **true positive**; the duplication is deliberate (different pins) and leaving it is right. It is the cleanest available evidence that the tool cannot infer intent — which is exactly why A8 makes it a pure reporter. Cite it in D5 rather than fixing it.
- **N86 (trivial).** (a) **Open, with D5:** N65 should state its revisit trigger (a trait-heavy corpus showing missed block-level clones) and the count asymmetry it implies — a duplicated `trait` with k default bodies yields k findings where a duplicated `impl` yields 1. (b) **LANDED:** `design.md` → Dedup now reads "(irreflexive via the positional tie-break — no separate guard)". (c) **Open, record only:** `dedup` could take a keep-mask + `into_iter` and clone nothing; inert at these n. (d) **LANDED:** the façade scenario is split into `a_single_method_impl_does_not_pair_with_its_own_method` (N61) and `a_copied_single_method_impl_reports_only_the_maximal_impl_pair` (N51 caveat), fixture hoisted to `SINGLE_METHOD_IMPL`.

**Calibration readiness: GO, with N84's scope.** T9 closes the last semantic mover; the pipeline is frozen enough to calibrate. Nothing in S2 remains open against D5 except corpus selection.

**Verification: 132 tests** (lib 103 + bins 7 + cli 9 + facade 13), debug and release, zero deleted. N79 negative control bit-identical on the fixed `5e2dc58` corpus (`src --threshold 0.75 --format text` → 906 bytes / 13 findings / 3 exact, sha `E963…3C89`). **[Recipe back-annotated per N90: `--threshold 0.75` was implicit at the time and is now written out, so the run stays reproducible after D5 re-based the default to `0.85`.]**

**Landed with T9/T10 and closed:** N68 (A3 restated), N65 (in A1), N79 (negative control taken, bit-identical), N80, N81, N82, N86(b), N86(d).

**Open ledger after T9/T10:** **N78/N84** (scope D5) · N83 (with T12) · N86(a) (with D5) · N75/N70 (T12) · N64/N69/N85/N86(c) (record) · N31/N33/N34/N40 (record/T12) · N46 (record) · N59 (rescopes T12) · N60 (record).

### D5 decision record — default `--threshold` `0.75` → `0.85`

**DECIDED and ratified by the human.** Two independent architecture reviews converged on `0.85`. This
section is the reasoning of record; A2/R2/Requirements now state `0.85`. Earlier review notes below
still read `0.75` — they are the dated record of what was true when they were written and are
deliberately **not** rewritten.

**Confidence, stated honestly: ~65–70% on `0.85` over `0.75`; ~90% on the direction of the move.**
This is a **reasoned estimate, not a measurement.** N78/N84 are **not** discharged.

#### 1. The δ-budget inversion

From A2's `sim = 1 − 2δ/(|T₁|+|T₂|+δ)`, solving for the largest edit distance a threshold `s` tolerates
gives **δ_max = N(1−s)/(1+s)** with `N = |T₁|+|T₂|`; for equal-size trees (`N = 2n`),
**δ_max/n = 2(1−s)/(1+s)**:

| threshold | δ per node | δ at n=20 | δ at n=200 |
|---|---|---|---|
| 0.75 | 28.6% | 5.7 | 57 |
| 0.80 | 22.2% | 4.4 | 44 |
| **0.85** | **16.2%** | **3.2** | **32** |
| 0.90 | 10.5% | 2.1 | 21 |

At roughly 5 nodes per typical Rust statement, `0.75` on a 200-node function permits about **11
statements changed** — and that is *on top of* A6's identifier, literal and type erasure, which are
already free. That is not a defensible duplicate report.

#### 2. The size-ratio identity — the most legible statement of what the threshold means

The cheapest way to accumulate δ is pure insertion, so `δ ≥ |n₁−n₂|`, giving the exact bound
**`sim ≤ n_small / n_large`**. A threshold `t` is therefore *identically* a cap on size ratio: two
fragments may differ in node count by at most `1/t − 1`. **`0.75` admits a 33% size difference;
`0.85` admits 17.6%.**

**Cross-reference:** this is the same identity as T11's admissible pre-filter, arrived at
independently. That matters — it means the pre-filter is not merely an optimization, it is a statement
of the metric's own ceiling.

#### 3. Why the band below 1.0 is where the risk lives

Under A2's normalization a genuine **Type-2** clone has **δ = 0 and scores exactly `1.00`** — renaming,
re-typing and re-constanting are all free (A6). True-positive mass therefore concentrates at `1.00`
with a thin Type-3 tail downward. Meanwhile the count of *unrelated* pairs within edit distance δ grows
combinatorially in δ, against an O(F²) base rate of pairs. Widening `0.85 → 0.75` nearly doubles the
δ-ball radius (`0.162n → 0.286n`), multiplying coincidence volume far faster than it recovers true
positives. The band **[0.75, 0.85)** is precisely where the marginal report is most likely
shape-coincidence and least likely an edited copy.

#### 4. Cost asymmetry — why the default sits above the F1-optimal point

There is no auto-fix, no per-site suppression, no baseline file (D2/D3 are out of v1) and A8 makes the
tool a pure reporter. A user cannot silence a wrong hit — only lower their opinion of the tool. A
missed clone costs nothing observable. Precision is the product, so the default belongs strictly
**above** the F1-optimal point.

**Revisit triggers (record them, do not forget them) — there are two, and the second is the larger:**

1. **Suppression / baseline.** If per-site suppression or a baseline file ever ships (D2/D3), the honest
   default drops to ~`0.80` and this decision must be revisited.
2. **Label model (N87d).** R3 freezes the **formula**, not the **label model**. `0.85` is a statement
   about a δ *distribution*, and that distribution is produced by A6's erasure and A1's granularity set.
   **Any A6 de-erasure (see T13's rejected lever (iii)) or any A1 granularity change moves the δ
   distribution and voids `0.85` outright** — a strictly larger trigger than (1), which only shifts it.
   T13 is the live candidate: if it amends A1 (N95), this number is re-opened, not merely re-tuned.

#### 5. Correction worth recording

The δ-in-denominator is a **low-threshold softener, not a small-δ one**. The budget exceeds a naive
`1 − δ/n̄` by exactly `2/(1+s)` — **+14% at `0.75`, only +5% at `0.90`**. So `0.75` was looser than it
looked, and the correction is largest exactly where we were least entitled to it.

#### 6. Dogfood delta — exact commands, pinned `(corpus, sha, flags)` (N84b, extended by N90)

**Corpus:** `dry4rust/src` at **`6c3d7e6`** (the D5 commit's source edits are doc-comment-only and move
no fragment — verified: the `--threshold 0.75` run below is **byte-identical** to the `6c3d7e6` baseline).

**N90 — the standing rule, strengthened from N84(b): every recorded number is a `(corpus, sha, full
flag set)` triple, never `(corpus, sha)`.** N84(b) is insufficient because a *default* is not part of a
recipe: N79's control was recorded as `src --format text` with the threshold **implicit**, so post-D5
that same command silently re-bases to `0.85` and can never again reproduce 906 bytes / 13 findings.
The threshold is now written out in N79's line (back-annotated `--threshold 0.75`), and the runs below
already state every flag. **Write the flags even when they are the defaults of the day.**

```
cargo build --release
target/release/dry4rust src --threshold 0.75 --format text   # old default
target/release/dry4rust src --threshold 0.85 --format text   # new default
```

| run | findings | bytes | sha256 (first 8) |
|---|---|---|---|
| `--threshold 0.75` (old default) | **28** | 1927 | `C7FC158F` |
| `--threshold 0.85` (new default) | **17** | 1172 | `A3B8ECF7` |

**Delta: −11 findings (−39%).** Every dropped pair lies in the band **[0.75, 0.85)**; nothing at or
above `0.85` moved, and the seven exact-`1.00` findings are untouched in both runs.

**N89 — reconciling 28 here with N79's 13 at the same threshold.** N79 pins **13 findings / 906 bytes**
at `5e2dc58`; this section pins **28 findings / 1927 bytes** at `6c3d7e6`, both at `--threshold 0.75` —
and **T9's containment dedup landed between them**. Read carelessly that says a *suppressing* filter
more than doubled the output. It did not: **the corpus grew.** `git diff --stat 5e2dc58 6c3d7e6 -- src`
is **+542/−4 lines across `dedup.rs` (+528), `lib.rs` (+6) and `report.rs` (+12)** — almost entirely
`dedup.rs`'s own test module, which we wrote two commits earlier. The two numbers are not comparable;
only a same-sha pair is (this is exactly what N90 now makes a standing rule).

**And it bounds the headline honestly:** **8 of the 11 dropped pairs are those very `dedup.rs` tests.**
So **−39% is measured on a corpus whose newest file supplies most of the delta** — a file we authored,
deliberately parallel (N85), two commits before measuring it. The direction of the move does not depend
on that, but the *magnitude* is a self-corpus artifact and must not be quoted as a general effect size.

**Test/non-test partition (N84d)** — `#[cfg(test)]` boundaries: `dedup.rs:140`, `detect.rs:191`,
`discovery.rs:134`, `parse.rs:674`, `report.rs:139`.

| | both-test | both-non-test | mixed | total |
|---|---|---|---|---|
| at `0.75` | 19 | 5 | 4 | 28 |
| at `0.85` | 10 | 3 | 4 | 17 |
| dropped | 9 | 2 | 0 | 11 |

Per N84(e) the threshold was **not** chosen to silence test clones, and no exclude-tests flag was
added: the move is justified on §1–§4 and drops non-test pairs too.

**N84(c) — pre/post-dedup ratio: NOT reported.** The façade emits only post-dedup candidates and there
is no flag or diagnostic exposing the pre-dedup count. Obtaining it needs instrumentation, which is out
of D5's scope; it is carried forward to **T12**, whose envelope work (N83) already has to count TED
evaluations and therefore has the number in hand.

#### 7. Hand-label of the dropped band — corroborative, **not** the owed measurement

All 11 dropped pairs were read. Applying the "would I factor these out?" test:

- 7 × `dedup.rs` test-body pairs (e.g. `208-215 ↔ 238-247` @ 0.81) — parallel A3-clause pins whose
  similarity *is* the point (N85). **No.**
- 1 × `dedup.rs:370-391 ↔ 402-426` @ 0.81 — the N80 permutation tests, same reading. **No.**
- 1 × `discovery.rs:212-219 ↔ 222-229` @ 0.77 — two unrelated arrange/act/assert tests. **No.**
- 2 × non-test `parse.rs` visitor methods (`95-102 ↔ 167-183`, `118-125 ↔ 167-183`, both @ 0.77) —
  "call `push`, then recurse" is the `Visit` trait's shape, not a copied fragment. **No.**

**0 of 11 actionable → 0% precision in the band**, which by §8's own criterion **would satisfy §8's
criterion if taken on a third-party corpus**. This is **our own corpus**, so per N78 it corroborates and
cannot decide. Recorded for exactly that weight.

**It also demonstrates T13's argument directly:** `dedup.rs:253-261 ↔ 431-440` scores **1.00** and
`238-247 ↔ 253-261` scores **0.97** — the *same* non-actionable class as the pairs `0.85` dropped,
sitting above any survivable threshold. No threshold reaches them — §10/T13 records the **three** levers
that do, and why two of them are rejected.

#### 7b. Hand-label of the **surviving** band `[0.85, 1.00)` (N88) — the untested half

§3's load-bearing premise is that true-positive mass sits at `1.00` with a thin Type-3 tail. §7 tested
that premise **below** the new line only. The 10 survivors in `[0.85, 1.00)` had never been read, so
they were read now, with the same rubric and the same honesty.

**Pinned run — `(corpus, sha, full flag set)` per N90.** Corpus `dry4rust/src` at **`6c3d7e6`** (working
tree carries only D5's doc-comment and default-literal edits, neither of which moves a fragment:
comments are not nodes and literals are erased by A6):

```
cargo build --release
target/release/dry4rust src --threshold 0.85 --format text
```

17 findings: 7 at exactly `1.00`, **10 in `[0.85, 1.00)`**. All 10, with "would I factor these out?":

| # | pair | score | partition | reading | actionable |
|---|---|---|---|---|---|
| 1 | `dedup.rs:180-183 ↔ detect.rs:47-50` | 0.90 | mixed | two `sort_by` comparator closures on *different* keys (`(left,right)` vs `(node_count,key)`) — the idiomatic tuple-`cmp` shape | **No** |
| 2 | `dedup.rs:238-247 ↔ 253-261` | 0.97 | both-test | A3 clause-3 pins: one-side-containment + disjointness vs + partial overlap. Parallel by design (N85) | **No** |
| 3 | `dedup.rs:238-247 ↔ 431-440` | 0.97 | both-test | same class, vs `spans_in_different_files_are_never_contained` | **No** |
| 4 | `detect.rs:47-50 ↔ 100-103` | 0.90 | both-non-test | the size-order pre-sort vs the canonical-key output sort — same shape, different keys, both load-bearing (N31/N58) | **No** |
| 5 | `detect.rs:47-50 ↔ 778-781` | 0.90 | mixed | production sort vs `reference_detect`'s. The differential test's whole value is that it is an **independent** reimplementation; factoring it out destroys the test | **No** |
| 6 | `detect.rs:232-244 ↔ report.rs:144-153` | 0.87 | both-test | two `#[cfg(test)]` `Fragment{…}` builders, same field set. Genuine repetition — but sharing it needs a crate-wide `#[cfg(test)] mod test_support`, for two instances | **Borderline** |
| 7 | `detect.rs:388-420 ↔ 510-538` | 0.90 | both-test | the **same 20-line `sum_positive`/`add_upbeat` fixture string, copy-pasted**. Hoist to a `const` | **Yes** |
| 8 | `detect.rs:488-507 ↔ parse.rs:867-899` | 0.86 | both-test | "parse a source, map `kind`s, `assert_eq!` the vec" — unrelated intents, shared arrange/act/assert shape | **No** |
| 9 | `discovery.rs:232-239 ↔ 251-258` | 0.94 | both-test | `explicit_file_is_included_even_if_gitignored` vs `duplicate_inputs_are_de_duplicated` — unrelated tests, same fixture/discover/assert shape | **No** |
| 10 | `parse.rs:104-116 ↔ 167-183` | 0.89 | both-non-test | `visit_item_impl` vs `visit_expr_block`: "call `push`, then recurse" is the `Visit` trait's shape (the same reading as §7's two dropped `parse.rs` pairs) | **No** |

**Result: 1 clearly actionable, 1 borderline, 8 not → precision in `[0.85, 1.00)` is 10%, or 20% if
the borderline counts.** Against 0% in the dropped band `[0.75, 0.85)`.

**Read it honestly — it is neither of the two clean outcomes.**

- It is **not** "0.85 is right". A 10–20% actionable rate is a bad reporter by any standard; the new
  line does not sit above a population of true positives.
- It is **not** the clean "~0 actionable above the line either" that would have promoted **T13's thesis
  — the threshold is the wrong instrument — from estimate to observation**. The surviving band was
  **observationally somewhat better in this sample** than the dropped one (1/10, or 2/10 counting the
  borderline, versus 0/11). That is the whole of the evidence: **ten pairs against eleven, on the
  project's own corpus**. It does **not** establish that the threshold robustly separates, and it says
  nothing about `0.85` *specifically* — any line drawn through this corpus between the two bands would
  produce the same two counts. Per §8's criterion, only a `<50%` hand-label on a **third-party** corpus
  confirms `0.85`, and that measurement is still owed (N78/N84, **T14**). So D5 remains an **estimate
  ratified on reasoning**; N88 neither confirms nor refutes it.
- **What it does support, and this is the sharper finding:** the single clear hit (#7) is *copy-pasted
  test fixture text*, and the borderline one (#6) is a *test builder*. **8 of the 10 survivors involve
  test/harness code** (only #4 and #10 are non-test on both sides), and **8 of the 10 are clearly
  non-actionable shape coincidences**, leaving one actionable and one borderline. The instrument is not
  mainly mis-*calibrated*; it is mainly pointed at a population where "same shape" carries little
  information — which is T13's argument arriving through a different door (the node/line floors and
  A6's erasure), not the threshold's.
- **One caveat against over-reading #7 (→ R8, which is this caveat's permanent home):** the tool scored
  it on the *enclosing test function's* shape,
  not on the pasted text. `parse.rs:431` lowers **every** `Expr::Lit` to one generic `Label::Literal`,
  so the 20 copied lines inside the raw string contribute a **single erased literal node**; the `0.90`
  comes entirely from the enclosing tests' parse/assert/control structure. The duplication a human would
  cite is therefore **not** the evidence the tool used. It is a true positive found for an adjacent
  reason, and so only **weak** evidence that the metric detects what a user would identify.

**Weight: identical to §7 — our own corpus, so corroborative only.** N78/N84 are not discharged, and
per §8 a `<50%` result on a *third-party* corpus is what would confirm `0.85`. **T14** owns that.

#### 7c. Node-count addendum to §7b (N100) — the free measurement, and it does not go T13's way

`left_nodes`/`right_nodes` were already in the JSON of the **same** run, so §7b's table is re-emitted
below with a node-count column at zero extra labelling cost. **Pinned run — `(corpus, sha, full flag
set)` per N90.** Corpus `dry4rust/src` at **`6c3d7e6`** (working tree carries only D5's doc-comment and
default-literal edits, neither of which moves a fragment):

```
cargo build --release
target/release/dry4rust src --threshold 0.85 --format json
```

`min_nodes` gates each fragment **independently** (`detect.rs:179`, `passes_floors`), so the column that
decides whether a floor keeps a pair is **`min(left,right)`**.

| # | pair | score | `left_nodes`/`right_nodes` | min | actionable (§7b) |
|---|---|---|---|---|---|
| 1 | `dedup.rs:180-183 ↔ detect.rs:47-50` | 0.90 | 20 / 20 | **20** | No |
| 2 | `dedup.rs:238-247 ↔ 253-261` | 0.97 | 40 / 39 | **39** | No |
| 3 | `dedup.rs:238-247 ↔ 431-440` | 0.97 | 40 / 39 | **39** | No |
| 4 | `detect.rs:47-50 ↔ 100-103` | 0.90 | 20 / 20 | **20** | No |
| 5 | `detect.rs:47-50 ↔ 778-781` | 0.90 | 20 / 20 | **20** | No |
| 6 | `detect.rs:232-244 ↔ report.rs:144-153` | 0.87 | 23 / 20 | **20** | Borderline |
| 7 | `detect.rs:388-420 ↔ 510-538` | 0.90 | 29 / 26 | **26** | **Yes** |
| 8 | `detect.rs:488-507 ↔ parse.rs:867-899` | 0.86 | 28 / 24 | **24** | No |
| 9 | `discovery.rs:232-239 ↔ 251-258` | 0.94 | 31 / 33 | **31** | No |
| 10 | `parse.rs:104-116 ↔ 167-183` | 0.89 | 26 / 27 | **26** | No |

**The result is the second branch N100 named, not the first — report it as found.** The eight shape
coincidences do **not** sit in a low `20–35` band with the true positive above them. They span
**20–39**, and the sole actionable finding (#7) sits at **26 — inside the mass, below its median**,
fifth-smallest of the ten. Projected against the estimate:

| floor | survivors (of the 10) | actionable among them |
|---|---|---|
| `20` (today) | all 10 | 1 (+1 borderline) |
| `30` | #2 (39), #3 (39), #9 (31) | **0** |
| `35` | #2 (39), #3 (39) | **0** |
| `40` | none | — |

**A `30–40` floor removes the band's only actionable finding first and takes precision from 10% to
0%.** On this sample it separates nothing: it is a size cut that correlates with neither population.
That **contests** the `30–40` estimate — it does not establish that a floor in that range is generally
wrong, and a categorical rejection would overstate ten pairs on one self-corpus. The cost is also
qualified: #7 is actionable **as a location pair** but **weakly attributed** — the copied text is a
single erased literal node and the `0.90` comes from the enclosing test structure (§7b's caveat, → R8).
So the floor's price here is **one weakly-attributed true positive**, not a strong one.

**And it does not clear the class T13 aims at either.** The seven exact-`1.00` findings from the same
run, by min node count: **20, 20, 20, 28, 38, 39, 41**. At `30` and at `35`, **3 of 7 survive**; at
`40`, **1 still does**. The floor is `>=` (`node_count >= min_nodes`, `detect.rs:179`), so the `41/41`
pair survives at `41` and the class does not empty until **`42`**. The floor is weakest exactly where
T13 says the dominant false-positive mass lives.

**One caveat, and one former caveat now discharged by measurement.** (a) **DISCHARGED — the projection
was checked against real runs, and the survivor sets are exact for this pinned run, not approximate.**
The projection filters the *emitted* rows, so a pair currently suppressed by dedup could in principle
resurface once its dominator is floored out. It does not, here. Same pinned corpus and sha, same
`(corpus, sha, full flag set)` discipline per N90:

```
target/release/dry4rust src --threshold 0.85 --min-nodes 30 --format json   # 6 findings
target/release/dry4rust src --threshold 0.85 --min-nodes 35 --format json   # 5 findings
target/release/dry4rust src --threshold 0.85 --min-nodes 40 --format json   # 1 finding
target/release/dry4rust src --threshold 0.85 --min-nodes 41 --format json   # 1 finding
target/release/dry4rust src --threshold 0.85 --min-nodes 42 --format json   # 0 findings
```

At `30`: #2, #3, #9 **plus** the three surviving exact-`1.00` pairs. At `35`: #2, #3 plus the same
three. At `40`: the exact-`1.00` `41/41` pair alone (`parse.rs:904-937 ↔ 1066-1089`), which survives
`41` as well and dies only at `42`. **No suppressed pair resurfaced at any floor.** (These are flag
overrides for measurement only; the shipped `--min-nodes` default is unchanged at **20**.) (b) **Same
self-corpus weight as the rest of §7b — an addendum, not a promotion:** ten pairs on the project's own,
test-heavy corpus. It cannot settle `30–40`. What it does do is remove `30–40`'s status as
*unopposed*: the one cheap check available now points the other way, so **T13 must either move the
estimate or explain this sample away**, and T14's data is what will decide. Not equivocal, but small.

#### 8. What is still owed (N78/N84 stand) — the cheapest decisive test

Recorded so it is not lost:

- **Hand-label.** Sample **20** reported pairs from the band **[0.75, 0.85)** on **one real medium
  third-party crate** and hand-label "would I factor these out?". **<50% precision confirms `0.85`;
  >75% would refute it.** **The 50–75% interval is not undefined (N87b): it lands on `0.80`** — which
  is also §4's suppression-trigger value, i.e. the same number arrives from two independent directions.
  Read `50–75%` as "the band carries real signal but not enough to pay for itself at `0.75`".
- **Zero-labelling complement.** Run across **two unrelated crates** and count cross-crate hits per
  KLOC at **≥0.75**, **≥0.85** and **=1.00**. Every cross-crate hit is non-actionable by construction,
  so that histogram reads the coincidence null directly, with no human judgement in the loop.

Both remain **open**; D5 is decided, not measured.

**N102 — this section's protocol is superseded by T14's, which is stricter.** The single-threshold
sample and the lone `<50%` criterion above are retained as history; T14 ships the replacement — a
per-bucket sample (`[0.75,0.85)`, `[0.85,0.95)`, `[0.95,1.00)`, `=1.00`), a **pre-registered** decision
rule stated before any labelling, and a third `attributed` label column (R8). Read T14's row, not this
list, when the labelling is actually run.

#### 9. Field comparison, one line

Mature clone detectors (CPD, Simian, jscpd, CCFinder, `dupl`) mostly default to *exact-after-
normalization plus a generous size floor* — they spend their precision budget on the **floor**, not on
a fuzzy band. **Do not cite dry4go's default as evidence:** if it follows `dupl`'s lineage its
"threshold" is a token **size**, not a similarity ratio, and is not commensurable with ours. A UX model
is not a metric model (this is D1/O1 restated).

**Where `0.75` actually came from (N87c) — it was never derived.** No note, review or measurement in
this file establishes `0.75`; it entered with the dry4go UX skin at D1/O1 and was carried forward as a
plausible-looking number. The paragraph above shows it is not even commensurable with its source: if
dry4go inherits `dupl`'s lineage, its "threshold" is a token **size**, not a similarity ratio. So the
honest framing of this whole decision is: **we are moving off a placeholder, not off a reasoned value.**
That cuts both ways and both must be recorded — it lowers the bar `0.85` had to clear (there was no
prior evidence to overturn), and it means the −39% delta in §6 measures a change *from an arbitrary
baseline*, so it quantifies nothing about `0.85`'s own correctness.

#### 10. `min_nodes` — recorded, deliberately NOT acted on (→ **T13**)

The human's call: `min_nodes` is deferred to its own task. The argument to preserve:

The **dominant** false-positive class — two unrelated builder setters, two `impl Display` bodies, two
arrange/act/assert tests — sits at score **`1.00`**, where **no threshold can reach it**. *Dominance is
attributed to its evidence, at the same standing as the `30–40` estimate:* **7 of the 17 findings at
`0.85` are exact-`1.00`, on our own corpus at `6c3d7e6`** — an estimate, not a third-party measurement.
The unreachability itself is **exact**, not an estimate: the gate is `>=`, so no threshold in `[0, 1]`,
**including `1.0`**, excludes a δ=0 pair. Moving `0.75 → 0.85` removes **none** of those (§7 shows two
live examples in our own output; §7b adds that 8 of the 10 *survivors* are the same shape-coincidence
class one band lower).

**N94 — the class exists because A6 erases, so there are three levers, not one.** Identifiers, literals
and types are free under A6; that is what makes two unrelated setters δ=0. Therefore:

- **(i) raise `min_nodes`** — cheapest, purely a parameter, no output-semantics change. **Chosen.**
- **(ii) raise `min_lines`** — available, but weaker: line count is formatting-sensitive where node
  count is not, so it buys the same suppression less predictably.
- **(iii) partially de-erase A6** — the only lever that attacks the *cause*. **REJECTED on the record:**
  it changes the meaning of `score`, which is a breaking output change under **R3**, and it re-opens the
  A2/A6 label-model commitment that R3 exists to freeze (see §4's second revisit trigger — a de-erasure
  voids `0.85` outright rather than shifting it).

So the honest statement is **"the floor is the cheapest of three levers, and here is why we won't touch
the other two"** — *not* "the only cure is the node floor", and *not* "tune a number". The estimate is
that `min_nodes` should be **30–40** rather than `20`.

**N72 reframed:** closures dying 88% at the floor is *not* evidence the floor is brutal. It is evidence
that closures sit **below the information threshold**, where "same shape" means anything. Read that way
it argues for raising the floor, not lowering it.

**N95 — the consequence nobody had drawn: T13 may close a decision, not just move a number.** At
`min_nodes` 30–40 the closure population — already **88% annihilated at 20** — goes to ~zero, and free
`{}` blocks are already **0 extracted** (N78). EXTENDED's two weakest granularities then carry cost and
produce nothing, so **D7 stops being a de-scope lever and becomes a cleanup**, and **A1 may need
amending** to drop them. Sequence T13 accordingly: it is a task that can **close D7 and amend A1**.

Tracked as **T13 (Pending)**; `min_nodes` is unchanged at `20`.

**Ledger:** N78/N84(a,b,d,e) **stand — still owed**; N84(c) → T12; N86(a) remains open (unrelated to
the threshold). New: **T13**, **T14**.

#### 11. Review repairs landed on this record (N87–N98)

Anders' D5 review was **APPROVE-WITH-NOTES**; the notes were applied here, not deferred.

- **N87** — four one-line repairs: (a) §7 no longer says the self-corpus hand-label *confirms* `0.85`;
  (b) §8's `50–75%` interval now resolves, to `0.80`; (c) §9 states `0.75`'s provenance — it was never
  derived; (d) §4 records the **second** revisit trigger (the label model).
- **N88** — the 10 survivors in `[0.85, 1.00)` were hand-labelled; §7b, with the result reported as
  found (1 clear + 1 borderline of 10) rather than as either clean narrative.
- **N89** — §6 reconciles N79's 13-at-`5e2dc58` with §6's 28-at-`6c3d7e6` (corpus growth, not a dedup
  regression) and bounds the −39% headline.
- **N90** — the pin is now `(corpus, sha, **full flag set**)`, standing rule; N79's recipe
  back-annotated with `--threshold 0.75`.
- **N91** — three stale but *live* notes corrected in place, not rewritten: N29 (`0.85/4/20`),
  `tests/facade.rs`'s `impl`↔method comment (now states the bound `1 − 1/(n+1) ≥ 0.95238` at `n ≥ 20`,
  so it cannot go stale again), and T7's dogfood line (stamped VOID inline).
- **N92** — N78(iii) stamped **consciously overridden by ratification, evidence still owed**.
- **N94/N95** — T13's framing corrected (three levers, (iii) rejected on the record; dominance
  attributed to its evidence) and its D7/A1 consequence recorded.
- **N96** — T12 must report its curve as a function of **F** with `min_nodes` a stated input.
- **N97** — **T14** added: one corpus artifact, four blocked consumers.
- **N98** — façade gap **accepted and relabelled**, not closed. The façade must **not** track the
  product default: that recreates the second source N29 forbids and couples lib tests to `clap`. The
  stronger framing: **no test anywhere is sensitive to the default's *value*** — the default-path runs
  all score `1.00` and pass at any threshold in `[0, 1)` — and that is **correct by design**, because
  the default is a **calibration constant, not a behavior**. Binding it to fixture behavior would turn
  every recalibration into a golden-churn event. Zero goldens moved at D5 for that reason; it was the
  right outcome, not a hole. Landed as the only code touch: `tests/facade.rs`'s literal is hoisted to
  `HARNESS_THRESHOLD`, documented as deliberately *not* the shipped default and chosen low so fixtures
  exercise the **gate**, not the **calibration**.
- **N93 — deliberately NOT done here.** The size-ratio identity (§2) stays in this record; promoting it
  into `similarity.rs` / `design.md` is **deferred to T12's doc pass**. `design.md` stays number-free.
- **N99 — the evidence-attribution property is a *risk*, not a note: R8 added.** A6's erasure plus
  unparsed macro bodies (`tree.rs:19`, `tree.rs:31-34`; `parse.rs:431` — one `Label::Literal` per literal;
  `macro_label`, `parse.rs:583` — one leaf per macro invocation, body unparsed; all four cited lines
  verified against the source) mean the score can describe something other than the duplication a human
  would cite. It has a user-visible consequence in **both** directions and a mitigation, which is what
  makes it a standing risk. **A6 keeps its wording** with `(consequence: R8)` appended. §7b's
  adjacent-reason caveat is cross-referenced to R8 and **left in place**. R8's mitigation is the **only**
  `docs/design.md` touch in D5: a number-free, threshold-free limitation in the score/report description,
  stating that T13's floors do not address this and that N94 (iii) is rejected under R3.
- **N100 — §7b's table re-emitted with a node-count column (§7c), from the JSON of the same pinned run.**
  Zero new labelling; `(corpus, sha, full flag set)` pinned per N90. **It came out the way T13 would not
  want and is reported as found:** the eight shape coincidences span **20–39** min-nodes rather than
  clustering low, and the sole actionable finding sits at **26**, *inside* them — so a `30–40` floor
  removes the true positive first (10% → **0%** actionable) and still leaves **3 of the 7** exact-`1.00`
  findings at `30`/`35` (**1** at `40`; the gate is `>=`, so the class empties only at **42**). The
  removed finding is #7 — actionable as a *location pair*, **weakly attributed** (→ R8). The projected
  survivor sets were then confirmed against real `--min-nodes` runs on the same pinned corpus (no
  dedup-suppressed pair resurfaced), so they are **exact for that run**. Self-corpus, N=10: an addendum,
  not a promotion — it **contests** the `30–40` estimate rather than refuting it, but `30–40` is no
  longer unopposed.
- **N101 — T13's rationale broadened, plus one observation that decides nothing.** The false-positive
  class is unreachable at `1.00` **and dominant just below it** (8 of 10 survivors, §7b) — same lever,
  wider evidence, unchanged standing. Observation feeding T13 **and** T14: the dominant noise carrier
  here is **test/harness code**, not fragment size, and test functions are node-rich (§7c puts them at
  31–39), so T13's floor plausibly misses the very population N88 found. Options (a) nothing /
  (b) a documented "point it at `src/`" recipe / (c) `--exclude` or a `#[cfg(test)]` skip — recorded,
  **none chosen**; (c) is flagged new surface and YAGNI-suspicious in v1.
- **N102 — T14's labelling protocol tightened, inside its existing deliverable.** Sample **per score
  bucket** (`[0.75,0.85)`, `[0.85,0.95)`, `[0.95,1.00)`, `=1.00`) and report precision per bucket — only
  a curve can *locate* a line, and the empty observed `(0.81, 0.86)` gap left the single-threshold label
  with no resolution; T14 also emits the full score histogram to test whether that gap is a small-N
  artifact. The **`=1.00` bucket is mandatory** and is the record's actual hole: T13's central claim
  rests on **7 findings nobody has ever hand-labelled**. The decision rule is **pre-registered before
  labelling**, replacing §8's `<50%` criterion with adjacent-bucket contrast against §4's cost
  asymmetry — writing it afterwards is **fitting** and is named as such. A third label column,
  **`attributed`** (R8), yields precision two ways; on §7b's data the strict figure is **0 of 10**, and
  that is the honest headline. The zero-labelling protocol is **kept unchanged** — attribution is
  undefined there — gaining only a **node-count axis** on the cross-crate histogram, which prices T13's
  floor against the coincidence null mechanically. §8 now points here.

**Sequencing (Anders, recorded):** **D5 (with these repairs) → T12 (N59/N70/N75/N83, N84(c),
parameterized per N96, plus N93 and N86(a) in its doc pass) → T14 → D5-confirm + T13 together →
S3 close.** T14 sits before the two evidence-bearing items because it is what makes them dischargeable.
