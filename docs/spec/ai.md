# AI: the learned-policy seat, its training plan, and its runtime

Status: spec section, decided at #9 (2026-09-11); its constants appendix is `ai-constants.md`, accepted
at #16 and merged here **by reference**. Merged into the assembled spec by #12. The strike declaration it
commits is `physics.md` §4; the legality mask comes from `rules.md` §2; the crate and the serve path are
`architecture.md` §1/§7.

## Scope and sources

One policy plays a seat of the game exactly as the rules layer models it. The design spine is the Rust ML
survey on #4 — **train in Python, serve in Rust through ONNX/`ort`; geometry proposes, policy disposes;
end-to-end aiming fails on full games** — and every measured number lives in the appendix with its
provenance, hardware, and *measured* / *derived* / *provisional* label. Rejected alternatives are named
in §10 where the choice was close.

## 1. Decision pipeline

One atomic declaration `{call, aim, cue speed, spin offsets, elevation}` per shot, produced by a
**macro/micro** split that separates intent from execution:

1. **Generate.** Recursive mirrored-table / ghost-ball constructions over a typed candidate set (§3),
   line-of-sight pruned, deduped by predicted contact point.
2. **Verify.** The top-K shortlist is executed in the exact sim: true makeability and the resulting leave
   are measured and feed the value estimate. Analytic geometry is an estimate; the sim is the truth —
   the appendix's candidate dump shows the analytic seed score is *not* makeability (its top-ranked
   candidate pots in all three positions while ranks 1–5 miss in the easy-pot position), which is what
   earns the verification pass its work units.
3. **Select.** The macro scores candidates with a masked categorical over the typed set (§4) — pot or
   safety — yielding `(target ball, pocket, leave point)`.
4. **Refine.** The analytic micro seeds the strike and refines it against the sim until the intended
   contact and leave are realized.
5. **Commit.** The declaration is logged like a human's input; the rules layer adjudicates as usual.

**Budget: a per-decision work-unit cap with anytime commitment.** The cap counts sim evaluations /
candidates scored — never wall clock, so replay stays a pure function of rack + input log + weights +
seed. At the cap, the best-scoring declaration found so far is played: a truncated shortlist still yields
a legal shot, and a stalled refinement degrades to the analytic seed. The hard 1 s wall SLO (typical
< 100 ms) is enforced by calibrating the cap. **`B` = 16–24 work units, pinned**; the cap is the only
budget unit in the design.

## 2. Observation and action

- **Observation: fixed ball slots + context vector.** 16 canonical slots (cue, 1–15) × (x, y, class)
  normalized to the table frame, plus scalars — target/group state, on-8 flag, group counts, case (break
  / play), ball-in-hand domain, scores. Continuous and exact: the precedent requires 0.01° aim precision,
  so no rasterization and no quantization anywhere.
- **Action: full parity with the human.** The AI may declare any strike the human can, including
  elevation (masse) and jump; the miscue envelope is enforced at the input boundary for both, in the
  envelope-fraction unit of `physics.md` §4, so the policy's validation reads `|(a, b)| ≤ 1`.
- **Legality masking is not learned.** Target group, the open-table claim, the 8's special states, and
  the break's missing call come from the rules layer and mask the candidate set.

## 3. Candidate set

- **Typed candidates.** *Pot* candidates carry a `{ball, pocket}` call plus a strike intent; *safety*
  candidates carry the `safety` call and come from intent templates — minimal-contact escape, roll-up /
  leave-behind, two-way shot. The policy chooses across the typed set, so pot-vs-safety is one masked
  decision and the rules layer receives a declaration it already models.
- **Coverage.** Direct pots, multi-rail banks and kicks, combinations, safeties — **`R` ≤ 3 cushion
  contacts and `C` ≤ 1 intermediate ball, pinned with an unexercised caveat**: the prototype measures the
  rail families as cheap and producible (0.673 ms for a 15-ball depth-3 set; forced success 0.625 banks,
  0.229 kicks, 0.875 combinations) but their **selection share was 0 %** against 78 % direct / 22 %
  safety, so nothing yet shows the caps *bind*. **The first league run must report the rail families'
  selection share on real rack positions**; that report, not the prototype, justifies keeping `R` = 3.
