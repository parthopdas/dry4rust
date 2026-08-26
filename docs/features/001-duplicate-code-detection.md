# Feature: Duplicate Code Detection (TED core, dry4go UX skin)
**Branch:** vibe/001-duplicate-code-detection
**Status:** WIP — **S1 DONE**; T11/T8/T8b/T8c/T9/T10 landed, **S2 DONE**. **D5 decided (threshold `0.85`)**, review repairs N87–N98 landed. T12 and T14 measured and reviewed (T14 review notes, N112–N123). Next: **three human decisions — D10** (v1 perf position: document the limit vs run S4 = T15–T18) → **D11** (`min_nodes` default; T13′ executes it) → **D12** (`--exclude <glob>` in v1?)

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
| S4 | **Admissible optimisation (T15–T18)** — measure where the 521 365 TED evaluations go, then apply *only* changes that "change speed, never results" (D8): structural-hash memo, label-multiset lower bound, parallel pair loop. Every task carries T9's negative control (dogfood + acceptance output byte-identical). **Whether S4 runs at all is D10, the human's.** | S3; **D10** |

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
| T12 | S3 | Perf guardrail benchmark on a medium fixture; document complexity envelope. **Integration/bench.** **N96 — parameterize, do not hardcode:** the envelope is driven by fragment count **F** against an **O(F²)** base rate, and `min_nodes` is what sets F. A 20→35 floor moves F, and the O(F²) pair count with it. Report the curve **as a function of F, with `min_nodes` a stated input** — if T12 pins `min_nodes = 20` and T13 later ships 35, T12's numbers are **void**, the exact failure N77(b) and N84(b) have already inflicted twice. Parameterized, a later floor change **re-parameterizes** T12 along F instead of voiding it — **not** a simple rescale; see the T12 record's *N59/N96 — the curve* section for what the two series do and do not show. Also carries N59/N70/N75/N83, **N84(c)** (pre/post-dedup ratio — T12 already counts TED evaluations), and in its doc pass **N93** and **N86(a)**. | Pending | - |
| T13 | S2 | **SPLIT at the T14 review (N118) — this row is now evidence-only and decides nothing.** The *decision* moved to **D11** (`min_nodes` default for v1 — the human's, blocked on D10); the *execution* moved to **T13′**. Everything below is the evidence trail that produced D11 and is retained unchanged; read D11 for the ruling. **`min_nodes` floor calibration (D5 spin-off).** The **dominant** false-positive class — two unrelated builder setters, two `impl Display` bodies, two arrange/act/assert tests — sits at score **1.0**. *Dominance is attributed to its evidence:* **7 of 17 findings at `0.85` are exact-`1.00` on our own corpus at `6c3d7e6`** (same standing as the `30–40` estimate below — an estimate, not a third-party measurement). **N101 — the class is not merely unreachable at `1.00`, it is dominant just below it too:** §7b's hand-label puts **8 of the 10 survivors in `[0.85, 1.00)`** in the same shape-coincidence class, so the lever is unchanged but the evidence is wider than the exact-`1.00` population alone. **Unreachability (exact):** the gate is `>=`, so **no** threshold in `[0, 1]` — **including `1.0`** — can exclude a δ=0 pair. D5's move to `0.85` removes none of them. **N94 — three levers, not one:** the class exists because **A6 erases** identifiers, literals and types, so (i) raise `min_nodes`, (ii) raise `min_lines`, (iii) partially de-erase A6. **(iii) is REJECTED on the record:** it changes the meaning of `score` and is a breaking output change under **R3**, and it re-opens the A2/A6 label-model commitment — the very thing R3 freezes. (ii) is weaker than (i) because line count is formatting-sensitive where node count is not. **(i) is the cheapest of three, not the only cure.** Estimate: `min_nodes` should be **30–40**, not 20. **Reframes N72:** closures dying 88% at the floor is not evidence the floor is brutal, it is evidence closures sit below the information threshold where "same shape" means anything. **N95 — this row may close a decision, not just move a number:** at `30–40` the closure population (already **88% annihilated at 20**) goes to ~zero and free `{}` blocks are already **0 extracted**, so EXTENDED's two weakest granularities become dead weight — **D7 stops being a de-scope lever and becomes a cleanup**, and **A1 may need amending**. Sequence T13 knowing it can **close D7 and amend A1**. **N100 — the estimate now has one measurement against it:** on §7b's own ten survivors the `min(left,right)` node counts are `20,39,39,20,20,20,26,24,31,26` (§7c); the eight coincidences span **20–39** and the sole actionable finding sits at **26, inside them**, so a `30–40` floor takes the band from 10% actionable to **0%** and still leaves **3 of the 7 exact-`1.00` findings** alive at `30` and `35` (**1** even at `40`; the gate is `>=`, so the class does not empty until **42**). The cost is qualified: the finding it removes is #7, actionable **as a location pair** but **weakly attributed** (→ R8). §7c's projected survivor sets were confirmed against real `--min-nodes` runs on the pinned corpus, so they are exact for that run. Self-corpus, N=10 — this **contests** `30–40` rather than refuting it (a categorical rejection would overstate the sample), but T13 must move the estimate or explain the sample away. **N101 — observation feeding T13 and T14, deciding nothing now:** the dominant noise carrier in this sample is not fragment *size* but **test/harness code** — 8 of §7b's 10 survivors and most of §7's 11 drops. Test functions are node-rich (§7c: the test pairs sit at 31–39, above the estimated floor), so a 35-node floor plausibly does **not** kill a 10-line arrange/act/assert test and T13's lever may miss the population N88 found. Three options, **none chosen**: (a) nothing — users pass paths; (b) a documented "point it at `src/`, not `tests/`" recipe; (c) a `--exclude` glob or `#[cfg(test)]` skip — **new surface, YAGNI-suspicious in v1** (**superseded by D12 (N117): the YAGNI objection is withdrawn on measured evidence — `--exclude <glob>` is proposed IN, `#[cfg(test)]` skipping stays OUT**). §7c's column is what tells us whether the floor covers this population at all. Same evidence burden as D5 (N78/N84): needs a third-party corpus → blocks on **T14**. **T14 delivered it (record §5d/§6/§1); the ruling is D11's.** | Split (N118) → **D11** + **T13′** | - |
| T13′ | S2 | **Execute D11's ruling on the `min_nodes` default.** No evidence-gathering: D11 is decided by the human on T14's record, and T13′ only lands it. **Carries N95's consequences conditionally, and the conditionality is the point.** *If D11 raises the floor to ≥30:* the closure population — already **88% annihilated at 20** (N72) — goes to ~zero and free `{}` blocks are already **0 extracted**, so EXTENDED's two weakest granularities become dead weight; **D7 stops being a de-scope lever and becomes a cleanup**, and **A1 may need amending**. *If D11 holds at 20 (Anders' recommendation):* **T13′ is a no-op and D7/A1 are untouched** — no cleanup, no amendment, nothing to write. Whichever way it goes, T13′ ships the number and its consequences in one change, with T9's negative control on the dogfood output. | TODO — blocked on **D11** | - |
| T14 | S3 | **Acquire and pin a third-party corpus harness.** **One artifact, four consumers:** D5-confirmation, **N78**, **N84(a,d,e)** and **T13** all block on it, and it has been deferred at every gate so far. Deliverable: a named crate + **pinned `(corpus, sha, full flag set)`** per N90, plus §8's **hand-label** and **zero-labelling** protocols written down as runnable recipes (sample size, band, the "would I factor these out?" rubric, the per-KLOC cross-crate histogram at `≥0.75` / `≥0.85` / `=1.00`). **N102 — three protocol edits inside that deliverable, and they change what the labelling can conclude.** (1) **Sample across scores, not at one threshold:** N per bucket in `[0.75,0.85)`, `[0.85,0.95)`, `[0.95,1.00)` and **`=1.00`**, reporting precision **per bucket**. A precision-vs-score curve is the only thing that can *locate* a line; a single number can only be *consistent with* one — and the observed `(0.81, 0.86)` gap is **empty**, so the single-threshold label had no resolution to give. T14 must therefore also emit the **full score histogram** of the un-sampled run, to test whether that gap is a small-N artifact. (2) **The `=1.00` bucket is mandatory and is the actual hole in the record:** T13's central claim rests on **7 findings nobody has ever hand-labelled** — §7 labelled the 11 dropped, §7b the 10 survivors, and the seven exact-`1.00` were never read. (3) **Pre-register the decision rule before labelling,** replacing §8's single `<50%` criterion with a statement about the *contrast between adjacent buckets* and about where precision crosses §4's cost asymmetry. Writing that rule after seeing the curve is **fitting**, and must be called that. (4) **Third label column — `attributed`:** does the tool's evidence match the human's reason (**R8**)? Report precision two ways, *actionable* and *actionable-and-attributed*. On §7b's own data the strict number is **0 of 10**, and that is the honest headline: without this column a detector that finds the right pairs for the wrong reasons is indistinguishable from one that works. (5) **Zero-labelling protocol: keep as written, and do not add the third column** — attribution is undefined when every hit is non-actionable by construction. Add one axis instead: bucket the cross-crate histogram **by node count as well as score**, which prices T13's floor against the coincidence null mechanically, with no human in the loop. Without T14, S3 closes with four "still owed" items and **no mechanism that will ever discharge them**. **N107 — three acceptance criteria, stated as criteria and not aspirations; T14 is not done until each has an answer on the record.** (1) **F per kLOC on real code, as a function of `min_nodes`** — the gap nobody had named: the envelope is parameterized in **F** (T12) while every user has **LOC**, so without the F/kLOC constant `Θ(F²)` is unusable as a user-facing statement and we cannot answer *"how long on my 80 kLOC crate?"*. Our single data point (134 fragments for `src`, *T12 record, N84(c)*) is an **anecdote, not a measurement**. **OUTCOME AT T14 — there is no constant** (§1: 10.56 / 27.05 / 43.38 F/kLOC, a 4.11× spread that `Θ(F²)` squares into ~17×), which is now **A10** (N114) and is what `docs/design.md`'s Performance section states. (2) **End-to-end wall-clock at a named scale target against a stated budget.** **The budget number is a product decision reserved to the human. It was recorded here as OPEN and was not invented; it is now **SET at 10 s** by the human (T14 record §0.1 — provenance, and the 30 s / 60 s alternatives he rejected).** Proposed *form* (Anders): one crate in the class of `syn` / `regex` / `ripgrep`, **default flags**, **best-of-3**, developer machine. The reason it is a criterion at all: **without a stated budget no measurement can ever be a pass or a fail**, and R1 stays open by construction. (3) **The joint `(node count, depth)` distribution of *admitted* fragments** — one histogram, which also discharges **D9**'s trigger. **N108 — corpus selection is pre-registered in this row, before any crate is picked.** Same reason as N102(3): choosing a corpus *after* seeing which one flatters dedup is **fitting — the same error wearing a different hat**. The criterion is **coverage of the architecture's cost-bearing shapes**, declared up front, with results reported whatever they turn out to be: **≥1 crate with many small `impl` blocks** (the close-sized nested wrapper case — N83's "tight where it hurts"); **≥1 trait-heavy crate**, which is also the first real chance at **A1**/N86(a)'s revisit trigger, unreachable on our trait-poor corpus; **≥1 crate containing generated code**, the only realistic source of the depth N70 needed a synthetic adversary to produce — it feeds N107(3) and D9's trigger. **Context (recorded, not a task):** N78's zero cross-granularity survivors, N79's bit-identical negative control and N84(c)'s 17-pre = 17-post are three independent observations with **one cause — our corpus contains no nested clone site**. Dedup does nothing because there is nothing to do, and its cost when idle is zero. **This is not a signal about dedup and T9 is not reopened:** removing it would produce the four-findings-per-clone-site output that is self-evidently wrong, and T10 plus the `d = 3` 5:1 fixture both witness the mechanism. The sharp consequence is about *evidence*, not design: **the pipeline's output had never been observed on the class of code where the architecture pays its `d²` cost** — every witness up to T12 was one we constructed. **Observed at T14** on three pinned third-party crates (record §4): dedup removes 37–66% of raw candidates there. **Counter-outcome, pre-written:** if pre/post-dedup comes back **1.00 across several real crates too**, that is a **genuine finding and goes on record** — it would say dedup's value is concentrated at nested and generated sites. That is a post-v1 revisit note, **still not a removal**. **OUTCOME AT T14 — the counter-outcome did NOT occur:** pre/post-dedup on third-party code is **1.97× / 1.58× / 2.90×** (T14 record §4), so **N84(c) closes affirmatively**, `17-pre = 17-post` is confirmed as a property of *our own fixture corpus* only, and the post-v1 revisit note is **not** created. **OUTCOME — D5-confirm is PARTIAL, and the record says so in its headline (T14 record §5c):** the pre-registered rule **fired** for `0.85`, but on `n = 8` per bucket from one crate labelled by one non-maintainer AI, a contrast **two pairs** wide that clears the 20 pp bar by only **5.0 pp** and that **one relabel (12.5 pp) would unfire** — row 7 `No → Yes` in `[0.75,0.85)`, or any one of rows 9/10/11/16 `Yes → Borderline/No` in `[0.85,0.95)`. **Row 7 is a † (deliberated) row tallied `No` and is individually decisive**; the other four † rows do not steer the rule (record §5c). *Rule fired*, **not** *threshold shown to separate*. The mechanical zero-labelling null (0 cross-crate hits per kLOC at `≥0.85`) is the stronger leg. **R2 stands.** | Done | dd81a61 |
| T15 | S4 | **Measurement only — no pipeline change, no result change.** On the same pins as T14 (`../_t14-corpus`, same shas, default flags), instrument the pair loop and count three things. **(a) How many *distinct* `(structural-hash, structural-hash)` pairs** are among `syn`'s **521 365** TED evaluations. If that number is ~50k, **memoisation alone is the 10×** and T16 is the whole answer. **(b) How many pairs a label-multiset lower bound would prune** — the yield of T17, before writing T17. **(c) What fraction of total TED time is spent on pairs touching D9's deep-and-large quadrant** (the 9 `syn` fragments at ≥100 nodes and depth ≥20, up to 36 pairs among themselves). **(c) is D9's missing number** — occupancy is not cost (N119) — and it costs no extra task because it is the same instrumentation. Output is a printed report, exactly like `tests/corpus.rs`; the shipped pipeline is untouched, so there is nothing for T9's negative control to catch. | TODO — blocked on **D10** | - |
| T16 | S4 | **Structural-hash memo.** Cache TED results by `(hash_left, hash_right)` over the normalized trees' structural hashes, collapsing each equivalence-class pair to **one** computation. Pure, **std-only `HashMap`**, no new dependency; **determinism untouched — a cache never orders output**. **Gated on T15(a)** (if the distinct-pair count is not far below 521 365 there is no win to take). **Negative control (T9's):** dogfood **and** the T14 acceptance outputs must be **byte-identical** before and after. | TODO — blocked on **D10**, gated on **T15** | - |
| T17 | S4 | **Label-multiset lower bound.** Prune a pair pre-TED when the symmetric difference of the two trees' label multisets already exceeds the δ the threshold allows. **Same standing as T11** — provably never prunes a real match, and it ships **the same prune-soundness unit test**. **Unknown yield**, and that is why it is measured first: the size-ratio filter already killed 79% of `syn`'s pairs and the survivors genuinely are similar. **Gated on T15(b).** **Negative control (T9's): byte-identical output.** | TODO — blocked on **D10**, gated on **T15** | - |
| T18 | S4 | **`rayon` over the pair loop.** Parallelise the `Θ(F²)` loop, collect, then sort by the existing canonical key so output order is unchanged. **Deliberately last of the three**, and the ordering is a position, not an accident: it is the one lever that **always works** (6–12× on a developer box) but also the only one that **touches architecture** — a third-party crate at/near core, against A5's "core is pure std-only" — and a parallel 10× would **mask whether the algorithmic work paid**. **Negative control (T9's): byte-identical output**, which for this task is also the determinism test (A7/R6). | TODO — blocked on **D10**, gated on **T15** | - |

## Risks (Rx)