- **Constructions are geometric**: mirror the pocket across the cushion chain (banks), mirror the ghost
  point (kicks), chain ghost points (combinations). Pruning is analytic (clearance, dedup by contact
  point); verification is the sim's job.

## 4. Policy and value

- **Per-candidate scoring + masked softmax.** One shared encoder over (state context, candidate
  features) produces a logit per candidate; the policy is a categorical over the candidate set, masked by
  legality. Set size is unbounded; ONNX dynamic axes carry the variable length.
- **Value head** on the same trunk: warm-started from the scripted planner's mobility-style leave score —
  which must exist anyway as the fallback and the league anchor — then trained on PPO returns.
  Sim-in-the-loop rollout value (PickPocket-style search) stays a documented extension if the head
  underfits.
- **Macro output `(target ball, pocket, leave point)`**: a continuous desired cue-ball position chosen so
  the next shot is easy; the value function scores the actual rest state, so an unreached leave costs
  value rather than breaking a constraint.

## 5. Reward

Adjudication-driven dense + terminal: + for the called ball legally pocketed, − graded for fouls, − for
the 4.8 losing conditions, ± terminal win/loss (one rack = one episode). Safety quality is not a reward
term — it is credited through the value of the resulting state. Normalized and clipped; the documented
upgrade path is potential-based position shaping if PPO plateaus. Break-specific terms live in the break
drill (§6).

**One structural constraint is pinned from a measured failure**, and it outlives the provisional weights:
a **terminal, per-ball reward must dominate the per-shot legal-hit term** (the first shaping collapsed
into a bank-only safe game after ~8k steps; potting 2.0 vs legal-hit 0.05 fixed it), and the reward stays
adjudication-derived. The magnitudes are re-tuned on the real env.

## 6. Training plan

- **Train/serve split:** PyTorch + SB3 / sb3-contrib Maskable PPO in Python; export ONNX at a pinned
  opset; serve with `ort` v2 (CPU, one intra-op thread, version pinned); `tract` is the pure-Rust escape
  hatch; libtorch never ships. Pins and the measured cost envelope: §9 and `ai-constants.md`.
- **Env: shot-granularity over a headless CLI/pipe** — step = rest state + context in, declaration out,
  sim to rest, adjudication record back. IPC is noise next to physics, and the same binary is #11's
  headless harness. **The shipped env is not the prototype env**: the prototype's is pure Python at 1,439
  steps/s; the shipped one drives `pool-headless`, where one decision is 0.10–0.14 ms of Rust. No
  throughput number in the appendix is a shipped-env prediction.
- **Episodes and league:** one rack per episode, shared net for both players (shooter's perspective),
  per-rack terminal; the race-to-5 match is eval-only. Opponents: rolling window of the last N snapshots,
  uniform sampling, scripted planner always in the mix. `M`/`N` keeps the **sizing rule**
  (`M·N·shots_per_rack·(decision + shot)`), with `M·N` = 8–16 the working envelope — set from the real
  sim's per-shot cost and a league round's wall clock, reported by the first league run.
- **Regime: imitation → drills → self-play.** Warm start by cloning the scripted planner's rankings
  (non-break shots), then named drills with seeded procedural scenarios and per-drill gates — break,
  N-ball open table, group clearance with clusters, 8-ball endgame, safety/escape duel, ball-in-hand
  finisher — then self-play against the league.
- **Break:** break-shaped macro `(rack contact point, speed, spin, elevation)` + the in-hand placement,
  learned from scratch in a dedicated break drill (thousands of breaks; reward = ≥ 4 balls to rails, no
  scratch, spread value), then refined in self-play. The scripted fallback keeps its own break recipe for
  unavailable-policy play.
- **Compute envelope:** the dev's Apple silicon (CPU-scale; MPS only if the net grows), sized to
  hours–a day per ladder rung from the measured throughput.
- **Checkpoint selection rule (replaces fixed fractions).** A checkpoint ships only if the fixed eval
  shows **monotone skill against the previous checkpoint**; how many are taken and where is an
  implementation parameter. The fractions 10 / 50 / 100 % are **dropped from the spec**: a 150k-step run
  degraded after ~30k (6.899 / 4.405 / 3.324 on a fixed eval) while a 32k run of the same configuration
  was monotone (6.770 / 6.972 / 7.104).

## 7. Difficulty: one policy, four levels

| Level | Checkpoint | σ |
|---|---|---|
| Beginner | early (≈ 10 % of the schedule — *illustrative*: the monotone-skill rule of §6 decides which checkpoints ship) | high |
| Intermediate | mid (≈ 50 %, illustrative) | medium |
| Advanced | final (100 %) | low |
| Pro | final | near-zero |

σ is a per-parameter correlated Gaussian (aim, cue speed, spin a/b, elevation) applied to the committed
declaration by the shell's seeded RNG — the same execution-noise model training uses for robustness. The
**aim σ curve is measured** (400 trials/point: 0 → 1.000, 0.5 mrad → 0.973, 1 → 0.932, 2 → 0.830,
4 → 0.660, 8 → 0.458, 16 → 0.295) and the level assignment is derived from it: Pro 0.5 mrad, Advanced
1–2, Intermediate 4, Beginner 8–16. **σ for speed, spin and elevation is unmeasured** — the shape stays
(one per-level factor scales all four axes, speed at 2.5 × aim) but the ratio is an assumption, not a
measurement; the settling measurement is the same 400-trial protocol on the real sim, one axis at a time.
Checkpoints ship only if eval shows monotone skill; a candidate-sampling temperature knob is documented
but off by default.

## 8. Runtime integration

- **Policy behind a trait, in a dedicated crate** (`pool-ai`, `architecture.md` §1): `OnnxPolicy`,
  `ScriptedPolicy` (fallback), `NullPolicy` (tests). The crate depends on sim + rules types and never on
  Bevy; encoding, candidates and the micro are ORT-free so they test without the runtime; ONNX loads once
  at startup with a hash check.
- **Sync decision at turn start.** `AwaitingShot`, `AwaitingPlacement` and `AwaitingChoice` are resolved
  immediately when they belong to the AI; the decision is logged as an input (replay = initial rack +
  input log).
- **Placement** (domains `Anywhere` / `AboveHeadString`) is scored by the same macro/micro loop —
  candidate placements evaluated by the best achievable shot from each — and the 1.6 ¶2 spot request is
  evaluated as an alternative successor state.
- **Choice states:** value-based successor comparison, with a cheap deterministic roll-forward for the
  re-rack branches; **stalemate: accept, never propose**.
- **Fallback:** any unavailability (missing/corrupt `.onnx`, `ort` load failure) → the scripted planner
  plays; the failure is logged and the UI marks degraded mode.
- **Debug view:** the candidate set + per-candidate scores is dumpable and viewable behind
  `--debug-candidates` (`architecture.md` §10).

## 9. Runtime stack, pins, and the artifact contract

- **Rust side:** `ort` v2 behind `pool-ai`'s non-default `ort` feature, CPU execution provider, **one
  intra-op thread**, version pinned with its opset; the app enables it, the default workspace build does
  not. `tract` is the pure-Rust escape hatch; libtorch is never shipped (tch-rs would pin libtorch
  v2.13.0 and add ~267 MB CPU-only — ~1.2 GB with CUDA — to a desktop game). Burn and Candle were
  surveyed and are not carried: Burn's ONNX import is build-time codegen over a subset of operators, and
  Candle buys nothing over `ort` for this policy.
- **Python side (pins move here from the throwaway prototype):** torch 2.14.0, stable-baselines3 2.9.0,
  sb3-contrib 2.9.0, gymnasium 1.3.0, onnx 1.22.0, onnxruntime 1.30.0, numpy 2.5.3. Training code lives
  in `training/` with its own pinned deps and is never built by Cargo.
- **Artifact identity rule.** The shipped model is `assets/policies/*.onnx` + `.sha256`, and
  **`difficulty.checkpoint` in the input-log header identifies it by that sha256** — the golden vectors
  are a *contract on that hash*, not a suggestion. Load happens once at startup (measured 29.2 ms),
  with the hash checked before use.
- **The measured serve path** (appendix §2.4): 32-candidate inference p50 0.016 ms / p99 0.022 ms,
  +0.015 ms to a 0.098 ms fallback decision; static-link binary 696,784 B → 26,552,480 B (+25.86 MB);
  ORT-vs-Python golden max abs error 4.77e−7, and the stronger end-to-end check — the Rust ORT masked
  argmax agreeing with the SB3 policy on **83/83 shots** across 20 live episodes. Keep the policy graph
  to common operators (Linear/ReLU/Tanh/Softmax) so `ort`, `tract` and any future import path all accept
  it.
- **Determinism.** Policy numerics are outside the bit-identity contract (`architecture.md` §11): replay
  rests on the sim, the rules layer, and the logged inputs; per-machine reproducibility is best-effort
  under the pinned single-threaded ORT. Inference never runs inside a shot's computation.

## 10. Evaluation bar

- Offline eval script: named per-shot suites (pots, banks, kicks, safeties, escapes, break legality) with
  target rates; ≥ 100 seeded racks vs the scripted anchor; decision-latency p99 against the 1 s SLO.
- **Pinned gates:** the **≥ 95 % easy-pot suite** is attainable and gated (the scripted anchor scores
  40/40 = 1.000 with no noise, 0.973 at the Pro σ); **zero illegal declarations** is a hard gate; the
  **≤ 1 s per decision** SLO holds at ≥ 100× the measured unit cost (the toy sim's 2.48 µs/shot and 9.2
  events/shot are the floor; the margin is derived, not measured).
- **Milestone gate (post-league), with its dependency named:**

  > **Rack gate.** The Pro-difficulty policy seat wins **≥ 60 % of racks** against the scripted planner
  > seat over a seeded rack suite, with both seats on the same rack seeds and the same break policy.
  > Reported with the suite size, the seed range, and the per-rack adjudication summary.
  >
  > **Blocked by:** the rules layer and the opponent seat (#6, `architecture.md` §8). Until both exist
  > the gate is **not reported** — no proxy number is substituted for it.

  The 3-ball clearance drill ≥ 0.95 (measured 0.967) is a **smoke** gate for the drill environment only
  and is never reported as the rack gate. The zero-illegal-declaration gate gets the same treatment: the
  prototype's mask is all-true by construction, so its measured first-contact rates (1.000 direct, 1.000
  bank, 0.833 kick, 1.000 combination, 1.000 safety escape) are a smoke substitute, and the real mask
  comes from the rules layer.
- Final acceptance: the human plays it and calls it plausible. CI runs only the cheap parts — ONNX load,
  golden vectors, and a legality smoke on a fixed small set (§11).

## 11. Re-pin obligations

Every number in `ai-constants.md` is re-pinned against the real sim at M1–M4. The four that gate the SLO
and the cap: **the real sim's per-shot cost, its event count, the real candidate-verification cost at
`B`, and the real 15-ball generation cost.** `B`, the SLO margin and the σ set are re-derived from those
four. Two more obligations ride with the appendix: the **candidate feature rows** get the same
cross-language fixture check the observation layout got before the first league run, and every row keeps
its *measured* / *derived* / *provisional* label with its provenance attached.

## 12. Handoffs

- **#11 (architecture):** `architecture.md` fixes the workspace, the pipe payload, the ORT feature
  boundary, and the `ai_host` worker; this section fixes the interface requirements only.
- **#10 (cue UX):** the strike declaration is the shared action space; the miscue envelope bounds both,
  in the same unit.
- **#8 (physics):** the prototype numbers get re-pinned against the real sim (§11); the rail-family
  selection share is the report that justifies `R` = 3.
- **#14 (racks):** seeded racks feed the drills and the break drill, and AI training/evaluation seed
  ranges stay disjoint from the fixture seeds (`rules-break.md` §2.9).
- **Build order (for `README.md` §5's M4):** generator + scripted planner → serve path → env bridge →
  imitation → drills → self-play.
- **Out of scope:** the difficulty-selection menu and think-beat UX are polish-stub lines, not spec
  decisions.