- **R1 (perf):** O(n²) pairs × super-quadratic TED (APTED ~O(n³) worst) → slow on large repos. Mitigate:
  `min-nodes` floor, size-ratio pre-filter (S3), size bucketing; keep TED confined so it's swappable.
  **Status after the T14 review (N112): CLOSED as a risk.** The measured envelope is no longer an
  uncertainty — it is a **documented limit, i.e. a specification**. `Θ(F²)` was reproduced, F/kLOC was
  measured on three ordinary crates, the budget was stated by the human, and the breach was measured
  **twice** in two independent sessions. Everything a risk register can ask of R1 has been answered;
  what remains is a known, quoted operating envelope, not a thing that might turn out badly.
  **Stated once, at T14 record §2** (the 10 s budget, the 103.878 s / 66.618 s scale-target FAIL and the
  exclusion measurement) **and at A10** (the F/kLOC bridge and why any user-facing runtime statement is
  a range); neither is restated here. What to *do* about that limit in v1 is a product decision, not a
  risk — see **D10**. **The tail is not R1's** — a single pair's shape-driven cost lives in **R9**.
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
- **R10 (non-actionable by construction, N113):** on a generated-code-heavy target the tool's output is
  dominated by findings **no user can ever act on**. Measured on the scale target (T14 record §4b):
  **90.7%** of `syn`'s 6 616 findings and **93.7%** of its 3 432 exact-`1.00` findings are
  **generated↔generated**. The duplication is real and the score is correct — **the generator wrote it**,
  and no local edit removes it. This class is **unreachable by threshold, by floor, and by the label
  model**: it is not a calibration error, so **calibration can never fix a finding that is non-actionable
  by construction**. It sits **beside R8**, at the same standing and for the same reason — R8 is *right
  pair, wrong evidence*; R10 is *right pair, right evidence, nothing a user can do*. **Mitigate:** path
  exclusion — **D12**'s `--exclude <glob>` — plus a documented recipe for pointing the tool at
  hand-written code. Floors and thresholds are **not** mitigations for it and must not be quoted as such.
- **R9 (shape-dominated per-pair cost, N103):** per-pair TED cost is driven by tree **shape** —
  `min(depth, leaves)` on each side — as well as by node count, so two pairs with the *same* node counts
  can differ enormously in price: **≈39× conservatively, up to ~95× observed** (measured at the ceiling
  in the *T12 measurement record*, **N70**; the two figures and the reason the conservative one is the
  quotable one are stated there and are not restated here). Like **R8**, this is a **standing property of
  the engine, not a defect to be tuned away**: it follows from Zhang–Shasha itself, so no calibration
  removes it. The existing mitigation — the **`max_nodes` ceiling** — bounds only the **node** axis, and
  is therefore an **imprecise instrument for this risk**: it prices what it can see, not what actually
  costs. Naming that imprecision *is* the mitigation's honest content, exactly as in R8. **Mitigate:
  document it** — `docs/design.md`'s Performance section and the `MAX_NODES` doc comment in `cli.rs`
  (both number-free of thresholds). A **shape-aware ceiling is the correct instrument and is deferred to
  D9**. The measurement owed against it is **T14**'s `(node count, depth)` histogram of admitted
  fragments (N107(3)). **Priced against the budget at the T14 review (N112): the tail is R9's, not
  R1's.** One **1 043-node nested fragment** cost **7.614 s – 17.478 s for a single TED evaluation on a
  single pair** across two sessions (T14 record §3/§8) — i.e. that one pair **straddles the entire 10 s
  budget**, under it in one session and 1.7× over it in the other. So R9's ≈39×/~95× ratio is no longer
  only a ratio: at the sizes the ceiling admits, it is budget-scale in absolute terms.

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
  **Count asymmetry and revisit trigger (N86(a), recorded at T12; counts corrected at T12 review).**
  What holds **unconditionally** is the asymmetry itself: a duplicated `trait` has **no** wrapper pair,
  so nothing can dominate its method findings under A3 clause 2, while a duplicated `impl` has one, so
  its corresponding-method echoes are dominated and collapse. The *exact* counts hold only under two
  stated assumptions — (i) the `k` methods are structurally distinct from one another, and (ii) only
  the `k` corresponding cross-file method pairs clear the score gate:
  - under (i)+(ii) a duplicated `trait` with `k` default bodies yields **k** findings and a duplicated
    `impl` with `k` methods yields **1** (the maximal `impl↔impl` pair, which dominates the `k`
    corresponding method echoes and the up-to-**2k** wrapper↔method crosses — `k` per copy, not two);
  - **without (i)** the trait count can *exceed* `k`: similar default bodies also produce cross-method
    pairs and same-file (disjoint, non-overlapping) pairs, each an undominated finding of its own;
  - **without (i)** the `impl` count can likewise exceed `1`: a similar disjoint method pair *within a
    single copy* survives, because a cross-file `impl↔impl` pair cannot dominate a same-file pair.
  So the direction is certain and the magnitudes are illustrative, not general. **Revisit trigger:** a trait-heavy corpus in
  which block-level trait clones are demonstrably *missed* — two copied `trait` definitions whose
  duplication a reader would cite at the block, surfacing instead as `k` scattered method findings, or
  not at all when the individual default bodies fall below the floors. Absent that evidence the wrapper
  stays off: manufacturing findings over declaration shape is the larger harm (and would be findings no
  edit can remove). **T14**'s third-party corpus is the first realistic chance to observe the trigger;
  our own corpus is trait-poor and cannot.
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
- **A10 (the envelope has no per-LOC constant, N114):** the performance envelope is parameterised in
  fragment count **F**; every user has **LOC**. There is **no constant** bridging them. Measured on three
  ordinary crates at the shipped floors (T14 record §1): **10.56 / 27.05 / 43.38 F/kLOC** — a **4.11×**
  spread, which `Θ(F²)` squares into a **~17×** spread in predicted cost. Two consequences are binding:
  1. **Any user-facing runtime statement must be a range**, never a single number or a single constant.
  2. **What places a crate within that range is generated-code density.** The bottom of the range is a
     hand-written crate whose fragments mostly fall under the floor; the top is a crate that is ~47%
     machine-generated. (`serde_core`, chosen for *many small `impl` blocks*, sits at the bottom — a
     misfit against its own N108 criterion, kept and reported, not swapped.)

  This is why **R1 is a documented limit and not an open risk**, and it is what `docs/design.md`'s
  Performance section now states (number-free of thresholds, LOC-bridged).

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
- **D9:** Shape-aware oversized-fragment ceiling — **deferred; the flat `max_nodes = 2000` ceiling stays
  unchanged in v1.** This is a **ruled decision, not an open question** (the human ruled at the T12
  review; `cli.rs`'s doc comment records it beside the constant). **Re-opened at the T14 review as a
  *decision* blocked on T15(c) (N119) — the v1 ceiling value is unchanged either way; see the
  trigger-status bullets below.**
  - **Why no lower flat value works.** Per-pair cost is `n₁·n₂·min(d,l)₁·min(d,l)₂`, and in the
    adversarial family depth scales *with* n (42 nested `if`s is `d ≈ n/47`), so cost there grows like
    **n⁴, not n²**. Taking the fastest nested measurement (`50.9 s` at 1 983 nodes — *T12 record, N70*)
    and asking for a worst pair around **1 s** implies a ceiling of **≈280** under n² and **≈740** under
    n⁴. Our own largest legitimate fragment is **439 nodes** — a single function (`cli.rs`, re-measured
    at T8). So every defensible lower value lands inside or just above the range of fragments we have
    already seen in ordinary non-pathological code. And it would bite the wrong population: lowering
    silently drops large **flat** fragments, which are precisely the **cheap** ones (`0.74 s` at the
    ceiling — *N70*). Bad trade in both directions, and picking a number from our own tree would repeat
    the **D5/T13** error of setting a user-visible number from self-corpus evidence.
  - **Shape-aware is the *correct* instrument** — it prices the axis that actually costs (**R9**) — but
    **depth was not computed anywhere when this was ruled** (`tree.rs` carried `node_count` and nothing
    else; **T14 added `NormTree::depth` as an unrendered run statistic for N107(3) — measurement only,
    nothing filters on it, and the v1 ceiling is unchanged by it; the "not computed" objection is
    RESOLVED, see the trigger-status bullets and N119**), it would
    add a second invisible, untunable drop rule with its own diagnostic, and **one adversarial fixture
    does not justify it**. **YAGNI in v1.**
  - **Any time- or budget-based cutoff is foreclosed outright**, and is written down here so nobody
    reaches for it later: aborting or skipping a pair on elapsed time would make the candidate set a
    function of machine load, destroying determinism — **A7**, **R6** and the cross-OS-stability golden
    rule all forbid it.
  - **Trigger:** T14's `(node count, depth)` histogram (N107(3)) showing real admitted fragments in the
    **deep-and-large quadrant**, or a user report of a single pair costing minutes.
  - **Trigger status: MET (T14 record §3) — 9 fragments, depth 35, real third-party code.**
    **Ruling at the T14 review (N119): D9 does not enter v1 on this evidence, and does not stay quietly
    deferred either — it RE-OPENS as a decision, blocked on T15(c).** Of D9's three original objections:
    *"depth was not computed"* is **RESOLVED** (`NormTree::depth()` ships as an unrendered statistic, so
    the instrument is now free); *"a second invisible, untunable drop rule with its own diagnostic"*
    **stands, unchanged**; *"one adversarial fixture does not justify it"* is **partially answered** —
    real code does reach the quadrant, but at **9 of 2 245 (0.4%) on one crate and 0 of ~550 across the
    other two**, at depth **35** against the synthetic **85**.
  - **The decisive gap: occupancy is not cost.** The record establishes that the quadrant is *occupied*;
    it does **not** establish that those 9 fragments cost anything measurable. Per-pair cost is
    `n₁·n₂·min(d,l)₁·min(d,l)₂`, and the 9 are mutually size-compatible enough to survive the size-ratio
    pre-filter, so they generate **up to 36 pairs among themselves**. If that handful is ~20 s of the
    104 s, a shape-aware ceiling is simultaneously D9's answer **and** a 20% performance win. If it is
    0.5 s, D9 **stays deferred** and we have learned something more important: the cost is **broad**
    (521 365 ordinary evaluations), reachable only by **memoisation, pruning or parallelism** — S4's
    levers, not a ceiling. That number is **T15(c)**, same instrumentation, no extra task.
  - **Structural point, true regardless of the number: a shape-aware ceiling is NOT admissible under
    D8.** It **drops fragments**, so it changes *results*, not only speed — which puts it under the
    **D5/T13 evidentiary standard**, where a user-visible drop rule may not be set from self-corpus or
    one-crate evidence. We have three crates and **only one occupies the quadrant**. Setting the rule
    from that basis is the exact error D9's own text warns against. So even if T15(c) comes back
    attractive, the number justifies a **targeted optimisation** (S4), **not** a drop rule.
  - **What N70 did and did not establish.** It measured **one point, not a curve**: the nested family was
    timed at the ceiling only, so the **exponent in that family is unknown** — which is why the n²/n⁴
    pair above is a bracket and why **no replacement number is named**. The missing measurement is cheap
    (nested-shape cost at ~500 / ~1 000 / ~2 000 nodes; the generator already takes node count as a
    parameter, so it is minutes of runtime) and is recorded as **optional and non-blocking**, to be
    folded into **T14**'s harness rather than reopening T12. **Run at T14** (record §8, two usable
    points) — it does not revise the ceiling, and the exponent still is not pinned.
- **D10 — the v1 performance position. HUMAN DECISION, open (N115).** R1 is now a documented limit
  (measured, breached twice), so the remaining question is not *what is the cost* but *what do we ship*.
  Two options, as Anders framed them:
  - **(a)** ship v1 with the limit documented — the envelope, the range, and "point it at `src/`";
  - **(b)** take **one admissible optimisation slice** — **S4 = T15–T18** — and re-measure.

  **Anders' recommendation: (b), with the 10 s budget left exactly where the human set it.** His
  reasoning, recorded in substance: **51.8 kLOC is a mid-size library, not a large one.** Extrapolating
  `Θ(F²)` to a 200 kLOC workspace at *middle* fragment density gives ~**2.4×** F ⇒ ~**5.8×** pairs ⇒
  **6–10 minutes**. **That figure is an extrapolation, not a measurement**, and is labelled as one
  wherever it is quoted — but it is the honest reading of the curve we did measure, and it makes
  "documented limit, ship it" read as *unusable on exactly the codebases most in need of a duplicate
  detector*. **D8 already licenses this work**: an admissible optimisation "changes only speed, never
  results", so S4 needs no new architectural permission — only sequencing. This decision **blocks D11**.
- **D11 — the default `min_nodes` for v1. HUMAN DECISION, open, BLOCKED ON D10 (N116).** This is the
  *decision* half of the old T13; the *execution* half is **T13′**. **The evidence is complete** — T14
  §5d (the floor removes 64% of actionable findings and separates only weakly by node count — per
  §6/N118, **weak separation in the desired direction, bought at that 64% actionable-recall loss**;
  the arithmetic is recomputed there, not here), §6 (the
  coincidence mass lives at the floor), §1/§2 (the floor is the cheapest lever on a cost that is over
  budget). **More labelling at n = 8 will not move it**; another eight pairs cannot resolve a
  two-pair-wide contrast. **Anders' recommendation: hold at 20 for v1**, on the principle that **you pay
  a cost problem with cost levers, not with a semantics lever** — `min_nodes` decides *what the tool
  means by a fragment worth comparing*, and raising it to buy runtime spends precision to pay for speed.
  `--min-nodes` is already exposed, so a user on a large crate can make that trade for themselves.
  Blocked on D10 because if S4 lands an admissible speed-up, the cost argument for raising the floor
  disappears entirely.
- **D12 — does `--exclude <glob>` enter v1 scope? HUMAN DECISION, open (N117).** **Anders withdraws the
  YAGNI objection he raised at N101(c)**, and states why: **YAGNI forbids building for a *speculated*
  need; this one is *measured*** (R10 / T14 §4b — 90.7% of the scale target's output is non-actionable by
  construction). His scope split, recorded precisely:
  1. **`--exclude <glob>`: IN.** It lives **entirely in the `discovery` adapter**; the `ignore` crate
     already ships `OverrideBuilder`, so there is **no new dependency, no core change, no score change
     and no output-format change**, and therefore **no R3 exposure**. Determinism holds because the
     match runs over the already-`/`-normalized paths (A7).
  2. **`#[cfg(test)]` skipping: OUT — stays deferred.** Different layer (**parse**, not discovery), and
     **N84(d) has no third-party measurement behind it** (T14 §7 could not deliver it). **Do not bundle
     an unmeasured feature with a measured one.**
  3. **Auto-detecting `@generated` banners: REJECTED for v1.** A heuristic over a *comment convention*,
     ecosystem-dependent, and a **silent drop rule** — the same objection as D9's.

  **The "a workaround already exists" argument is disproved on the record, not merely doubted:** T14
  §2's own mitigation command **lists 48 files on a single command line**. That is evidence the current
  surface is inadequate, not a hypothesis about it. Riders: `--exclude` is a **findings-quality lever,
  not a performance fix** (§2 — removing 47% of the input removed 95.7% of findings and only 58% of the
  time); it ships **with a docs recipe**; and excluding `gen/**` moves `syn` from the **43** end of
  A10's F/kLOC range toward the **~11** end, which makes A10's range *more* useful, not less.

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
  corpora should include one near-ceiling fragment. **[Stamped at T12: measured — see the T12
  measurement record, *N70*. The condition fired, and the human then ruled the ceiling stays 2000 in
  v1; the reasoning and the revisit trigger are **D9**, the shape axis it exposed is **R9**.]**
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
- **N83 (T12/N59 — recorded in `design.md`).** Nesting depth d at a clone site costs **d² TED evaluations** to yield one finding, all paid before dedup — the *report* is clean, the *cost* is not. It cannot move pre-TED: a dominated pair must survive if its dominator fails the gate. Fold the multiplicity into T12's envelope. **Refined at T12 (measured, not derived): `d²` is an *upper* bound — see the T12 record's *N83* section for what it is tight on.**
- **N84 (scopes D5, with N78).** (a) N78 stands **unchanged** — the bit-identical negative control proves the dogfood distribution did not move, so a second, larger, third-party corpus is still required before `0.75` is confirmed. (b) New: the dogfood drifts with our own commits, so every D5 number must be a pinned `(corpus, sha)` pair, as N79's control already was. (c) Report the pre/post-dedup ratio as a corpus statistic. (d) **Partition counts test vs non-test** — arrange/act/assert repetition is idiomatic, and pooling it skews the distribution. (e) Do **not** raise the threshold to silence test clones, and do **not** add an exclude-tests flag (YAGNI, off dry4go parity, hides true positives).
- **N85 (record-only — dogfooding our own tests).** The 1.00 between `dedup.rs:324-351` and `411-451` is a **true positive**; the duplication is deliberate (different pins) and leaving it is right. It is the cleanest available evidence that the tool cannot infer intent — which is exactly why A8 makes it a pure reporter. Cite it in D5 rather than fixing it.
- **N86 (trivial).** (a) **Open, with D5:** N65 should state its revisit trigger (a trait-heavy corpus showing missed block-level clones) and the count asymmetry it implies — a duplicated `trait` with k default bodies yields k findings where a duplicated `impl` yields 1. **Counts corrected at T12 review: only the asymmetry holds unconditionally; the exact counts are assumption-gated — see A1.** (b) **LANDED:** `design.md` → Dedup now reads "(irreflexive via the positional tie-break — no separate guard)". (c) **Open, record only:** `dedup` could take a keep-mask + `into_iter` and clone nothing; inert at these n. (d) **LANDED:** the façade scenario is split into `a_single_method_impl_does_not_pair_with_its_own_method` (N61) and `a_copied_single_method_impl_reports_only_the_maximal_impl_pair` (N51 caveat), fixture hoisted to `SINGLE_METHOD_IMPL`.

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
(**T14 decided it — §5d reproduces N100 on third-party code; the ruling is **D11**'s, N116/N118.**)

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

Tracked as **T13 (Pending)**; `min_nodes` is unchanged at `20`. (**At the T14 review T13 SPLIT: the
decision is **D11**, the execution is **T13′**, and N95's consequences are carried *conditionally* —
if D11 holds at 20, T13′ is a no-op and D7/A1 are untouched. N118.**)

**Ledger:** N78/N84(a,b,d,e) **stand — still owed**; N84(c) → T12; N86(a) remains open (unrelated to
the threshold). New: **T13**, **T14**.

#### 11. Review repairs landed on this record (N87–N108)

Anders' D5 review was **APPROVE-WITH-NOTES**; the notes were applied here, not deferred. **N99–N102**
arrived with T12's landing and **N103–N108** with the **T12 review (APPROVE — commit)**; both sets are
kept here so the note ledger stays in one place.

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
  **none chosen**; (c) is flagged new surface and YAGNI-suspicious in v1. (**Superseded at the T14
  review — D12/N117: Anders withdraws the YAGNI objection on measured evidence; `--exclude <glob>` is
  proposed IN, `#[cfg(test)]` skipping stays OUT, `@generated` banner detection is rejected.**)
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
- **N103 — the shape axis is a *risk*, not a note: R9 added.** Per-pair TED cost is driven by tree
  **shape** as well as node count, so equal-node-count pairs differ by **≈39× conservatively (up to ~95×
  observed)** — the measurement lives in the T12 record (*N70*) and is cited, not restated. Like R8 this
  is a **standing property of the engine**, not a defect to tune away. The existing mitigation, the
  `max_nodes` ceiling, bounds only the **node** axis and is therefore an **imprecise instrument** for it;
  **the mitigation naming its own imprecision is the point**, as it is in R8, and that honesty is what
  makes the row useful later. Mitigation landed: documented in `design.md`'s Performance section and in
  `cli.rs` beside the constant; shape-aware ceiling → **D9**; measurement owed → **T14** (N107(3)).
- **N104 — D9 added: the ceiling decision is closed, and the reasoning is recorded so it stays closed.**
  Flat `max_nodes = 2000` **stays in v1** — a **ruled decision, not an open question**. No lower flat
  value defends itself (the n²/n⁴ bracket, `≈280`/`≈740`, against our own 439-node largest legitimate
  fragment; lowering drops the **cheap flat** fragments first, and picking the number from our own tree
  repeats the D5/T13 error). Shape-aware is the *correct* instrument but depth is computed nowhere today
  and one adversarial fixture does not justify a second invisible drop rule — **YAGNI in v1**.
  (**Updated at the T14 review — N119: depth *is* now computed (`NormTree::depth()`, unrendered), so
  that objection is RESOLVED; the trigger is MET; D9 re-opens as a decision blocked on T15(c), and the
  v1 ceiling value is unchanged.**) **Any
  time- or budget-based cutoff is foreclosed outright** (it would make the candidate set a function of
  machine load — A7/R6/determinism), written down explicitly so nobody reaches for it later. **What N70
  did and did not establish** is recorded too: **one point, not a curve**, so the nested-family exponent
  is unknown and no replacement number is named; the cheap missing measurement (nested cost at ~500 /
  ~1 000 / ~2 000 nodes) is **optional, non-blocking, folded into T14's harness** rather than reopening
  T12.
- **N105 — the `cli.rs` stamp**, the single non-doc-file touch of this pass and a **comment-only** one.
  `MAX_NODES`'s doc comment said the number stands *"until the human rules on it"*; the human has ruled,
  so it now records **unchanged in v1, see D9**, with the trigger. **Value unchanged at 2000.**
- **N106 — R1 amended: mitigated and characterized, not discharged, and it ships open.** T12 gave the
  exponent, the pre-filter's true value — a stable constant, correctly demoted from "asymptotic win" —
  and a shape-dominated tail; those conclusions stay in the T12 record and R1 cites them. What T12
  cannot give is **any constant for real code**. R1's mitigation line now points at **T14's acceptance
  criteria (N107)** and states that **S3 closes with R1 carried forward as an accepted, characterized,
  triggered risk** — the honest state, and a fine state to ship v1 in. **SUPERSEDED at the T14 review
  (N112): T14 supplied what T12 could not, so R1 is now CLOSED as a risk and restated as a documented
  limit; and N107(1)'s "constant" turns out not to exist — see **A10** (N114).**
- **N107 — T14 gains three acceptance criteria, written as criteria rather than aspirations.**
  (1) **F per kLOC on real code as a function of `min_nodes`** — the actual gap nobody had named: the
  envelope is parameterized in F, every user has LOC, and without that constant `Θ(F²)` cannot answer
  *"how long on my 80 kLOC crate?"*; our one data point (134 fragments for `src`) is an anecdote.
  (**Answered at T14 §1, and the answer is that there is no constant — only a 10.6–43.4 range; A10.**)
  (2) **End-to-end wall-clock at a named scale target against a stated budget** — the budget was a
  **product decision reserved to the human**, recorded OPEN here at T12 with **no number invented**, in
  Anders' proposed form (one crate in the class of `syn`/`regex`/`ripgrep`, default flags, best-of-3,
  developer machine). Reason recorded: **without a stated budget no measurement can be a pass or a
  fail**, and R1 stays open by construction. **SET at T14: 10 s** (T14 record §0.1, with provenance and
  the rejected alternatives); the scale target **FAILED it** (§2). (3) The joint **`(node count, depth)`
  distribution of admitted fragments** — one histogram, which also discharges D9's trigger.
- **N108 — T14's corpus selection is pre-registered in its row, before any crate is picked.** Choosing a
  corpus after seeing which one flatters dedup is **fitting, the same error N102(3) names, wearing a
  different hat**. The declared criterion is **coverage of the cost-bearing shapes**: ≥1 crate with many
  small `impl` blocks, ≥1 **trait-heavy** crate (also the first real shot at A1/N86(a)'s revisit
  trigger), ≥1 crate with **generated code** (the only realistic source of real depth). Context recorded
  with it: N78, N79 and N84(c) are **three observations with one cause — our corpus contains no nested
  clone site**; dedup does nothing because there is nothing to do, and **T9 is not reopened** (T10 and
  the `d = 3` fixture witness the mechanism). The consequence is about evidence: **the output has never
  been observed on code where the architecture pays its `d²` cost.** The **counter-outcome is
  pre-written** — a `1.00` ratio across several real crates is a **genuine finding**, saying dedup's
  value is concentrated at nested/generated sites: a post-v1 revisit note, **not a removal**.

**Explicitly NOT rows (recorded so nobody picks them up).** (a) **The unexplained wall-clock anomaly**
(the `×7.81` step, *T12 record, N59/N96*) — correctly recorded as unexplained, the deterministic counters
carry the conclusion, and the re-run showed it non-reproducible. Chasing it on a loaded dev box is a time
sink with **no decision hanging on it**: **record-only, and said so here**. (b) **N84(c)'s `1.00` on its
own** — an **input to T14's corpus criteria (N108)**, not a standing risk.

**Micro-notes from the T12 review — recorded, deliberately not acted on.**
- `detect`'s `#[cfg(test)]` wrapper is a **second entry point production never uses**; the DRY-cleaner
  shape is for the tests to call `detect_counted(..).candidates` and delete it. **Cosmetic — fine to
  leave** (Anders); not churned now.
- **`RunStats` is public API surface.** `lib.rs:61` already carries the "treat any added field as a
  breaking change" note, so it is covered — but adding a **fifth counter** later is breaking under R3's
  spirit. The crate is unpublished, so today that costs nothing.

**Sequencing (Anders, recorded):** **D5 (with these repairs) → T12 (N59/N70/N75/N83, N84(c),
parameterized per N96, plus N93 and N86(a) in its doc pass) → T14 → D5-confirm + T13 together →
S3 close.** T14 sits before the two evidence-bearing items because it is what makes them dischargeable.
(**Re-sequenced at the T14 review — N115/N116/N118: T12 and T14 are done; what follows is **D10**, then
**D11** → **T13′**, and **D12** — all three human decisions, with **S4 = T15–T18** conditional on D10.**)

### T12 measurement record — the performance envelope (N59/N70/N83/N84c/N96)

**Machine and profile.** Windows dev box, `--release` (a debug timing number is worthless and appears
nowhere here). The box was under **unknown, non-trivial load** — the same configuration varied by up
to **2.7×** between repeats, and the nested worst pair measured `50.9 s` fastest in one session and
`70.1 s` in another. Every timing below is therefore reported as a **range over repeats**, and the
**fastest** is the headline (it is the least contaminated by contention). Anything load-sensitive is
labelled; the load-free counters (F, TED evaluations, candidate counts) are exact and reproducible.

**Harness.** `tests/perf.rs` — one asserted guardrail, one asserted N83 pin, and three `#[ignore]`d
benchmarks (**134 tests pass, up from the 132 baseline: +2 asserted, both new and both in
`tests/perf.rs` — `the_size_ratio_pre_filter_keeps_ted_evaluations_far_below_the_pair_count` and
`a_clone_nested_three_deep_costs_at_most_d_squared_ted_evaluations_for_one_finding`; the three
benchmarks are `#[ignore]`d and add nothing to the passed count**). Timing is never asserted — a
wall-clock threshold on a dev machine is a flake — but the **counters are**, because they are
deterministic and load-free. Pinned recipe per N90, every flag written out including the defaults:

```
cargo build --release
cargo test --release --test perf -- --ignored --nocapture --test-threads=1
# flags inside the harness: --threshold 0.85 --min-lines 4 --max-nodes 2000, --min-nodes as stated
```

`--test-threads=1` is load-bearing: the default parallel harness measures contention between the
benchmarks, not the pipeline.

**Re-running this on a verifier's budget (added at T12 review).** The pinned recipe above is the
*full* run — three repetitions over eight curve points, the better part of an hour, which is why the
first attempt to reproduce it was abandoned. Two environment variables, read at run time by
`tests/perf.rs`, make repetitions and point selection controllable; **defaults are unchanged**, so
the recipe above still produces the tables above:

| variable | default | effect |
|---|---|---|
| `DRY4RUST_PERF_REPEATS` | `3` | repetitions per timed configuration, floored at `1` |
| `DRY4RUST_PERF_MAX_F` | `800` | curve points above this requested F are skipped, in both `min_nodes` series |

The deterministic counters (F, TED evaluations, pre/post-dedup) are unaffected by either knob, so the
cheap points can be re-checked without paying for the expensive ones — this reproduces the `919` and
`3 782` TED-evaluation integers in about a minute:

```
DRY4RUST_PERF_REPEATS=1 DRY4RUST_PERF_MAX_F=200 \
  cargo test --release --test perf -- --ignored --nocapture --test-threads=1
```

`DRY4RUST_PERF_REPEATS=1` alone reproduces all four points and both N70 shapes at one run each; only
the fastest/slowest spread is lost, and that spread is exactly the part this record already declines
to treat as evidence.

#### N59/N96 — the curve, as a function of F with `min_nodes` a stated input

Synthetic corpora of free functions with structurally varied bodies (deterministic LCG over six
statement templates; node counts spread over a realistic band). F is the fragment count that clears
the floors and enters the O(F²) pair loop.

| `min_nodes` | requested | **F** | pairs `F(F−1)/2` | TED evals | admitted | pre-dedup | post-dedup | fastest | slowest |
|---|---|---|---|---|---|---|---|---|---|
| 20 | 100 | 100 | 4 950 | 919 | 18.6% | 2 | 2 | 1.12 s | 1.55 s |
| 20 | 200 | 200 | 19 900 | 3 782 | 19.0% | 14 | 14 | 8.75 s | 16.58 s |
| 20 | 400 | 400 | 79 800 | 15 842 | 19.9% | 47 | 47 | 16.94 s | 35.98 s |
| 20 | 800 | 800 | 319 600 | 64 280 | 20.1% | 203 | 203 | 85.06 s | 108.09 s |
| 35 | 100 | 86 | 3 655 | 855 | 23.4% | 1 | 1 | 1.41 s | 1.48 s |
| 35 | 200 | 172 | 14 706 | 3 531 | 24.0% | 7 | 7 | 5.31 s | 11.20 s |
| 35 | 400 | 349 | 60 726 | 14 970 | 24.7% | 20 | 20 | 22.77 s | 23.93 s |
| 35 | 800 | 695 | 241 165 | 60 287 | 25.0% | 63 | 63 | 76.50 s | 89.93 s |

**Shape — read the counter column, not the clock.** TED evaluations go `919 → 3 782 → 15 842 →
64 280`, i.e. **×4.11, ×4.19, ×4.06** per doubling of F: **exactly quadratic**, load-free and exactly
reproducible. Wall-clock broadly follows (fastest: ×7.81, ×1.94, ×5.02; 1.12 s → 85 s over an 8×
growth in F is ×76 against a quadratic's ×64), but **two of the three steps are anomalous in opposite
directions and T12 does not explain them**: 100 → 200 is **×7.81** against a pair-count growth of
**×4.11**, and 200 → 400 is **×1.94** against **×4.19**. An earlier draft attributed the first to
growth in mean fragment size; that is **wrong** — the generator cycles body lengths through the same
13 values, so doubling F leaves the size distribution *converging*, not growing — and it would in any
case predict the opposite sign for the second step. Unknown machine load is the most plausible cause,
but this data cannot separate load from warm-up from some other fixed effect, so it is recorded as
**unexplained**. Note that the *deterministic* TED-evaluation counts are untouched by whatever it is:
that is precisely why they, and not the clock, carry the N59 conclusion. A spot-check at
`DRY4RUST_PERF_REPEATS=1 DRY4RUST_PERF_MAX_F=200` (see the invocation below) supports the
load hypothesis without confirming it: the counters came back **identical** (`919`, `3 782`, `855`,
`3 531`, and every pre/post-dedup column), while the wall-clock came back `1.06 s → 4.85 s`, i.e.
**×4.58** — the ×7.81 step did **not** reproduce, and the 1.12 s / 8.75 s pair above is retained only
because N90 pins what was measured.

**The pre-filter is a constant, not an exponent — N59's bound on T11 confirmed.** The admitted
fraction is flat at ~19–20% across an 8× range of F (and ~24–25% at the higher floor, where the
surviving fragments are more uniform in size). T11's `4.7–5.5×` is therefore a **constant factor**
(1/0.20 = 5.0× — the same number, now shown to be stable rather than a single-corpus artifact) and
the pipeline remains **Θ(F²)** TED evaluations. R1 is mitigated, not discharged. (**R1's standing was
superseded at the T14 review — N112 closes it as a documented limit; the Θ(F²) statement here is
unaffected.**)

**N96 satisfied — what the two series do and do not show.** The supported claim is narrower than
"same curve": **the pair-count exponent is parameterized by F** — Θ(F²) TED evaluations, deterministic
and identical in both series at their own F — **while the constant is a function of the corpus's
node-count and shape distributions**, which the floor also changes. The `35` series is the *same*
corpora read at a higher floor, and it reproduces the exponent at its own F; that F = 695 happens to
time between the `min_nodes 20` points at F = 400 and F = 800 is **weak** evidence and is not offered
as more. So `min_nodes` enters the exponent only by setting F, and a later floor change — T13's
`20 → 30/40` — re-parameterizes this table along F rather than voiding it; but it does **not** simply
rescale it, for two reasons: (a) the floor also raises the *mean fragment size* of the survivors,
raising per-TED cost (visible as the rising admitted fraction, ~19–20% → ~24–25%); (b) F alone does
not determine wall-clock — our own corpus at F = 134 runs in **0.32 s** where a synthetic F = 100
takes **1.12 s**. That last discrepancy is the direct demonstration: two corpora at comparable F, a
3.5× wall-clock gap, because the synthetic fragments are larger. Exponent in F; constant in the
distributions.

#### N70 — the worst admitted pair, measured (and it is the finding of this task)

`MAX_NODES = 2000`, so the largest pair TED can be asked for is 2000 × 2000. Both fragments are
synthesized as a single function and **the shape is stated exactly**, because Zhang–Shasha costs
`O(n₁·n₂·min(depth₁,leaves₁)·min(depth₂,leaves₂))` — node count alone does not price it:

- **flat** — one `fn` with 284 sequential `total += values[i] * i;` statements → **1 997 nodes**,
  depth ≈ 3, ~1 000 leaves, so `min(depth, leaves)` ≈ 3.
- **nested** — one `fn` of 42 nested `if` blocks, six statements each → **1 983 nodes**, depth ≈ 85,
  so `min(depth, leaves)` ≈ 85.

The corpus is two files holding the same fragment, so F = 2 and the run is one parse pair plus
**exactly one** TED evaluation (asserted in the harness, not assumed).

| shape | nodes | TED evals | fastest | slowest |
|---|---|---|---|---|
| flat | 1 997 | 1 | **0.74 s** | 1.31 s |
| nested | 1 983 | 1 | **70.1 s** (50.9 s in a second session) | 189.8 s (341.2 s in a second session) |

**Both are reported; the flattering one is not the answer.** At the *same* node count the worst shape
measured is **~70–95×** more expensive than the typical one — but that precise range is **not robust**
under the session-to-session spread (`70.1 s` vs `50.9 s` for the same fragment), so the figure to
quote is the **conservative cross-range comparison: slowest-flat vs fastest-nested, `1.31 s` against
`50.9 s` ≈ 39×**. Even at ≈39× the gap is very large and the safety-ceiling concern stands unchanged.
Read the fixture honestly, too: **42 nested `if`s is pathological, not idiomatic** — it is an
**adversarial ceiling**, not an estimate of typical real code, where `match` arms are mostly wide
rather than deep and ordinary nested closures are shallower. It is not fantasy either: builder and
method chains, and especially **generated code**, do create real depth. A single user-visible
pathological pair therefore costs **tens of seconds to minutes**, independent of corpus size — and
N66 wrote the trigger in advance: *"if a single admitted maximal pair costs seconds, 2000 is too
generous."* **That condition is met.** Recorded in `cli.rs` beside the constant; the ceiling was **not**
changed by this task, and at the T12 review **the human ruled it stays flat at 2000 for v1 — see D9**,
which records why no lower flat value defends itself and defers the shape-aware ceiling that would.
Note the mitigation that already exists:
oversized fragments are dropped, not scored, and dropping an `impl` block is benign because its
methods are still compared individually.

#### N83 — d² TED evaluations per finding: **measured**, and refined

Measured, not derived, at the core seam and end-to-end (`tests/perf.rs`, asserted): two copies of a
site nested three levels deep (`impl` ⊃ method ⊃ closure) give F = 6, and of the `d² = 9` cross-file
pairs the pipeline pays **5** TED evaluations, produces **5** candidates, and reports **1**.

- The `d(d−1)` intra-copy pairs cost **nothing** — they overlap in source and die at admission (N61).
- `d²` is an **upper bound**: the size-ratio pre-filter also prunes cross-level pairs whose node
  counts are far apart (here both `impl↔closure` and both `method↔closure`), leaving `impl↔impl`,
  `method↔method`, `closure↔closure` and the two `impl↔method` crosses.
- **The refinement matters and cuts the wrong way for comfort:** the pairs the pre-filter *cannot*
  prune are exactly those whose nesting levels are close in size — a single-method `impl` against its
  own method — which is precisely the case where multiplicity is worst. So the bound is loose where
  nesting is size-varied and **tight where it hurts**. That reading is supported *for close-sized
  nested wrappers* (the single-method `impl` above is the canonical one) and **must not be read as a
  claim that all real deep nesting attains `d²`**: where levels differ substantially in node count the
  pre-filter prunes them, as it did for four of the nine pairs in this very fixture.
- All of it is paid **before** dedup, and cannot move pre-TED: a dominated pair must survive if its
  dominator fails the score gate. `design.md` already carries this; it is now a pinned number.

#### N84(c) — pre/post-dedup ratio, now observable

`RunOutput` gained a `stats: RunStats` counter block (F, TED evaluations, pre-dedup, post-dedup); the
report format is untouched. Pinned per N90, **on a detached worktree so the self-scan trap (N79)
cannot apply** — the live tree contains T12's own changes and is not a pinned corpus:

```
git worktree add ../dry4rust-perf-7261d7b --detach 7261d7b
DRY4RUST_PERF_CORPUS=../dry4rust-perf-7261d7b/src \
  cargo test --release --test perf -- --ignored --nocapture --test-threads=1 dedup_ratio
# harness flags: --threshold 0.85 --min-lines 4 --max-nodes 2000
```

| corpus | `min_nodes` | F | TED evals | pre-dedup | post-dedup | ratio | fastest | slowest |
|---|---|---|---|---|---|---|---|---|
| `src` @ `7261d7b` | 20 | 134 | 1 819 (20.4% of 8 911 pairs) | 17 | 17 | **1.00** | 0.32 s | 0.34 s |
| `src` @ `7261d7b` | 35 | 67 | 587 | 5 | 5 | **1.00** | 0.22 s | 0.23 s |
| synthetic (free functions only) | 20 | 100–800 | see curve | = post | = post | **1.00** | — | — |
| nested `d = 3` fixture | 20 | 6 | 5 | 5 | 1 | **5.00** | — | — |

**The honest reading: on our own corpus dedup removes nothing.** 17 pre = 17 post, which also
cross-checks the pipeline against D5 §6 (17 findings at `0.85`, same sha, same flags). That is not a
surprise and not a defect — it is N78's "zero cross-granularity survivors" and N79's bit-identical
negative control arriving through a third door. The consequence belongs on the record, stated no
wider than the evidence: **the architecture pays every admitted nested-pair TED before dedup runs,
and on the pinned project corpus (`src` @ `7261d7b`) that purchase bought no observed benefit** — 17
pre = 17 post. The `d²` multiplicity is **not** a per-corpus tax: it arises at *nested clone sites*,
and the synthetic free-function corpora are the immediate counterexample — nothing nests there, so
there is nothing to multiply and nothing to dedup (ratio `1.00` by construction, not by luck). Nor is
the benefit unwitnessed: T10's fixture is an observable witness outside the pinned corpus, and the
`5 : 1` row below is that measurement — what the ratio looks like where nesting is real.

**Negative control (N79 discipline) — T12 moved no output byte.** Run from *inside* the pinned
worktree so the reported paths are identical to D5's, with T12's binary:

```
cd ../dry4rust-perf-7261d7b
<t12-build>/dry4rust src --threshold 0.85 --format text   # 17 findings / 1172 bytes / sha A3B8ECF7
```

`1172` bytes and sha `A3B8ECF7` are **exactly** D5 §6's pinned figures. `Label: Copy`, the counter
plumbing and the new façade field changed no reported byte — as intended, since none of them is
rendered.

#### The envelope, stated

> **Wall-clock ≈ (parse, Θ(F)) + a·F²·p·t̄**, where `p` ≈ **0.20** is the pre-filter's admitted
> fraction (stable across F; ≈ 0.25 at `min_nodes 35`), `t̄` is the mean per-pair TED cost —
> `O(n₁·n₂·min(depth,leaves)²)`, so it depends on fragment **shape** as well as size — and **F is set
> by `min_nodes`**. The tail is bounded not by this curve but by the ceiling: one admitted pair at
> `max_nodes` costs `0.7 s` (flat) to `50–70 s` (nested) — the nested end is a session-to-session
> range, not a point (see *N70*).

**What T12 does not claim.** These are synthetic corpora on one loaded dev machine: they establish the
**shape** (Θ(F²), constant-factor pre-filter, shape-dominated tail) and pin the counters exactly, but
not the constant for any real codebase. T14's third-party corpus is the first honest source for that.
(**T14 supplied it, and the answer is that no single constant exists — F/kLOC spans 10.6–43.4, driven
by generated-code density: **A10** (N114).**)

#### Ledger

**Landed with T12 and closed:** N59, N70 (measured; the ceiling decision is the human's), N75
(`Label: Copy`, clone dropped at `ted.rs:80`), N83 (measured), N84(c) (observable, and reported),
N86(a) (in A1), N93 (in `similarity.rs` + `design.md`, number-free), N96 (curve parameterized by F).
**N24** (per-pair scratch buffers) is answered by the same data and **not** taken: allocation is not
the driver — the nested-vs-flat gap at equal node counts is **≈39× conservatively** (up to `~95×`
observed) and is pure DP work — so a scratch
buffer is a change we cannot justify on evidence (YAGNI). **N31/N33/N34/N40** remain record-only for
the same reason.

**Open ledger after T12:** N78/N84(a,b,d,e) (still owed → **T14**) · **T13** (`min_nodes`; T12's curve
is parameterized in F, so T13 **re-parameterizes** it rather than voiding it — not a simple rescale;
see *N59/N96 — the curve*; **T13 was SPLIT at the T14 review into D11 + T13′ — N118**) · N64/N69/N85/N86(c) (record) · N24/N31/N33/N34/N40 (record) ·
N46 (record) · N60 (record) · **the `MAX_NODES = 2000` ceiling is now evidence-backed as too generous
for pathological shapes — the human ruled at the T12 review that it stays flat at 2000 in v1; the
reasoning and the revisit trigger are D9.**

**Added at the T12 review (APPROVE):** **R9** (shape-dominated per-pair cost, N103) · **D9** (shape-aware
ceiling deferred, N104) · **R1** amended to *carried forward, characterized* (N106 — **superseded at the
T14 review: R1 CLOSES as a documented limit, N112**) · **T14** gains
acceptance criteria (N107) and pre-registered corpus criteria (N108) · `cli.rs`'s `MAX_NODES` comment
stamped (N105). All of it is documentation; **no behaviour, threshold, floor, ceiling value, report byte
or counter changed.** See §11 for the note-by-note ledger, including the two items explicitly recorded as
**not** rows.

### T14 measurement record — the third-party corpus (N102/N107/N108)

#### 0. Pre-registration (N108) — written down BEFORE the tool was run on any of these crates

Everything in this section was committed to the record before `dry4rust` was pointed at a single
third-party file. That ordering **is** the protocol: choosing a corpus after seeing which one flatters
the tool is *fitting*, the same error as writing a decision rule after seeing the curve (N102(3)).
Deviations from anything below are recorded explicitly in the results sections, with the reason.

##### 0.1 The budget — **10 s**, set by the human, deliberately aggressive

N107(2) recorded the budget as **OPEN, the human's to set, and not to be invented**. It is now **SET**:

> **10 seconds**, one mid-size crate, **default flags**, **best-of-3**, developer machine, `--release`.

**Provenance, recorded because D5's `0.75` had none (N87c).** The human chose `10 s` over two
alternatives he named and rejected: **30 s** ("a coin toss") and **60 s** ("likely to pass"). He chose
the number *knowing it would probably fail*, so that a failure would force optimisation work into S3
rather than let a comfortable line ratify the status quo. Three consequences follow and are binding on
this task:

- **A clean, well-measured FAIL against a deliberately hard line is a successful T14.** The verdict is
  the deliverable, not a pass.
- **T14 measures; it does not optimise.** Nothing was tuned to meet the number — no flag, floor,
  ceiling or code path was changed to move a timing. If the budget is blown, that is a **finding**, and
  any optimisation is a separate task the human sequences.
- The budget is **not softened, re-scoped or re-based** anywhere in this record.

##### 0.2 The corpus — crates, shas and justification, fixed before measurement

Chosen against N108's declared coverage criteria only, and **not** against any observed output. All
three are well known and permissively licensed (MIT **or** Apache-2.0). **No third-party source is
vendored into this repository**: the clones live outside the working tree at
`../_t14-corpus/<crate>` and are pinned by sha here. Only the harness, the pins and our measurements
are committed.

| slot | crate | scanned path | sha (pinned) | N108 criterion it covers | why this crate |
|---|---|---|---|---|---|
| **C1** | `serde` (serde-rs/serde) | `serde_core/src` | `a874a1b1bb1cc16cf5ee3b1b7b527af5705742bb` | **many small `impl` blocks** | `de/impls.rs` and `ser/impls.rs` are hundreds of tiny `impl Serialize/Deserialize for T` blocks, most of them single-method — exactly N83's *close-sized nested wrapper* case, where the size-ratio pre-filter **cannot** prune the `impl↔method` cross and the `d²` multiplicity is tight rather than loose |
| **C2** | `itertools` (rust-itertools/itertools) | `src` | `af6d17d3f4a963c087e81b327b957966e1169ff5` | **trait-heavy** | the `Itertools` trait is a single trait with ~100 **provided (default-body)** methods — A1's wrapper asymmetry says a `trait` emits no wrapper fragment, only its default bodies, and that path has never been exercised. First realistic chance at **N86(a)**'s revisit trigger |
| **C3** | `syn` (dtolnay/syn) | `src` | `b5d62a6e43a29418e118b7bcb48e211cefc0154f` | **generated code** + **scale target** | `src/gen/{clone,debug,eq,hash,visit,visit_mut,fold}.rs` are machine-generated by `syn-internal-codegen` — the only realistic source of the depth N70 needed a synthetic adversary to produce (feeds N107(3) and **D9**'s trigger). Also the **named scale target** for §0.1's budget: it is the first crate in Anders' proposed class (`syn` / `regex` / `ripgrep`) |

**Cross-crate pair for the zero-labelling protocol:** **C1 × C2** (`serde_core` × `itertools`) — two
crates with no shared lineage, no shared dependency and no shared domain. Every cross-crate hit is
non-actionable **by construction**, so the histogram reads the coincidence null with no human in the
loop.

**Pre-committed honesty clause.** If a crate turns out to be a poor fit for the criterion it was
chosen against, the result is **kept and the misfit reported** — it is not swapped for a
better-looking crate. Same rule as N108's "results reported whatever they turn out to be".

##### 0.3 The decision rule for the hand-label — pre-registered (N102(3)), stated before any labelling

§8's single `<50%` criterion is **replaced**. Writing a rule after seeing the precision curve is
*fitting*, so the rule is fixed here, in terms of the **contrast between adjacent buckets** and of §4's
**cost asymmetry** (a false positive costs a developer a read and a dismissal; a false negative costs
nothing that was not already being paid — so the default belongs *above* the F1-optimal point):

Let `p(B)` be the *actionable* precision of bucket `B`, over buckets
`[0.75,0.85)`, `[0.85,0.95)`, `[0.95,1.00)`, `=1.00`.

1. **Confirms `0.85`** iff `p([0.85,0.95)) − p([0.75,0.85)) ≥ 20` percentage points **and**
   `p([0.75,0.85)) < 50%`. The threshold must be shown to *separate*, not merely to sit somewhere.
2. **Refutes `0.85` downward** (the line is too high) iff `p([0.75,0.85)) ≥ 50%` — the dropped band
   carries enough signal to pay for itself, and `0.80` or `0.75` is the better line.
3. **Refutes the threshold as the instrument** (T13's thesis; the lever is `min_nodes`, not
   `--threshold`) iff **no** adjacent-bucket contrast reaches 20 points **and** every bucket including
   `=1.00` sits below 50%. A flat, low curve says the score does not rank actionability at all.
4. **Indeterminate** in every other case — reported as indeterminate, not rounded to the nearest
   conclusion.
5. **`=1.00` is a bucket, not a control.** T13's central claim rests on 7 exact-`1.00` findings that
   nobody has ever hand-labelled. If `p(=1.00)` is **below** `p([0.95,1.00))`, that is direct evidence
   for T13's dominant-false-positive-class claim and is reported as such under rule 3.
6. **Attribution is reported alongside, never folded in.** Precision is reported **twice** — *actionable*
   and *actionable-and-attributed* (R8). The rules above are evaluated on the *actionable* number, and
   the strict number is reported beside every one of them. On our own corpus the strict number was
   **0 of 10** (§7b); that is the honest headline form.
7. **`unsure` is a first-class label.** Third-party code the labeller does not know is genuinely hard to
   judge. An unsure pair is counted in the denominator and **not** in the numerator, and the count is
   reported per bucket, so the reader can see how much of the curve is guesswork.

##### 0.4 Sample size, pre-registered

**N = 8 per bucket** (4 buckets, 32 pairs), sampled **deterministically** — every `⌈n/N⌉`-th finding in
the run's own canonical `(left, right)` order — so the sample is reproducible from the pin alone and
carries no labeller discretion. Where a bucket holds fewer than 8 findings, **all** of them are
labelled and the actual `n` is stated. N is below §8's original 20 for one reason, stated in advance:
labelling is the expensive part of T14 and the acceptance criteria (N107 1–3) are sequenced first.

##### 0.5 What is measured, and with which pinned flags (N90)

Every number below is a `(corpus, sha, full flag set)` triple, **defaults written out**. Two flag sets
are used and are never mixed:

```
default    --threshold 0.85 --min-lines 4 --min-nodes 20 --max-nodes 2000
histogram  --threshold 0.75 --min-lines 4 --min-nodes 20 --max-nodes 2000
```

The `histogram` set exists only so the `[0.75, 0.85)` bucket is visible at all; it is not a proposal to
move any default.

#### 1. N107(1) — **F per kLOC on real code, as a function of `min_nodes`**

The gap Anders named: the envelope is parameterised in **F**, every user has **LOC**. Without this
constant `Θ(F²)` cannot answer *"how long on my 80 kLOC crate?"*. The 134-fragments-for-`src` number
was one anecdote on one corpus; this is three.

**Pinned command** (`DRY4RUST_CORPUS_ROOT=../_t14-corpus`, sha-verified at run time):

```
cargo test --release --test corpus -- --ignored --exact f_per_kloc_by_min_nodes --nocapture --test-threads=1
```

Flag set: `--threshold 0.85 --min-lines 4 --min-nodes {20,30,35,40,42} --max-nodes 2000`.

| crate (sha) | files | LOC | F @20 | **F/kLOC @20** | F @30 | F @35 | F @40 | F @42 |
|---|---|---|---|---|---|---|---|---|
| `serde_core` `a874a1b1` | 19 | 12 032 | 127 | **10.56** | 68 | 48 | 40 | 37 |
| `itertools` `af6d17d3` | 52 | 15 639 | 423 | **27.05** | 243 | 204 | 186 | 173 |
| `syn` `b5d62a6e` | 55 | 51 754 | 2 245 | **43.38** | 1 379 | 1 083 | 829 | 760 |

At `min_nodes = 20`: TED evaluations 1 808 / 17 456 / **521 365**; pairs considered on `syn`
**2 518 890**; findings 30 / 71 / **6 616**.

**Verdict — DELIVERED, with a caveat that must travel with it.** F/kLOC is **not a constant**. It
spans **10.6 → 43.4**, a **4.1× spread**, across three ordinary crates at the same flags. So
`Θ(F²)` still cannot be restated per-kLOC as a single number; the honest user-facing form is a
**range**, and the range is wide enough that the two ends differ by **17×** in predicted cost. The
driver is visible in the corpus itself: `syn` is 47% machine-generated (§4b), and generated code is
fragment-dense. `serde_core`, chosen for *many small `impl` blocks*, sits at the **bottom** — the
blocks are small enough that most fall under the 20-node floor. That is a **misfit against its
N108 criterion, kept and reported, not swapped** (§0.2's honesty clause): C1 does exercise the
close-sized nested-wrapper cross, but it is the *least* fragment-dense crate of the three, which is
the opposite of what "many small impl blocks" suggests.

`min_nodes` is a strong lever on F, and its effect is **crate-dependent**: 20→42 cuts F by
**71%** on `serde_core`, **59%** on `itertools`, **66%** on `syn`. Since cost is `Θ(F²)`, a 66% cut in
F is an ~**8.6×** cut in pair count.

#### 2. N107(2) — **end-to-end wall-clock vs the 10 s budget**

**Pinned command:**

```
cargo test --release --test corpus -- --ignored --exact end_to_end_wall_clock_against_the_budget --nocapture --test-threads=1
```

Flag set: **defaults** — `--threshold 0.85 --min-lines 4 --min-nodes 20 --max-nodes 2000`.
Best-of-3, `--release`, single-threaded test harness, machine otherwise idle.
Machine: Windows developer box, the same one as the T12 record (which measured up to **2.7×**
session-to-session variance on it — so the **fastest** repeat is the headline and the slowest is
printed beside it, never averaged).

| crate | fastest of 3 | slowest of 3 | vs 10 s |
|---|---|---|---|
| `serde_core` (12.0 kLOC) | **210.5 ms** | 244.6 ms | under, 48× headroom — context only |
| `itertools` (15.6 kLOC) | **3.467 s** | 6.494 s | under, but the *slowest* repeat is 65% of budget |
| **`syn` (51.8 kLOC) — the named scale target** | **103.878 s** | 124.222 s | **FAIL — 10.4× over** |

> ### **BUDGET VERDICT: FAIL.**
> **103.878 s against a 10 s budget — 10.4× over, on the fastest of three repeats.**

**This is a successful T14, not a failed one** (§0.1). The human set `10 s` over `30 s` and `60 s`
*expecting* it to fail, so that the failure would force the optimisation into S3. Nothing was tuned to
move this number.

**Where the time goes.** The T12 counters, exposed for exactly this, answer it without new
instrumentation: at defaults `syn` yields **F = 2 245** fragments → **2 518 890** pairs → after the
size-ratio pre-filter **521 365 TED evaluations**. Fragment extraction and parsing are not the cost;
the quadratic pair loop is. `itertools` at 15.6 kLOC passes with F = 423 (**17 456** TED evals); `syn`
is **3.3× the LOC** but **30× the TED evaluations** — the `Θ(F²)` envelope reproducing itself exactly
as D5 predicted, with the F/kLOC spread of §1 amplified by the square.

**A user-side mitigation was priced, and it is not enough.** Because 90.7% of `syn`'s findings are
generated-to-generated (§4b), the obvious question is whether excluding `src/gen/**` rescues the
budget. It does not:

```
target\release\dry4rust.exe <the 48 non-gen .rs files under syn/src> \
  --threshold 0.85 --min-lines 4 --min-nodes 20 --format json
```

| scan | LOC | findings | fastest of 3 | vs 10 s |
|---|---|---|---|---|
| `syn/src` (all 55 files) | 51 754 | 6 616 | 103.878 s | **10.4× over** |
| `syn/src` minus `gen/**` (48 files) | ~26 400 | **287** | **43.578 s** | **4.4× over** |

Halving the input removes **95.7%** of the findings and only **58%** of the time. Exclusion is a
findings-quality lever, **not** a performance fix. (Runs: 43.578 / 78.221 / 106.456 s — the variance
is the box, and is itself a reason the headline is best-of-3.)

##### 2b. Independent repeat (Bhaskar, T14 verification) — recorded beside, not merged into, the above

A second session on the same box re-ran both timings **once** (not best-of-3). Both sets are kept with
their pins; neither overwrites the other, because the **spread between sessions is itself the finding**:

| measurement | this record (best of 3) | verification repeat (1 run) | session swing | verdict on both |
|---|---|---|---|---|
| `syn/src`, defaults | **103.878 s** — 10.4× over | **66.618 s** — **6.7× over** | 1.56× | **FAIL either way** |
| `syn/src` minus `gen/**` | **43.578 s** — 4.4× over (58% of time removed) | **31.087 s** — **3.1× over** (~53% removed) | 1.40× | **still over budget; exclusion is not a performance fix either way** |

**The conclusions are unchanged in every case, and that is the point worth making.** A 1.4–1.6×
session swing on this box moves the multiple over budget (10.4× → 6.7×) without touching the verdict:
the scale target misses a 10 s budget by most of an order of magnitude, and removing 47% of the input
still leaves it 3–4× over. Timings on this box are quoted with their session, never as a constant.

#### 3. N107(3) — **the joint `(node count, depth)` distribution of admitted fragments**

Depth was **not computed anywhere** before this task; `NormTree::depth()` was added for it (leaf = 1,
counts-self, mirroring `node_count`), surfaced as `RunOutput::admitted: Vec<FragmentShape>` — unrendered,
no report-format change, façade goldens byte-exact, `RunStats` still `Copy`.

**Pinned command:**

```
cargo test --release --test corpus -- --ignored --exact admitted_fragment_shape_histogram --nocapture --test-threads=1
```

Flag set: **defaults**.

| crate | admitted F | deepest fragment | largest fragment | occupancy of the deep-and-large quadrant (≥100 nodes **and** depth ≥20) |
|---|---|---|---|---|
| `serde_core` | 127 | 70 nodes / **depth 15** | 181 nodes / depth 8 | **empty** |
| `itertools` | 423 | 168 nodes / **depth 19** | 753 nodes / depth 11 | **empty** (168/19 misses by one) |
| `syn` | 2 245 | 264 nodes / **depth 35** | 988 nodes / depth 22 | **9 fragments** — 4 at 250–499 nodes & depth ≥30, 3 at 100–249 & depth 20–29, 2 at 500–2000 & depth 20–29 |

> ### **D9 TRIGGER: HIT — on real, third-party code.**
> The deep-and-large quadrant is **not purely synthetic**. `syn` admits fragments at **depth 35 and
> 264 nodes**, and 9 fragments in the quadrant altogether. N70 needed a synthetic depth-85 adversary
> to reach it; real code reaches it too, less extremely.

Two qualifications, both material and neither softening the verdict:

- Real depth is **far** below the synthetic adversary (35 vs 85), and the observed quadrant is
  **thinly populated**: 9 of 2 245 fragments (**0.4%**) on the single crate that hits it at all, and
  **0 of 550** across the other two.
- All nine come from `syn`, and `syn` is the crate chosen *for* generated code. The shape is real, but
  its observed source in this corpus is narrow — a **one-crate** result, and it should be labelled as
  one rather than generalised to "Rust code reaches depth 35".

**D9's optional non-blocking nested-family exponent sweep**, which D9 asked to be folded into T14's
harness, was run (`nested_shape_cost_curve` in `tests/perf.rs`, `DRY4RUST_PERF_REPEATS=3`):

| target nodes | levels | actual nodes | TED evals | fastest of 3 | slowest of 3 |
|---|---|---|---|---|---|
| 500 | 11 | 526 | 1 | **339.712 ms** | 397.161 ms |
| 1 000 | 22 | 1 043 | 1 | **17.478 s** | 23.550 s |
| 2 000 | 43 | 2 030 | — | **above the 2 000-node ceiling, skipped** | — |

**A single TED evaluation** on a 1 043-node nested fragment costs **17.5 s** in this session and
**7.614 s** in the verification repeat (Bhaskar, one run) — see §8, where both are carried.

#### 4. N84(c) — **pre/post-dedup, and the pre-written counter-outcome**

**Pinned command:**

```
cargo test --release --test corpus -- --ignored --exact score_histogram_and_dedup_ratio --nocapture --test-threads=1
```

Flag set: **histogram** — `--threshold 0.75 --min-lines 4 --min-nodes 20 --max-nodes 2000`.

| crate | candidates **before** dedup | **after** | ratio |
|---|---|---|---|
| `serde_core` | 59 | 30 | **1.97×** |
| `itertools` | 112 | 71 | **1.58×** |
| `syn` | 19 205 | 6 616 | **2.90×** |

> ### **The pre-written 1.00 counter-outcome did NOT occur. Dedup does substantial real work.**

The brief pre-wrote the outcome "if it comes back 1.00 across several real crates too, that is a
genuine finding — a post-v1 revisit note, not a removal". **It came back 1.58–2.90.** So the
opposite is now on the record, and the earlier observation is explained rather than generalised:
**17-pre = 17-post was a property of our own fixture corpus**, which has no nested clone site (N78/N79
identified that cause correctly). On real code, dedup removes **37–66%** of raw candidates. This
**closes** N84(c) affirmatively and removes the post-v1 revisit note that the 1.00 outcome would have
created.

##### 4b. An unregistered observation, recorded because it dominates the scale target

Not asked for, found while labelling, and measured afterwards with a pinned command rather than left
as an impression:

```
target\release\dry4rust.exe D:\src\gh\_t14-corpus\syn\src --threshold 0.85 --min-lines 4 --min-nodes 20 --format json
```

| population | count | share |
|---|---|---|
| all findings on `syn` | 6 616 | 100% |
| **both sides inside `src/gen/**` (machine-generated)** | **6 001** | **90.7%** |
| one side generated, one hand-written | 328 | 5.0% |
| both sides hand-written | **287** | **4.3%** |
| exact-`1.00` findings | 3 432 | 100% |
| — of those, both sides generated | **3 215** | **93.7%** |

`src/gen/*.rs` carries the banner *"This file is @generated by syn-internal-codegen. It is not
intended for manual editing."* — so **90.7% of the tool's output on the scale target is, by
construction, non-actionable**: the duplication is the code generator's, and no local edit can remove
it. This is an **observation, not a decision**. It bears on T13 (§5), on the budget (§2 — excluding it
does not rescue the timing) and on N101's option (c) `--exclude`, and it is the human's to sequence.
(**Promoted at the T14 review to the risk register as **R10** (N113); the `--exclude` question is now
**D12** (N117).**)

#### 5. N102 — the hand-label, against the §0.3 pre-registered rule

**Pinned command** (deterministic stride sampling, N = 8 per bucket, reproducible from the pin alone):

```
cargo test --release --test corpus -- --ignored --exact hand_label_sample --nocapture --test-threads=1
```

Flag set: **histogram** (so the `[0.75,0.85)` bucket exists at all).

**Scope actually labelled, and the deviation from §0.4 stated plainly.** §0.4 pre-registered N = 8 per
bucket. Delivered: **all four buckets of `itertools` (32 pairs)** and **the `=1.00` bucket of `syn`
(8 pairs)** = **40 pairs**. **Not labelled: `serde_core` entirely, and `syn`'s three non-`=1.00`
buckets.** Reason, per the brief's own sequencing: labelling is the expensive part, N107 1–3 were
sequenced first, and the mandatory bucket is `=1.00`. `itertools` was chosen as the fully-labelled
crate because it is the trait-heavy slot and therefore also carries the A1/N86(a) verdict (§7).

**Labeller.** One labeller, an AI agent, not a maintainer of any of these crates. That is a real
limitation and is not dressed up: the rubric below is applied honestly but without maintainer context.

**Rubric.** `Yes` = a maintainer could remove the duplication with a straightforward local change and
would plausibly take the patch. `Borderline` = the duplication is real but removal is awkward or
costs more than it saves. `No` = nothing extractable; shape coincidence. `Unsure` = cannot judge.
Per §0.3(7), `Borderline`/`Unsure` count in the denominator, never the numerator.
Second column **`attributed`** (R8) = does the tool's evidence — a score over an
identifier/literal/macro-erased tree — match the *human's* reason for calling it duplication?

##### 5a. `itertools` `af6d17d3`, all four buckets

| bucket | n | Yes | Borderline | No | Unsure | **precision (actionable)** | **actionable-and-attributed** |
|---|---|---|---|---|---|---|---|
| `[0.75,0.85)` | 8 | 2 | 1 | 5 | 0 | **25.0%** | **12.5%** (1/8) |
| `[0.85,0.95)` | 8 | 4 | 2 | 2 | 0 | **50.0%** | **50.0%** (4/8) |
| `[0.95,1.00)` | 8 | 3 | 3 | 2 | 0 | **37.5%** | **37.5%** (3/8) |
| **`=1.00`** | 8 | 2 | 3 | 3 | 0 | **25.0%** | **25.0%** (2/8) |

The single `Yes`-but-**not**-attributed case is instructive and is the same defect R8 named on our own
corpus: `merge_join.rs` 150↔211 is a genuine duplication, but the tool's evidence is *the whole `impl`
block*, whereas the duplication a human would act on is the small `size_hint` body inside it. The tool
is right for a reason a human would not give.

##### 5b. `syn` `b5d62a6e`, the mandatory `=1.00` bucket

| bucket | n | Yes | Borderline | No | Unsure | precision (actionable) | actionable-and-attributed |
|---|---|---|---|---|---|---|---|
| **`=1.00`** | 8 | **0** | 1 | 7 | 0 | **0.0%** | **0.0%** |

**7 of the 8 sampled pairs are generated-to-generated** (`gen/debug.rs`, `gen/eq.rs`, `gen/fold.rs`,
`gen/hash.rs`) —
non-actionable by construction, consistent with §4b's 93.7%. The eighth (`buffer.rs` `ident()` ↔
`literal()`) is a real same-shape/different-variant pair that a macro could collapse and that the
maintainer evidently chose not to: `Borderline`.

##### 5c. Verdict against the §0.3 rule — evaluated as written, before the curve was seen

- `p([0.85,0.95)) − p([0.75,0.85))` = `50.0 − 25.0` = **25.0 pp ≥ 20 pp** ✔, and
  `p([0.75,0.85))` = `25.0% < 50%` ✔ →

> ### **Rule 1 FIRES — on low-confidence evidence (n = 8 per bucket, one crate, one non-maintainer labeller).**
> The pre-registered condition is met, and is reported as met: precision doubles across the line
> (25.0% → 50.0%). **This is not a demonstration that `0.85` separates.** The 25 pp contrast is
> **two pairs** wide and clears the 20 pp bar by only **5.0 pp**, and one label is worth **12.5 pp** of
> a bucket — so **a single relabel in the right direction unfires the rule** (§9(4)). Either direction
> does it: row **7**, `No → Yes` in `[0.75,0.85)`, takes that bucket to 37.5% and the contrast to
> `50.0 − 37.5` = **12.5 pp**; any one of rows **9/10/11/16**, `Yes → Borderline`/`No` in
> `[0.85,0.95)`, takes that bucket to 37.5% and the contrast to `37.5 − 25.0` = **12.5 pp**. A
> pre-registered rule that fires with a **one-relabel** margin is much weaker evidence than one that
> survives two, and that sensitivity belongs in this headline, not only in the caveats, because the
> headline is what gets quoted. What is on the record is *"the pre-registered rule fired on a small
> sample"*, not *"the threshold is shown to separate"*.
> Rules 2 and 3 do not fire (`p([0.75,0.85)) < 50%`; an adjacent contrast of 25 pp exists).

> **Row 7 is pivotal, and it is a † row** — one of the five §5e rows whose deliberation was close and
> moved between labels while labelling. It is `position_max` vs `position_max_by_key`, tallied **`No`**
> in the low bucket: that is the direction that **helps** rule 1 fire, and it is individually decisive —
> **had it been tallied `Yes`, the rule would not have fired**. Disclosed here, next to the sensitivity,
> because that is where a reader meets it.
> **The other four † rows do not steer the rule** (Bhaskar, T14 verification): rows **12** and **14**
> moved *away* from `Yes` in the retained `[0.85,0.95)` bucket and therefore **hurt** rule 1, and rows
> **19** and **20** sit in `[0.95,1.00)`, which rule 1 does not read. So there is **no systematic
> directional steering** in the deliberated rows — the one exposure is a single pivotal close call, and
> it is on the firing side.

- **A limitation of the pre-registered rule itself, recorded rather than repaired (Bhaskar, T14
  verification).** Rule 1 can fire while the above-threshold bucket is **still mostly false positives**:
  here `p([0.85,0.95))` = **50.0%** — half of what the default retains is non-actionable — and
  `p(=1.00)` = 25.0%. Measured against §4's cost-asymmetry rationale, which is an argument about what a
  false positive costs a reader, a rule that tests only the *contrast* between buckets and never the
  *absolute* level of the retained bucket is weak evidence for a default. The rule was
  **pre-registered**, so it is evaluated exactly as written and the weakness is **reported, not
  rewritten** — writing or amending the rule after seeing the curve is the fitting error N102(3) exists
  to prevent. A successor rule with an absolute-precision term is the human's to set, **before** the
  next labelling.
- **Chronology, stated plainly (Bhaskar, T14 verification).** §0's pre-registration and every result in
  §1–§8 are landing together in the **same uncommitted change**, so **pre-registration cannot be
  established from the artifact**: the record asserts the ordering and nothing in git witnesses it. Two
  things are evidence *against* wholesale fitting — `serde_core` is kept and reported as a **misfit**
  against its own N108 criterion (§1) rather than swapped, and the pre-written `1.00` dedup
  counter-outcome was left standing in falsifiable form and then did not occur (§4). **That is
  evidence, not proof.** The fix is procedural and belongs to the *next* task, not to a retrofit here:
  **commit the pre-registration before measuring.**

- **Rule 5 also holds, and points the other way about the top of the range.** `p(=1.00)` = **25.0%** is
  **below** `p([0.95,1.00))` = **37.5%**, and on `syn` it is **0.0%**. The precision curve is
  **non-monotonic**: it rises across `0.85`, peaks in `[0.85,0.95)`, and **falls** at exact `1.00`.
  Per §0.3(5) this is **direct evidence for T13's dominant-false-positive-class claim** — the exact-
  `1.00` population is disproportionately shape coincidence (mirrored `next`/`next_back`, `min`/`max`,
  `sub_scalar`/`mul_scalar`) and, on the scale target, machine-generated code. T13's seven
  never-labelled exact-`1.00` findings now have 16 labelled third-party siblings, and they do **not**
  behave like the best findings in the run.
- **Both results stand together and neither is rounded away:** `0.85` **survives** its pre-registered
  test on this evidence, and score **above** it does not rank actionability. That is precisely T13's
  thesis restated from data — the remaining lever is `min_nodes` (§6), not `--threshold`.
- Strict (attributed) precision peaks at **50.0%** and never exceeds actionable precision, as expected.
  It is far better than the **0 of 10** our own corpus produced (§7b) — because `itertools`' clones are
  small, whole-function and identifier-driven, exactly the case the erased tree scores for the right
  reason.

##### 5d. T13's contested floor — node counts of actionable vs non-actionable findings

N100 measured our one actionable finding at **26 nodes**, *inside* the coincidence mass, so a 30–40
floor removes it first and takes precision 10%→0% on our own corpus. This is what T13 needed and our
corpus could not give — the `min(left,right)` node count of the 40 labelled third-party pairs.

**Correction, found by building §5e's ledger and stated before the table it corrects.** The version of
this table first written into the record (`Yes` n = 8 / median 28 / range 20–61 / 5 of 8; `Borderline`
n = 10 / 27 / 20–74; `No` n = 22 / 26 / 20–239) **cannot be recomputed from the labelled sample** — its
`n`s do not match §5a+§5b's own counts (`Yes` is 2+4+3+2+0 = **11**, `No` is 5+2+2+3+7 = **19**) and its
ranges name node counts (20, 61, 74, 239) that no sampled pair has. It was a transcription defect, not a
relabelling: the labels themselves are unchanged and reproduce §5a/§5b exactly. The table below is
**recomputed from §5e row by row**, and every figure in it is checkable there. This is precisely the
class of error that aggregate-only reporting hides, and it was caught the moment the ledger existed.

| label | n | median min-nodes | range | **share at < 30 nodes** |
|---|---|---|---|---|
| `Yes` (actionable) | **11** | **27** | 21–74 | **7 of 11 (64%)** |
| `Borderline` | 10 | 28 | 22–87 | 6 of 10 (60%) |
| `No` (non-actionable) | **19** | **25** | 21–53 | 15 of 19 (79%) |

(`median` = the middle order statistic, taking the **upper** of the two middles for even `n` — the same
`nodes[len/2]` convention the harness prints. Worked: `Borderline` sorted is
`22,23,24,26,26,28,46,48,52,87`, whose lower middle is 26 and whose `nodes[10/2]` is **28** — 28 is the
figure reported, and the convention is the upper middle.)

> **The floor separates only weakly, and the price is high.** Actionable and non-actionable findings
> have **overlapping** node counts on real code (medians **27 vs 25** — no statistical test was run, so
> this is an **overlap observed, not a difference tested**), and **64% of actionable findings
> sit below 30 nodes** — so a `min_nodes = 30` floor would remove roughly **two of every three true
> findings** along with the coincidence mass. What separation the floor does buy is **weak, in the
> desired direction, and bought at that 64% actionable-recall loss** — it concedes the separation and
> rejects the price. The arithmetic is recomputed once at **§6/N118** (retained shares, both
> denominator conventions) and is deliberately **not re-derived here**. N100's 26-node result on our own
> corpus was **not** an artifact of a 10-finding sample; it reproduces on third-party code at 40 pairs.

This is a **negative result for the 30–40 floor as a precision instrument** and it is reported as it
falls. It does not decide T13 — the floor is also T13's only lever on *cost* (§1: 20→42 cuts F by
59–71%, an ~6–12× cut in pair count), and §2's FAIL makes that lever valuable for a reason that has
nothing to do with precision. **Precision and cost now pull in opposite directions on the same knob,
and which one wins is the human's decision, not this record's.** (**De-symmetrised at the T14 review —
N118**: the two legs measure different populations, the pro-floor leg is the weaker one, and the
coupling exists only because `min_nodes` is currently the sole cost lever. The decision is **D11**.)

##### 5e. The per-pair ledger — all 40 labels, so every aggregate above is recomputable

Aggregates alone are unauditable: a reader can check a named example but not an arbitrary one, so
neither steering nor transcription error can be excluded — and §5c's headline turns on **two** pair
labels. The full ledger is therefore on the record. **These are the labels made at labelling time,
transcribed, not re-made**; the sample identities, scores and node counts are reproduced verbatim from
`hand_label_sample`'s own output at the pins. `min(nodes)` = `min(left_nodes, right_nodes)`, the
quantity §5d and T13's floor are about. Paths are relative to each pin's scanned path (§0.2).

`attributed` is recorded only where it is defined — for an actionable pair, per §0.3(6); the strict
"actionable-and-attributed" numerator counts exactly the rows that are `Yes` **and** `Yes`. Rows marked
**†** are ones whose deliberation was close and moved between labels while labelling; the label shown
is the one that was tallied into §5a, and the deliberation is stated in the reason rather than hidden.

**`itertools` `af6d17d3` — `[0.75, 0.85)`**

| # | pair | score | min(nodes) | actionable | attributed | reason |
|---|---|---|---|---|---|---|
| 1 | `adaptors/coalesce.rs:183-194` ↔ `:247-262` | 0.7647 | 26 | Borderline | — | mirrored `CoalescePredicate` if/else; the with-count variant threads a tuple through, so factoring is not clearly worth it |
| 2 | `adaptors/mod.rs:939-961` ↔ `:1044-1066` | 0.7674 | 53 | No | — | `FilterOk` vs `FilterMapOk` double-ended impls: parallel adapter families, bodies differ (`rfind` vs `find_map`) |
| 3 | `combinations_with_replacement.rs:112-123` ↔ `:125-141` | 0.7547 | 40 | **Yes** | **Yes** | `nth` copies `next`'s prologue verbatim before adding its own loop — exactly the duplication a reader would cite |
| 4 | `either_or_both.rs:151-157` ↔ `:182-192` | 0.8361 | 28 | No | — | `as_ref` vs `as_deref_mut`: same three-arm `Left/Right/Both` shape, divergent semantics; unifying buys nothing |
| 5 | `either_or_both.rs:195-201` ↔ `:287-293` | 0.8033 | 27 | No | — | `flip` vs `or`: the three-arm match shape again, unrelated intent |
| 6 | `either_or_both.rs:246-254` ↔ `:346-354` | 0.7736 | 23 | No | — | `left_and_then` vs `left_or_insert_with`: shape coincidence over the same enum |
| 7 † | `lib.rs:4587-4595` ↔ `:4617-4626` | 0.7742 | 24 | No | — | `position_max` vs `position_max_by_key`: near copy-paste apart from the `key()` call, but the by-key variant is the idiomatic family member — close call, tallied `No` |
| 8 | `merge_join.rs:150-169` ↔ `:192-211` | 0.7920 | 50 | **Yes** | **No** | identical `size_hint` bodies in two `OrderingOrBool` impls — real, but the tool's evidence is the whole impl block while the duplication a human would act on is the small body inside it (the R8 case in §5a) |

**`itertools` `af6d17d3` — `[0.85, 0.95)`**

| # | pair | score | min(nodes) | actionable | attributed | reason |
|---|---|---|---|---|---|---|
| 9 | `adaptors/mod.rs:78-104` ↔ `:201-227` | 0.8916 | 74 | **Yes** | **Yes** | two `fold` bodies differing only in a couple of match arms |
| 10 | `combinations.rs:136-144` ↔ `combinations_with_replacement.rs:150-158` | 0.9167 | 22 | **Yes** | **Yes** | the two counting bodies (`n_and_count` / `count`) share their arithmetic |
| 11 | `either_or_both.rs:287-293` ↔ `:298-308` | 0.8667 | 28 | **Yes** | **Yes** | `or_default` is `or` with `Default::default()` arguments |
| 12 † | `either_or_both.rs:384-409` ↔ `:426-450` | 0.9259 | 52 | Borderline | — | mirror-image `insert_left`/`insert_right`, `unsafe` blocks and SAFETY comments included; genuine duplication but on different fields — weighed as `Yes`, tallied `Borderline` |
| 13 | `lib.rs:4389-4395` ↔ `:4560-4566` | 0.9130 | 21 | No | — | `max_set_by` vs `minmax_by`: one-line delegations with matching closure shape, unrelated intent |
| 14 † | `merge_join.rs:138-147` ↔ `size_hint.rs:77-86` | 0.8763 | 43 | No | — | merge_join's inline lower/upper combination vs the `size_hint::min` helper — an idiom-placement argument, not literal duplication; weighed as a weak `Yes`, tallied `No` |
| 15 | `multipeek_impl.rs:91-113` ↔ `put_back_n_impl.rs:52-71` | 0.9231 | 48 | Borderline | — | parallel `Iterator` impls over `VecDeque` vs `Vec`; a shared helper is awkward across the buffer types |
| 16 | `unique_impl.rs:78-81` ↔ `:132-135` | 0.9200 | 23 | **Yes** | **Yes** | two `size_hint` bodies differing only by `self.iter` vs `self.iter.iter` — one should delegate to the other |

**`itertools` `af6d17d3` — `[0.95, 1.00)`**

| # | pair | score | min(nodes) | actionable | attributed | reason |
|---|---|---|---|---|---|---|
| 17 | `adaptors/mod.rs:1199-1206` ↔ `:1245-1258` | 0.9583 | 23 | Borderline | — | `Update::next`/`next_back`: near-identical bodies differing only in which iterator method is called |
| 18 | `lib.rs:4587-4595` ↔ `:4648-4656` | 0.9600 | 24 | **Yes** | **Yes** | `position_max` reduces to `position_max_by(Ord::cmp)` |
| 19 † | `lib.rs:4587-4595` ↔ `:4738-4746` | 0.9600 | 24 | No | — | `position_max` vs `position_min_by`: cross max/min; the family-level duplication is real but no pairwise fix exists — weighed `Borderline`, tallied `No` |
| 20 † | `lib.rs:4648-4656` ↔ `:4677-4685` | 0.9600 | 24 | No | — | `position_max_by` vs `position_min`: the same cross max/min case, same reasoning |
| 21 | `lib.rs:4677-4685` ↔ `:4738-4746` | 0.9600 | 24 | **Yes** | **Yes** | `position_min` reduces to `position_min_by(Ord::cmp)` |
| 22 | `size_hint.rs:61-73` ↔ `:77-86` | 0.9583 | 46 | Borderline | — | `max`/`min` differ by `cmp::max`/`cmp::min` **and** in the `_ => None` fallback; factoring would hide the difference |
| 23 | `unique_impl.rs:72-75` ↔ `:89-100` | 0.9565 | 22 | Borderline | — | `next`/`next_back` find/rfind mirror — same category as `Update` |
| 24 | `unique_impl.rs:119-129` ↔ `:142-159` | 0.9500 | 38 | **Yes** | **Yes** | the `find_map` closure body is duplicated verbatim across `next`/`next_back`; extract it as a named function |

**`itertools` `af6d17d3` — `=1.00`**

| # | pair | score | min(nodes) | actionable | attributed | reason |
|---|---|---|---|---|---|---|
| 25 | `adaptors/mod.rs:906-912` ↔ `:944-950` | 1.0000 | 26 | Borderline | — | `FilterOk::next`/`next_back`: verbatim mirror, hard to factor across find/rfind |
| 26 | `duplicates_impl.rs:173-183` ↔ `:187-197` | 1.0000 | 23 | No | — | `KeyXorValue` impls for `KeyValue`/`JustValue` — the two types are *meant* to differ |
| 27 | `flatten_ok.rs:75-96` ↔ `:157-178` | 1.0000 | 87 | Borderline | — | `fold`/`rfold` mirror; factoring this pair in Rust is awkward |
| 28 | `grouping_map.rs:334-342` ↔ `:415-423` | 1.0000 | 28 | Borderline | — | `max_by`/`min_by`: a helper could swap the comparison order, but the `Equal` handling differs |
| 29 | `lib.rs:4292-4298` ↔ `:4389-4395` | 1.0000 | 21 | No | — | `min_set_by`/`max_set_by` are one-line delegations; nothing to extract |
| 30 | `peek_nth.rs:71-77` ↔ `:112-118` | 1.0000 | 27 | **Yes** | **Yes** | `peek_nth`/`peek_nth_mut` share buffering logic a private `fill` method would carry |
| 31 | `peeking_take_while.rs:96-106` ↔ `:134-144` | 1.0000 | 21 | **Yes** | **Yes** | two verbatim `peeking_next` bodies in separate `PeekingNext` impls |
| 32 | `size_hint.rs:31-36` ↔ `:52-57` | 1.0000 | 28 | No | — | `sub_scalar`/`mul_scalar` share shape only; the operation *is* the content |

**`syn` `b5d62a6e` — `=1.00` (the mandatory bucket)**

| # | pair | score | min(nodes) | actionable | attributed | reason |
|---|---|---|---|---|---|---|
| 33 | `buffer.rs:192-198` ↔ `:240-246` | 1.0000 | 24 | Borderline | — | `ident()`/`literal()`: same shape, different `Entry` variant — collapsible only by introducing a macro, which the maintainer evidently chose not to do |
| 34 | `gen/debug.rs:267-276` ↔ `:2541-2550` | 1.0000 | 38 | No | — | machine-generated by `syn-internal-codegen`, banner-marked "not intended for manual editing" — non-actionable by construction |
| 35 | `gen/debug.rs:884-891` ↔ `:2998-3005` | 1.0000 | 27 | No | — | generated, as #34 |
| 36 | `gen/debug.rs:2014-2021` ↔ `:2458-2465` | 1.0000 | 27 | No | — | generated, as #34 |
| 37 | `gen/eq.rs:410-415` ↔ `:529-535` | 1.0000 | 30 | No | — | generated, as #34 |
| 38 | `gen/eq.rs:676-680` ↔ `:901-905` | 1.0000 | 24 | No | — | generated, as #34 |
| 39 | `gen/fold.rs:1087-1097` ↔ `:1684-1694` | 1.0000 | 25 | No | — | generated, as #34 |
| 40 | `gen/hash.rs:1589-1599` ↔ `:2241-2251` | 1.0000 | 22 | No | — | generated, as #34 |

**Recomputing the record from this table alone** (the point of having it):

- §5a `itertools` per bucket — `[0.75,0.85)` rows 1–8: Yes 2 / Borderline 1 / No 5 / Unsure 0 →
  **25.0%**, attributed 1/8 = **12.5%**. `[0.85,0.95)` rows 9–16: 4 / 2 / 2 / 0 → **50.0%**,
  attributed 4/8 = **50.0%**. `[0.95,1.00)` rows 17–24: 3 / 3 / 2 / 0 → **37.5%**, attributed 3/8 =
  **37.5%**. `=1.00` rows 25–32: 2 / 3 / 3 / 0 → **25.0%**, attributed 2/8 = **25.0%**.
- §5b `syn` `=1.00` rows 33–40: 0 / 1 / 7 / 0 → **0.0%**, attributed **0.0%**.
- §5c rule 1: `50.0 − 25.0` = **25.0 pp**, clearing the 20 pp bar by **5.0 pp** — and one label is
  worth **12.5 pp**, so **any single relabel unfires it**: row **7** `No → Yes` takes `[0.75,0.85)` to
  37.5% (contrast 12.5 pp), and any one of rows **9/10/11/16** `Yes → Borderline`/`No` takes
  `[0.85,0.95)` to 37.5% (contrast 12.5 pp). Row **7** is a † row and is individually decisive (§5c).
  Rule 5: `p(=1.00)` 25.0% < `p([0.95,1.00))` 37.5%.
- §5d: the three `min(nodes)` populations are rows {3, 8, 9, 10, 11, 16, 18, 21, 24, 30, 31} (`Yes`,
  n = 11), {1, 12, 15, 17, 22, 23, 25, 27, 28, 33} (`Borderline`, n = 10) and the remaining 19 (`No`).
- §5a's `Yes`-but-not-attributed example is row **8**, and it is the only one.
- **`Unsure` was never used**: 0 of 40. §0.3(7) kept it available and no pair needed it — which is
  itself worth reading with suspicion given a single labeller, and is recorded as a caveat in §9(3).

#### 6. The zero-labelling protocol (N102) — the coincidence null

Kept as specified; the `attributed` column is **not** added (it is undefined when every hit is
non-actionable by construction). One axis added per the brief: bucketed by **node count** as well as
score, which prices T13's floor against the null.

**Pinned command:**

```
cargo test --release --test corpus -- --ignored --exact cross_crate_coincidence_histogram --nocapture --test-threads=1
```

Corpus: `serde_core` `a874a1b1` × `itertools` `af6d17d3`, 27.7 kLOC combined. Flag set: **histogram**.

| cut-off | cross-crate hits | **per kLOC** |
|---|---|---|
| `≥ 0.75` | **6** | **0.217** |
| `≥ 0.85` (the default) | **0** | **0.000** |
| `= 1.00` | **0** | **0.000** |

(197 findings total in the combined run; 6 of them cross the crate boundary.)

By node band, all six: **5 at 20–24 nodes**, **1 at 30–39**, none above.

**Reading.** The coincidence null at the shipping default is **zero per kLOC** — two unrelated crates
produce **no** ≥0.85 cross-crate hit at all. That is a genuinely good result for the `0.85` default and
it agrees with §5c's rule-1 firing from a completely independent direction (no human in the loop). It
also says the coincidence mass that *does* exist lives **at the floor**: 5 of 6 hits are in the
smallest node band. That is the strongest available argument **for** T13's floor — and it sits directly
against §5d, where the floor also removes 64% of true findings. Both are now measured; neither is
suppressed.

> **De-symmetrised at the T14 review (N118, Anders) — the two legs are NOT measuring the same
> population, so this is not a genuine tie.** §5d's 7-of-11 is about **actionable findings a user
> loses**; this section's 5-of-6 is about **cross-crate coincidences between unrelated crates**, a
> population **users never encounter** — and §5d/§5e show the `No` rows spanning **21–53 nodes, median
> 25**, against `Yes`'s median **27**: the two populations **overlap heavily**, and what separation the
> floor buys is **weak and paid for in recall** (quantified below). (An earlier draft of this paragraph
> cited "21 to 239, median 26" — that is the **defective aggregate §5d retracts**; on the true range the
> *sub*-argument it carried, that the coincidence mass is not confined to the floor, does **not**
> survive: **15 of 19 `No` rows sit below 30 nodes**. The de-symmetrisation does not rest on that
> sub-argument. It rests on the two legs measuring different populations, and it survives **unchanged
> and on its own footing** — the correction does **not** strengthen it. What the corrected ledger
> actually says at a floor of 30: `Yes` loses **7 of 11**, so **4/11 are retained**; `No` loses **15 of
> 19**, so **4/19 are retained**. The floor therefore **preferentially retains `Yes` rows**, and the
> actionable share **rises**: **11/30 = 36.7% → 4/8 = 50.0%** excluding `Borderline`, or **11/40 = 27.5%
> → 4/12 = 33.3%** counting `Borderline` as non-actionable. That is **weak separation in the desired
> direction, bought at the loss of 64% of actionable findings** — which is a *stronger* argument against
> raising the floor than "it does not separate", because it **concedes the separation and rejects the
> price**; read it that way and not as having gone soft. These figures **cannot** be compared for
> separator quality against the retracted aggregate, because that aggregate has **no recomputable
> distribution**. §5d's caveat governs unchanged: **an overlap observed, not a difference tested.**)
> **The pro-floor leg above is
> the weaker one**, and the "opposite directions on the same knob" framing holds only because
> `min_nodes` is currently the **sole cost lever** — a knob shortage, not evidence. The ruling is
> **D11**'s; see **N118**.

#### 7. The remaining T14 consumers

- **N86(a) / A1's revisit trigger — verdict: NOT triggered.** A1's trigger is a specific observation:
  *block-level trait clones demonstrably missed*. `itertools` was chosen as the first real test (its
  `Itertools` trait has ~100 provided methods; our own corpus is trait-poor). Of the 32 labelled
  `itertools` pairs, the actionable ones are **method-body** clones and the tool **found** them
  (`unique_impl` `size_hint`, `adaptors` `fold`, `either_or_both` `or`/`or_default`,
  `peeking_take_while`'s two verbatim `peeking_next` bodies). **No case was found where a duplicated
  `impl`-block-as-a-whole was missed because the trait emitted no wrapper fragment.** A1 stands as
  decided; the trigger is not met on this evidence. This is a **32-pair sample on one crate**, so it is
  "not triggered", not "cannot be triggered".
- **N84(d), test-vs-non-test partition — NOT delivered, and stated as a gap.** All three scanned paths
  are library `src` trees; `serde_core` and `syn` keep their tests outside the scanned path entirely,
  and `itertools`' in-file `#[cfg(test)]` modules were not partitioned out. So this corpus **cannot**
  answer N84(d) as configured, and no number is invented for it. It needs either a corpus slot chosen
  for its test tree or a partition in the harness; both are new work.
- **N84(a), N84(e)** are served by §1/§2 (F and its cost consequence) and §4 respectively.
- **D5-confirm** is served by §1 (the F envelope on real code) and §2 (the `Θ(F²)` reproduction:
  3.3× LOC → 30× TED evaluations).

#### 8. D9's nested-family exponent (optional, non-blocking)

`nested_shape_cost_curve`, `DRY4RUST_PERF_REPEATS=3`, defaults, `--release`. Two usable points only —
the third target (2 030 nodes) is above the 2 000-node ceiling and is **correctly skipped**, which is
itself the ceiling doing its job.

Recorded in §3's table. **1.983× the nodes (526 → 1 043) costs 51.4× the time (339.712 ms → 17.478 s)**,
an apparent exponent of **≈ 5.8**. The verification repeat (Bhaskar, one run) measured the same two
points at **34.4×** with the same 1.98× node ratio, and the large point at **7.614 s** rather than
17.478 s. **Both are kept; neither is averaged away** — and the reading below is stated so that it
holds at either end. **Read with care:** two points, on a box with measured 2.7×
session variance and a 34.4×–51.4× session spread on this very ratio, cannot pin an exponent, and the
small point carries fixed overhead that inflates the slope. What can be said is bounded and is said
that way — the nested family's cost grows **steeply super-quadratically in node count**, and the two
observations are **consistent with, and possibly worse than**, D5's `O(n²·d²)` (which, with `d` growing
linearly in `n`, predicts ~15×; the observed 34.4×/51.4× are both above it, but two noisy points cannot
establish that the curve *is* at or beyond the theoretical one — only that nothing here contradicts it).
A **single** TED evaluation on one 1 043-node
nested fragment costs **17.5 s** in one session and **7.6 s** in another — so *"one pair can approach or
exceed the whole 10 s budget"* is **session-dependent**: it happened, and it did **not** reproduce on
the repeat. The 2 000-node ceiling
is the only thing standing between that curve and an unbounded run. D9's decision is **unchanged**; this
measurement supports the ceiling's existence rather than revising anything. (**At the T14 review — N119
— D9 RE-OPENS as a decision blocked on T15(c): the trigger is MET and the "depth not computed"
objection is resolved. The v1 ceiling value is still unchanged, and a shape-aware ceiling is **not**
admissible under D8 because it drops fragments.**)

#### 9. What T14 did **not** reach

Stated plainly, per the brief's "a partial T14 with honest gaps beats a rushed complete one":

1. **`serde_core` and `syn` are not fully hand-labelled** — 40 of a full-programme 96 pairs. `syn`'s
   mandatory `=1.00` bucket **is** labelled; its other three buckets and all four of `serde_core`'s are
   not. Cost, and N107 1–3 sequenced first (§5).
2. **N84(d)** test-vs-non-test is not answerable from this corpus (§7).
3. **Single labeller, not a maintainer** of any of the three crates. No inter-labeller agreement number
   exists, so the precision figures carry no error bar beyond `n = 8`. `Unsure` was used **0 times in
   40** (§5e) — read that as a property of one labeller's confidence, not as evidence the pairs were
   easy.
4. **`n = 8` per bucket** is small. A 25.0% and a 37.5% differ by **one pair**. §5c's rule-1 firing
   rests on a 25 pp gap that is **two pairs** wide and clears the 20 pp bar by **5.0 pp** — **one
   relabel (12.5 pp) unfires the rule**, and the five labels it hangs on are rows **7**, 9, 10, 11 and
   16 of §5e, auditable individually. Row **7** is a † row and is individually decisive (§5c). It is
   reported because
   the rule was pre-registered and fired, not because 8 is a comfortable sample.
5. **One machine** for §2's timings, with 2.7× known variance — and a second session measured the same
   scale-target run at **66.618 s** against this record's **103.878 s** (§2b). Both are on the record;
   neither is averaged. The overrun (**6.7×–10.4×**) is far outside that spread, so the FAIL verdict is
   safe; the `itertools` PASS at 3.5–6.5 s is **not** comfortably safe and should not be quoted as
   headroom.
6. **The `(node, depth)` quadrant result is one crate.** `syn` hits it; the other two do not (§3).
7. **Pre-registration is asserted by this record, not witnessed by the repository** — §0 and §1–§8 land
   in the same uncommitted change (§5c). The `serde_core` misfit and the failed `1.00` dedup
   counter-outcome are evidence against wholesale fitting; they are not proof. Next time: commit §0
   before measuring.
8. **The first version of §5d did not reconcile with its own labels** and was corrected against §5e's
   ledger (§5d). The labels did not change; the aggregate transcription did. It went unnoticed for as
   long as only aggregates were published — which is the argument for §5e existing at all.
#### 10. Ledger after T14

**Discharged by this record:** **N107(1)** (§1) · **N107(2)** (§2, **FAIL**) · **N107(3)** (§3, **D9
trigger HIT**) · **N108** (§0.2, pre-registered before measurement) · **N102(1)–(7)** (§0.3, §5, §6) ·
**N84(c)** (§4, closes affirmatively — counter-outcome refuted) · **N84(a)/N84(e)** (§1/§2, §4) ·
**N86(a)/A1 revisit trigger** (§7 — **not** triggered) · **N78** (§4 — the "no nested clone site" cause
is confirmed as *ours*, and dedup's benefit is now observed outside our fixtures) · **D9's optional
nested-shape sweep** (§8) · **D5-confirm** (§1/§2 for the envelope; §5c/§6 for `0.85` — **partially**:
the owed measurement was finally *run*, and it returns **support at low confidence**, not established
separation. The pre-registered rule **fired** on `n = 8` per bucket from one crate labelled by one
non-maintainer, **one relabel** would unfire it (§9(4)) — and row 7, a † row tallied `No` in the low
bucket, is individually decisive (§5c) — and the rule itself can fire while the retained
bucket is half false positives (§5c); the zero-labelling null is a genuinely clean **0 per kLOC at
≥0.85** (§6) and is the stronger of the two because no human is in its loop. **R2 stands as written.**).

**Open, and now with evidence pulling both ways:** **T13** — §5d says a `30–40` floor removes **64% of
actionable findings** (7 of 11 labelled `Yes` pairs sit below 30 nodes) and separates actionable from
non-actionable by node count only **weakly** (per §6/N118: **weak separation in the desired direction,
bought at that 64% actionable-recall loss** — conceding the separation and rejecting the price; the
arithmetic is recomputed there, and §5d's caveat governs — an overlap observed, not a difference
tested), and N100 reproduces on third-party code at 40 pairs, while §6 says the coincidence null lives **at** the floor
(5 of 6 cross-crate hits at 20–24 nodes) and §1/§2 say the floor is the cheapest lever on a cost that is
**10.4× over budget** (6.7× in the verification session — §2b; over either way). Precision and cost pull in opposite directions on the same knob. **T13 must now
rule between them; T14 does not.** **Superseded at the T14 review (N118): T13 SPLIT — the decision is
**D11** (the human's, blocked on **D10**) and the execution is **T13′**; the two legs are
de-symmetrised (§5d/§6 above) and the pro-floor leg is the weaker one.**

**Still open, unchanged by T14:** **R1** (measured and breached — §2; carried forward) **— superseded at
the T14 review (N112): R1 is CLOSED as a risk and restated as a documented limit; the single-pair tail
folds into R9, and what to ship is **D10**.** · **N84(b)**
(recall — needs known-duplicate ground truth, which no third-party corpus supplies) · **N84(d)**
(test/non-test partition — **not answerable from this corpus**, §7) · **N101** option (c) `--exclude`
(§4b gives it a second, independent motivation — 90.7% of the scale target's output is generated code —
but decides nothing) **— now carried as **D12** (N117), where the YAGNI objection is withdrawn on
measured evidence and the scope is split** · N64/N69/N85/N86(c) · N24/N31/N33/N34/N40 · N46 · N60 (all record-only).

**New, opened by T14 (observations, deciding nothing):**

- **N109 — the scale target's output is dominated by machine-generated code.** 90.7% of `syn`'s findings
  and 93.7% of its exact-`1.00` findings are generated-to-generated (§4b), in files banner-marked *"not
  intended for manual editing"*. Excluding them removes 95.7% of findings and only 58% of the run time
  (§2; ~53% in the verification session, §2b), so it is a **findings-quality** lever, not a performance
  fix. Feeds N101(c) and T13; decides nothing here. **Promoted at the T14 review to **R10** (N113) —
  same standing as R8, and calibration can never fix it.**
- **N110 — F/kLOC is not a constant and cannot be quoted as one.** 10.6 → 43.4 across three ordinary
  crates (§1), a 4.1× spread which the `Θ(F²)` envelope squares into **17×** in predicted cost. Any
  user-facing restatement of the envelope in LOC must be a **range**, and must say which end a
  generated-code-heavy crate sits at. Also: `serde_core` is a **misfit** against its own N108 criterion
  (chosen for many small `impl` blocks, it is the *least* fragment-dense of the three) — kept and
  reported per §0.2's honesty clause, not swapped. **Promoted at the T14 review to **A10** (N114), and
  it is the drift `docs/design.md`'s Performance section carried (N121).**
- **N111 — a single TED evaluation *can* approach or exceed the entire budget, session-dependently.**
  One 1 043-node nested fragment cost **17.5 s** in this record's session — 1.7× the 10 s budget, for
  **one pair** — and **7.6 s**, under it, in the verification repeat (§8). So the breach is **observed
  and not established**: on this box the same single pair straddles the budget line. The growth is
  steeply super-quadratic and **consistent with** D5's `O(n²·d²)` or worse; two noisy points cannot
  place it at or beyond that curve, and this note does not claim they do. R9 and D9 both already say
  the per-pair cost is shape-dominated; this prices it against the budget for the first time. The
  2 000-node ceiling is what stands between that curve and an unbounded run.
  **Not given a risk row of its own (N112): this folds into R9's status line**, which is where
  shape-dominated per-pair cost already lives.

### T14 review notes (Anders — **APPROVE**, with notes)

Anders approved T14 as committed. **No code, test or measurement was changed in this round** — it is a
register/documentation round only. Each note below is his ruling, recorded and attributed to him.

- **N112 (Anders) — R1 splits: the measured envelope CLOSES, the tail moves to R9.**
  R1's measured-envelope half is **closed as a risk**: `Θ(F²)` was reproduced, F/kLOC was measured on
  three ordinary crates, the budget was stated by the human, and the breach was measured **twice** in
  independent sessions. A quantity measured that thoroughly is no longer an uncertainty — **it is a
  documented limit, i.e. a specification**. R1's row is rewritten to say exactly that and to point at
  **T14 record §2** and **A10** rather than restate them. **N111's tail belongs to R9, not R1, and gets
  no row of its own**: R9's status line now also carries that one **1 043-node nested fragment cost
  7.614–17.478 s for a single TED evaluation on a single pair**, straddling the whole 10 s budget.
  What to *do* about the documented limit is **D10**, a product decision, not a risk.
- **N113 (Anders) — R10 opened: non-actionable-by-construction findings dominate generated-code-heavy
  targets.** Promoted out of the T14 ledger (N109) into the risk register **beside R8, at the same
  standing**. **90.7%** of `syn`'s 6 616 findings and **93.7%** of its 3 432 exact-`1.00` findings are
  generated↔generated. The class is **unreachable by threshold, by floor, and by the label model** —
  the duplication is real and the tool is correct; **the generator wrote it**. The consequence Anders
  wanted on the record: **calibration can never fix a finding that is non-actionable by construction**,
  so no `--threshold` or `min_nodes` number may be quoted as a mitigation for it. Mitigation is **path
  exclusion (D12)** plus a docs recipe.
- **N114 (Anders) — A10 opened: the envelope has no per-LOC constant.** Promoted from N110. Measured
  **10.56 / 27.05 / 43.38 F/kLOC** across three ordinary crates — a **4.11×** spread that `Θ(F²)`
  squares into **~17×** in predicted cost. Binding consequences: **every user-facing runtime statement
  must be a range**, and it must say that **generated-code density** is what places a crate within that
  range. This is also the drift `docs/design.md` carried and N121 repairs.
- **N115 (Anders) — D10 opened: the v1 performance position. Human decision.** Options as framed:
  **(a)** ship with a documented limit; **(b)** one admissible optimisation slice (**S4 = T15–T18**).
  **Anders' recommendation: (b)**, with **the 10 s budget left exactly where the human set it.** His
  reasoning as recorded: **51.8 kLOC is a mid-size library**; extrapolating `Θ(F²)` to a **200 kLOC**
  workspace at *middle* density gives ~**2.4×** F ⇒ ~**5.8×** pairs ⇒ **6–10 minutes**. That figure is
  **an extrapolation, not a measurement**, and is marked as one wherever it appears — but it makes
  "documented limit, ship it" read as **unusable on the codebases most in need of a duplicate
  detector**. **D8 already licenses this work** ("changes only speed, never results"), so S4 needs
  sequencing, not new architectural permission.
- **N116 (Anders) — D11 opened: the default `min_nodes` for v1. Human decision, blocked on D10.** The
  evidence is **complete** — §5d, §6 and §1/§2 — and **more labelling at n = 8 will not move it**.
  **Anders' recommendation: hold at 20 for v1**, on the principle that **you pay a cost problem with
  cost levers, not with a semantics lever**; `--min-nodes` is already exposed for users on large crates.
  Blocked on D10 because an admissible speed-up removes the cost argument entirely.
- **N117 (Anders) — D12 opened: does `--exclude <glob>` enter v1 scope? Human decision. Anders withdraws
  his own N101(c) YAGNI objection.** Why: **YAGNI forbids building for a *speculated* need; this one is
  *measured*** (R10 / §4b). His scope split, recorded exactly: **(1) `--exclude <glob>`: IN** — lives
  entirely in the `discovery` adapter, `ignore` already ships `OverrideBuilder`, so **no new dependency,
  no core change, no score change, no output-format change, no R3 exposure**, and determinism holds over
  the `/`-normalized paths; **(2) `#[cfg(test)]` skipping: OUT, stays deferred** — different layer
  (**parse**, not discovery) and **N84(d) has no third-party measurement behind it**; do not bundle an
  unmeasured feature with a measured one; **(3) auto-detecting `@generated` banners: REJECTED for v1** —
  a heuristic over a comment convention, ecosystem-dependent, and a silent drop rule. He also **disproves
  the "a workaround already exists" argument** rather than doubting it: **§2's own mitigation command
  lists 48 files on a command line**, which is evidence the current surface is inadequate. Riders: it is
  a **findings-quality lever, not a perf fix**; it ships with a **docs recipe**; and excluding `gen/**`
  moves `syn` from the **43** end of A10's range toward the **~11** end, making A10's range *more*
  useful.
- **N118 (Anders) — T13 splits into D11 (the decision) + T13′ (the execution), and §6's conflict is
  DE-SYMMETRISED.** This last part is a substantive correction to how **T14 record §6** currently reads.
  The two sides of the "precision vs cost" conflict **are not measuring the same population**:
  - **§5d's 7-of-11** is about **actionable findings a user loses** — real cost to a real user;
  - **§6's 5-of-6** is about **cross-crate coincidences between unrelated crates**, a population
    **users never encounter** in a real run;
  - and the two populations **overlap heavily** by node count, so the separation the floor buys on the
    population that matters is **weak and paid for in recall**. **The arithmetic is not restated here.**
    The corrected ledger, the retained shares, the actionable-share movement under both denominator
    conventions, the recall cost, the bar on comparing these figures against the retracted aggregate,
    and §5d's governing caveat are derived **once**, at **§6** — cite it; do not re-derive it.

  **Anders' note as delivered carried the defective figures** — the aggregate §5d retracts. On the true
  range its **sub-argument inverts** while its **main argument holds**: the population-based
  de-symmetrisation is **independent of the retracted figures** and survives **unchanged and on its own
  footing**; the correction does **not** strengthen it. See **§6** for the derivation and the figures.

  **Therefore it is not a genuine tie: the pro-floor leg is the weaker one.** Anders also names the
  reason the two axes look coupled at all: **cost and precision are coupled only because there is one
  knob.** `min_nodes` looks like a cost lever only because it is currently the *sole* cost lever — that
  is a **knob shortage, not evidence**. **If S4 delivers an admissible 10×, the cost argument for
  raising the floor evaporates entirely.** **T13′** carries N95's consequences **conditionally**: at ≥30
  the closure population goes to ~zero and free `{}` blocks are already 0, so **D7 becomes a cleanup and
  A1 may need amending** — but **if D11 holds at 20, T13′ is a no-op and D7/A1 are untouched.**
- **N119 (Anders) — D9: trigger MET; does not enter v1 on this evidence; RE-OPENS as a decision blocked
  on T15(c).** The trigger is **MET** (9 fragments, depth 35, real code). It **does not enter v1 on this
  evidence, and does not stay quietly deferred either**. Of D9's three original objections: *"depth not
  computed"* is **RESOLVED** (`NormTree::depth()` exists as an unrendered statistic, so the instrument
  is free); *"a second invisible, untunable drop rule with its own diagnostic"* **stands**; *"one
  adversarial fixture does not justify it"* is **partially answered** — real code reaches the quadrant,
  but at **9 of 2 245 (0.4%) on one crate, 0 of ~550 on the other two**, at depth **35** against
  synthetic **85**. **The decisive gap is that occupancy is not cost.** Nothing on the record says those
  9 fragments cost anything measurable. Per-pair cost is `n₁·n₂·min(d,l)₁·min(d,l)₂`, and the 9 are
  mutually size-compatible enough to survive the pre-filter, generating **up to 36 pairs among
  themselves**. **If that handful is ~20 s of the 104 s**, a shape-aware ceiling is simultaneously D9's
  answer **and** a 20% perf win. **If it is 0.5 s, D9 stays deferred and we have learned something more
  important — the cost is *broad*** (521 365 ordinary evaluations), reachable only by memoisation,
  pruning or parallelism. **Structural point, true regardless of the number: a shape-aware ceiling is
  NOT admissible under D8** — it **drops fragments**, so it changes results and inherits the **D5/T13
  evidentiary standard**. We have three crates and **only one occupies the quadrant**; setting a
  user-visible drop rule from a one-crate basis is the exact error D9's own text warns against. Even if
  T15(c) is attractive, the number justifies a **targeted optimisation, not a drop rule.**
- **N120 (Anders) — S4 opened: T15–T18, priced, ordered, and gated.** All four are **blocked on D10**;
  T16–T18 are additionally **gated on T15**, and each carries **T9's negative control** (dogfood **and**
  acceptance output **byte-identical**). Anders' priced lever table and his ordering rationale:

  | lever | task | expected win | standing | why it sits where it does |
  |---|---|---|---|---|
  | structural-hash memo | **T16** | potentially the whole 10× | **strongest hypothesis, and unmeasured** | equivalence classes, not short-circuits (below) |
  | label-multiset lower bound | **T17** | **unknown yield** | **same standing as T11** — provably never prunes a real match, same prune-soundness unit test | the size-ratio filter already killed **79%** of pairs and `syn`'s survivors genuinely *are* similar |
  | `rayon` over the pair loop | **T18** | **6–12×**, reliably | it **always works** | **deliberately last** — the only lever that **touches architecture** (a third-party crate near core), and a parallel 10× would **mask whether the algorithmic work paid** |

  **The memo is explicitly NOT the exact-`1.00` short-circuit.** Skipping identical-hash pairs would
  skip only **3 432 of 521 365** evaluations — **0.7%, worthless alone**. The win is **equivalence
  classes**: class A of `k` members against class B of `m` members is `k·m` evaluations of **one**
  computation, collapsing to **1** under a `(hash_left, hash_right)` memo — pure, **std-only
  `HashMap`**, **no dependency**, and **determinism untouched, since a cache never orders output**.
  **Target arithmetic, stated so nobody mistakes the scale of the job: 103.9 s / 521 365 evaluations
  ≈ 200 µs per TED evaluation, and we need ~10× — this is not a micro-optimisation.** **Gate:** re-run
  the wall-clock test at the same pins; **the 10 s budget does not move.** Landing at 15 s is **a new
  honest number and a new human decision**, not a re-based budget.
- **N121 (Anders) — `docs/design.md` carried real drift, now repaired.** The Performance section stated
  the envelope **purely in F, with no LOC bridge**, which **A10 invalidates**. The doc pass adds the
  measured **10.6–43.4 F/kLOC** range, states that **generated-code density** is what places a crate
  within it, and makes explicit that any user-facing runtime statement is a **range**. **No threshold
  number entered `docs/design.md`** — that invariant holds. This is the only `design.md` edit in this
  round.
- **N122 (Anders) — two durable conventions moved out of the feature file, per golden rule #10.**
  §9(7)'s **"commit the pre-registration before measuring"** and §0.2's **honesty clause** (a corpus
  chosen against a declared criterion is **kept and its misfit reported**, never swapped for a
  better-looking one) are **durable process conventions**, not T14 facts, and would be lost by the next
  calibration task if they stayed in T14's §9. Recorded — with the related standing item that **D11's
  successor decision rule needs an absolute-precision term, set by the human *before* the next
  labelling** — in **`.github/agent-roles/orchestrator.md`** (section *Measurement-task conventions
  (durable)*), which is the role that owns commit sequencing and the pause-for-human decisions.
- **N123 (Anders) — explicitly NOT done in this round, so nobody re-litigates it later.**
  (1) **T14 §9(7) is not retrofitted** — Anders was explicit: **commit as-is**; the pre-registration
  chronology stays asserted-not-witnessed and the fix belongs to the *next* measurement task.
  (2) **The pre-registered decision rule is not amended** — its successor, with an absolute-precision
  term, is **reserved to the human and must be set before the next labelling** (§5c). Amending it now
  would be the fitting error N102(3) exists to prevent.
  (3) **No risk row was created for N111** (folded into **R9** per N112) and **no number was invented
  for N84(d)**, which this corpus still cannot answer.