# AI pipeline: measured constants and calibration

Status: **prototype measurements for wayfinder #16**, feeding the AI section of the spec (the
resolution on #9). Every number here is indicative and gets re-pinned against the real simulation
(#8) at implementation: the prototype's table model is spec frame/geometry constants only, with no
fitted physics, no rules layer, and no shipping code anywhere.

- Prototype: `prototypes/ai-spine/` (Rust generator/planner/toy sim/`ort` serve path; Python env and
  PPO smoke run). Commands and toolchain: `prototypes/ai-spine/README.md`.
- Raw records: `prototypes/ai-spine/results/*.json` (throughput, per-decision cost, eval
  calibration, serve latency, candidate dump, ONNX golden vectors).
- Hardware for **every** measurement in this document: **Apple M5, 10 cores, 16 GB RAM, macOS
  26.4.1 (aarch64)**, release build, single process, one `ort` intra-op thread.
- **Measurement conditions, stated plainly:** this worktree ran beside two other agent workloads on
  the same machine for the whole session, so the 1-minute load average sat between 13 and 48 on 10
  cores while the runs were taken. Every published record is the **fastest of 5–6 runs**; the same
  cells varied by up to ~2.3× across those runs. Treat every timing below as an **upper bound**, and
  note that the SLO conclusion survives that pessimism (the worst p99 recorded under 5× CPU
  oversubscription is still ~60× inside the 1 s budget). Nothing here is a clean-room benchmark.

Labels: **measured** = read directly off a run recorded in `results/`; **derived** = arithmetic on
measured values with the rule stated; **provisional** = not measured here, carried from #9 or
needing measurement that is out of the prototype's scope.

## 1. Calibration table (#9 §10, filled)

| Constant | Provisional | Pinned by this prototype | Label |
|---|---|---|---|
| Work-unit budget `B` (sim evals / decision) | — | **4–8** for quality (executed pot rate saturates: 0.63 → 0.93 (3-ball) and 0.70 → 0.88 (6-ball) at 4 evals, flat to 64); **16–24** recommended for the spec, bounded by the SLO at the real sim's per-shot cost | derived (quality) / provisional (real-sim bound) |
| `R` / `C` (cushion contacts / intermediate balls) | 3 / 1 | Kept. Measured: rails cost 0.89 ms vs 0.05 ms per 15-ball generation at depth 3 vs 0 — cheap; forced success 0.625 (banks), 0.229 (kicks), 0.875 (combinations); **selection share 0 %** in the prototype's drills (direct pots and safeties always outscore them) | measured (cost, success, share) / provisional (that the caps bind on real racks) |
| σ magnitudes per level (aim) | — | Measured easy-pot make rate vs aim σ (400 trials per point): 0 → 1.000, 0.5 mrad → 0.973, 1 mrad → 0.932, 2 mrad → 0.830, 4 mrad → 0.660, 8 mrad → 0.458, 16 mrad → 0.295. Level assignment **derived** from that curve: Pro 0.5 mrad (≥ 0.97), Advanced 1–2 mrad (0.83–0.93), Intermediate 4 mrad (0.66), Beginner 8–16 mrad (0.30–0.46) | measured (curve) / derived (assignment) |
| σ for speed / spin a,b / elevation | — | Speed σ assumed at 2.5 × aim σ; **never measured** — the toy model has no spin or elevation to perturb | provisional |
| Checkpoint fractions | 10 / 50 / 100 % | **Not pinnable from this prototype.** A 150k-step smoke run evaluated at those fractions degrades (mean return 6.899 / 4.405 / 3.324 on a fixed 100-episode eval); a 32k-step run of the same configuration is monotone (6.770 / 6.972 / 7.104). The fractions depend on where the real schedule plateaus, which nothing here establishes | provisional |
| League `M` / `N` | order 8–16 | Sizing rule from measured throughput: a league round costs `M·N·shots_per_rack·(decision + shot)`. At the measured 0.27 ms decision and 8 shots/rack, `M·N = 16` is ~35 ms of CPU in the prototype; at the measured Python env throughput (1,209 steps/s) the same round is ~0.1 s. The binding term is the **real** sim's per-shot cost and whatever fraction of the training budget a round may take — `M·N` = 8–16 remains plausible, but it is an envelope argument, not a measurement | derived |
| Net sizes, PPO hyperparameters | — | Smoke net: cand-branch `16→64→64`, ctx-branch `64→64→64`, combine `128→64→1`, value `64→64→1`; **30,210 params**, ONNX 91,668 B; PPO hyperparameters and throughput from §4 | measured |
| Reward weights and normalization | — | **Not pinned, and shown to matter:** the drill reward needed one rebalance before PPO learned at all (the first shaping let the policy collapse into a bank-only safe game after ~8k steps; potting 2.0 vs legal-hit 0.05 fixed it). The spec's adjudication-driven reward (§9 §5) is a different object. Measured return range: mean 7.13, sd 0.34, 0.89 per shot | provisional |
| Eval suite seeds and gate thresholds | provisional in #9 §9 | Seeds and measured anchor rates in §3; the `≥95 %` easy-pot gate is **attainable** — the scripted anchor scores 40/40 = 1.000 at zero noise and 0.973 at the Pro σ | measured (anchor) / derived (thresholds) |

## 2. Measurements (the ticket's list)

### 2.1 Candidate generation throughput

Per **decision's** candidate set for one position, mean over 12 seeded positions per cell, depth =
maximum rail contacts in the constructions (`depth 0` = direct + combination + safety only), release.

| Balls | Rail depth | Generation | Constructions/s | Candidates kept (after LOS) | After dedup | Banks kept (1/2/3 rails) | Kicks kept (1/2/3 rails) |
|---|---|---|---|---|---|---|---|
| 3 | 0 | 0.008 ms | 8.8 M | 22 | 22 | — | — |
| 3 | 1 | 0.026 ms | 8.1 M | 56 | 55 | 12 / — / — | 21 / — / — |
| 3 | 2 | 0.078 ms | 8.3 M | 119 | 117 | 12 / 18 / — | 21 / 44 / — |
| 3 | 3 | 0.189 ms | 10.3 M | 168 | 165 | 12 / 18 / 13 | 21 / 44 / 35 |
| 6 | 0 | 0.010 ms | 24.0 M | 29 | 29 | — | — |
| 6 | 1 | 0.042 ms | 12.2 M | 72 | 72 | 13 / — / — | 29 / — / — |
| 6 | 2 | 0.137 ms | 10.1 M | 148 | 146 | 13 / 20 / — | 29 / 55 / — |
| 6 | 3 | 0.310 ms | 12.8 M | 200 | 197 | 13 / 20 / 12 | 29 / 55 / 39 |
| 15 | 0 | 0.038 ms | 36.1 M | 42 | 42 | — | — |
| 15 | 1 | 0.106 ms | 19.7 M | 74 | 74 | 8 / — / — | 23 / — / — |
| 15 | 2 | 0.308 ms | 13.8 M | 118 | 117 | 8 / 8 / — | 23 / 36 / — |
| 15 | 3 | 0.744 ms | 14.4 M | 142 | 140 | 8 / 8 / 2 | 23 / 36 / 20 |

Reading: the rail families cost ~20× the direct-only generation at full rack (0.744 ms vs 0.038 ms)
and are still under a millisecond; a crowded table prunes hard (banks 42 → 20 kept at 6 balls;
20 → 11 at 15 balls) because line-of-sight kills multi-leg paths first. Per-class offered rates at
6 balls (untruncated, 60 positions): direct 5.9, bank 42.5, kick 109.8, combination 6.0, safety
escape 7.9, roll-up 3.8, two-way 1.0 candidates per position.

**Top-K shortlist cost.** Sort + truncate to K = 6: mean **0.18 µs**, p50 0.17 µs, p99 0.42 µs — free
next to a sim evaluation (a sim shot is ~3.5 µs).

### 2.2 Sim verification + micro refinement per decision; the 1 s SLO

Per decision, 60 (3-ball) / 40 (6-ball) seeded drill positions per row, scripted planner.
"Executed pot" is the committed declaration re-simulated noise-free — the quality that cap buys.

| Balls | `B` (cap) | Total mean | Total p99 | Generate | Verify | Micro | Sim evals used | Executed pot |
|---|---|---|---|---|---|---|---|---|
| 3 | 0 | 0.157 ms | 0.201 | 0.156 | 0 | 0 | 0 | 0.633 |
| 3 | 4 | 0.156 ms | 0.164 | 0.148 | 0.007 | 0 | 4 | **0.933** |
| 3 | 8 | 0.165 ms | 0.181 | 0.150 | 0.010 | 0.003 | 8 | 0.933 |
| 3 | 16 | 0.172 ms | 0.184 | 0.148 | 0.009 | 0.013 | 16 | 0.933 |
| 3 | 24 | 0.180 ms | 0.200 | 0.148 | 0.010 | 0.021 | 21 | 0.933 |
| 3 | 64 | 0.176 ms | 0.185 | 0.147 | 0.009 | 0.019 | 21 | 0.933 |
| 6 | 0 | 0.282 ms | 0.317 | 0.281 | 0 | 0 | 0 | 0.700 |
| 6 | 4 | 0.278 ms | 0.310 | 0.265 | 0.012 | 0 | 4 | **0.875** |
| 6 | 8 | 0.287 ms | 0.306 | 0.262 | 0.018 | 0.006 | 8 | 0.875 |
| 6 | 16 | 0.305 ms | 0.340 | 0.263 | 0.017 | 0.024 | 16 | 0.875 |
| 6 | 24 | 0.321 ms | 0.345 | 0.264 | 0.018 | 0.037 | 21 | 0.875 |
| 6 | 64 | 0.318 ms | 0.346 | 0.263 | 0.017 | 0.037 | 21 | 0.875 |

- **The SLO holds with a very large margin on this machine**: worst p99 in *this* run is 0.35 ms
  against the 1 s budget. In the most contended run of the series (load average 48) the same 6-ball
  cell reached p99 0.82 ms and a single decision peaked at 29.6 ms — still ~34× inside the budget
  under a 5× oversubscribed CPU.
- Micro refinement costs **~20–40 µs** and does not change the outcome in these drills (the top
  seeds pot without it) — its value is robustness, not measurable quality here.
- Generation dominates: 0.15 ms of the 0.18 ms decision at 3 balls, 0.26 ms of 0.32 ms at 6 balls.
- One micro-refinement pass is 15 sim evaluations; with the default cap 24 the shortlist takes 6.
- **Reproducibility of the harness itself:** two full `measure all` runs produce byte-identical
  candidate dumps and eval tables; only wall-clock fields differ.

**Sim cost per shot (the unit that scales the SLO).** Toy model: **mean 3.55 µs per shot**
(p50 2.67, p90 4.83), **9.2 events per shot** (p50 8, p90 14, p99 21) — i.e. ~0.4 µs per event. The real event-driven sim adds spin modes, jaw collisions, simultaneity groups and the
per-event solving that goes with them; if it lands even 100× more expensive per event (≈ 0.25 ms
per shot), the recommended `B = 24` still costs ≈ 6 ms per decision and the SLO remains two orders
of magnitude away. `[INFERENCE]` — the multiplier is a guess; the *measured* facts are the toy
per-shot cost, the event count and the decision cost.

### 2.3 Candidate-set debug dump

`results/candidate-dump.json`: three positions (3-ball drill, 6-ball drill, easy pot) with the
position, the full generated count, the top 24 candidates by analytic seed score, the 8 sim-verified
shortlist rows with per-candidate scores, the chosen candidate and the refined strike, and one real
encoded sample (`obs` 64 floats, first candidate row 16 floats, K = 32). Intended for eyeballing
here and for the later human playtest (#9 §8's debug view).

### 2.4 ONNX → `ort`

| Quantity | Value |
|---|---|
| Model | `policy-smoke.onnx` (the trained smoke policy), 91,668 B, sha256 `1553dac9…b250f`, opset 17, 30,210 params; the untrained checkpoint `policy-scratch.onnx` (sha256 `5220b8f2…ebf2e5`, byte-identical architecture) is kept beside it |
| ORT (Rust, in-process) vs Python golden vectors | max abs error **4.77e-7** at k = 5 — the same error the Python side's own ORT run reports (4.77e-7), i.e. the Rust path reproduces the export bit-for-bit at f32 granularity |
| ORT (Python) golden checks | 4.77e-7 at K = 5, 1.91e-6 at K = 9 (dynamic candidate axis verified) |
| Session load | 21–34 ms, once at startup |
| Inference per decision (32 candidates) | min **0.035 ms**, p50 0.056, p99 2.88 ms under load (0.027 / 0.066 on a lightly loaded box) |
| Generation per decision (same loop) | min 0.235 ms, p50 0.317 |
| Encoding per decision | p50 0.0013 ms |
| Total per-decision path, ORT build | min **0.273 ms**, p50 0.378, p99 16.5 ms under load |
| Total per-decision path, fallback build | min **0.239 ms**, p50 0.312, p99 4.14 ms under load |
| ORT's own contribution to the decision | **+0.035 ms** at the median-of-minima (0.273 vs 0.239) — the policy network is noise next to generation |
| **Binary size**, same binary without ONNX Runtime | **696,784 B** |
| **Binary size**, with ONNX Runtime (static, `ort` 2.0.0-rc.13) | **26,552,480 B** |
| **Delta** | **+25,855,696 B (+25.86 MB, ×38.1)** |

The delta is the whole statically linked ONNX Runtime: `ort-sys` fetched **ONNX Runtime 1.28.0**
(pyke dist, `aarch64-apple-darwin+coreml`, tarball sha256 `6934874e…674c7`) and its
`libonnxruntime.a` is 80,425,592 B on disk, of which 25,855,696 B survives into the linked binary
after the linker keeps only the reachable code. The "same binary" claim is literal — one source
file, two feature builds, identical workload. Static linking means no `libonnxruntime` dylib to
ship, and no runtime load path to get wrong; the cost is the 25.9 MB. That is fine for the desktop
app and it is why #9 keeps `tract` as the escape hatch and libtorch off the ship list.
`otool -L` on the ORT build shows only system frameworks (`libc++`, Foundation, CoreML).

## 3. Eval-bar calibration (what the toy env could measure)

Suites and seeds (all reproducible; the seeds are the ones the runs used):

| Suite | Positions | Seeds | Measured (scripted anchor, σ = 0) |
|---|---|---|---|
| Easy pot | 40 | `easy_pot(9000..9039)` | **40/40 = 1.000**, scratch 0.000 |
| Class probes: direct | 48 | `drill(20000..20015, 6 balls)`, top 3 by makeability per position, micro-refined | pot **0.875**, first contact 1.000, scratch 0.000 |
| Class probes: bank (1–3 rails) | 48 | same | pot **0.625**, first contact 1.000, scratch 0.042 |
| Class probes: kick (1–3 rails) | 48 | same | pot **0.229**, first contact 0.833, scratch 0.104 |
| Class probes: combination (≤1 intermediate) | 48 | same | pot **0.875**, first contact 1.000, scratch 0.062 |
| Safety (escape) | 48 | same | first contact **1.000**, scratch 0.000, leave 0.598 (best of any class) |
| Clearance drill (3-ball open table) | 30 | `drill(31000..31029, 3 balls)`, planner, max 8 shots | rack cleared **29/30 = 0.967** |
| Determinism | 8 | `drill(41000..41007)` | identical decisions bit-for-bit and identical rest-state hashes: **true** |

- **`≥ 95 %` easy-pot gate: achievable.** The anchor hits 1.000 with no noise, and the Pro-level σ
  costs only 2.7 points (0.973). The gate is therefore a real gate rather than a free pass, and it
  survives the difficulty model's smallest σ.
- **`≥ 60 %` rack win rate vs the scripted anchor: NOT measurable here.** There is no rules layer,
  no opponent seat and no 8-ball rack in the prototype, so a win rate has no definition yet. The
  closest honest reading is the clearance drill above (0.967 with 3 balls on an open table, which is
  an easier game than a rack). The gate stays **provisional** and needs the real env.
- **Zero illegal declarations**: in the prototype every generated candidate names a live object ball
  and was cleared for line of sight, so the mask is all-true by construction (the real mask comes
  from the rules layer, #9 §2). Measured substitute: the executed shot made a first contact in
  1.000 (direct), 1.000 (bank), 0.833 (kick), 1.000 (combination), 1.000 (safety escape) of probes;
  the fallback positions are misses on hard multi-rail kicks, not illegal calls.
- **Selection mix** (60 drill positions, full capability set): direct **78.3 %**, safety escape
  **21.7 %**, bank/kick/combination **0 %**. Rail families are offered in volume (42.5 banks, 109.8
  kicks per position) but never win a decision once a direct pot or a safe exists.

## 4. PPO smoke run, env throughput, and the service envelope

Commands: `prototypes/ai-spine/python/README.md`. Raw record:
`prototypes/ai-spine/results/python-measurements.json`; curve points in the same file.

| Quantity | Value |
|---|---|
| Env | shot-granularity Gymnasium, 3 object balls + cue, `max_shots = 8`, obs 64, cand 16, K_max 32, `Discrete(32)` + mask |
| Env throughput | **1,209 env steps/s** single process (~153 episodes/s, mean episode 7.9 shots) with uniformly random legal actions; the prototype env is pure Python. The shipped env is #11's pipe to the headless binary, where one decision costs 0.27 ms of Rust — so this number measures the prototype's inner loop, not the design |
| Policy | `MaskablePPO` (sb3-contrib 2.9.0) with a custom per-candidate shared-scoring policy head; 8 envs, `n_steps 64`, `batch 256`, `n_epochs 10`, `lr 1e-4`, `ent_coef 0.005`, `target_kl 0.03`, `seed 42`, CPU |
| Steps/s | **1,501** (32,256 steps in 21.5 s) |
| Drills | 3-ball open table; episode = one rack, reward `+2.0` per ball potted, `+1.0` clear, `+0.05` legal hit, `+0.10 × progress`, `−1.0` scratch, `−0.10` illegal |
| Learning curve | 2.34 mean return at 512 steps → 7.13 at 32,256 (63 rollout points, monotone from ~9k); random policy **2.22**; greedy heuristic policy 4.3; a full clear is 7.0 + progress |
| Drill gate reached | `ep_rew_mean ≥ random + 0.5` for 3 consecutive rollouts at **9,216 steps / 6.4 s** wall clock |
| Checkpoints 10 / 50 / 100 % | mean return 6.770 / 6.972 / 7.104 on a fixed 100-episode eval — monotone in this 32k run **but not in the 150k run** (6.899 / 4.405 / 3.324; see §1 and §5) |
| Value head | final value loss 0.084, explained variance **0.970** |
| Return distribution (final policy, 100 episodes) | mean 7.126, sd 0.337, min 6.206, max 7.408 → **0.891 reward per shot** |
| AI decisions/s (policy forward pass, Python) | 40,153/s — the net is not the constraint; the env/sim is |
| ONNX export of the trained policy | `results/onnx/policy-smoke.onnx`, 91,668 B, sha256 `1553dac9…b250f`, opset 17, 30,210 scoring params |
| Scripted planner decisions/s (Rust, incl. sim-in-the-loop micro) | **~9,400/s** (3-ball clearance drill, 0.106 ms/decision mean) |

Note the asymmetry: the learned policy's *decisions* are ~4× cheaper than the scripted planner's,
because the planner pays for candidate generation and sim verification while the policy pays for a
forward pass over candidates someone else generated. Both sit far under the SLO.

## 5. What could not be measured, and the sensitivity

1. **Anything involving the real physics.** No spin, throw, squirt, sliding phase, jaw geometry or
   simultaneity: the toy's per-shot cost (3.55 µs, 9.2 events) is a floor, and the SLO margin scales
   linearly with whatever the real sim costs per shot. The work-unit cap is the right budget unit
   precisely because of this.
2. **Rack-level quality.** Rack win rate, groups, safeties-as-a-plan and the 8-ball endgame need the
   rules layer; the clearance drill is a proxy, not the gate.
3. **σ for spin and elevation.** The toy has no spin/elevation axes; only the aim (and a proportional
   speed) σ curve is measured. The correlation structure #9 §7 asks for is unmeasured by
   construction, and so is the claim that one σ set per level reads as a difficulty curve to a human.
4. **Reward shaping.** The prototype's drill reward is a stand-in, and it needed one rebalance
   before the policy would learn: with the first shaping the policy collapsed into a bank-only safe
   game after ~8k steps; the delivered scale (pot 2.0 vs legal hit 0.05) improves monotonically.
   That is evidence *for* keeping the reward row provisional, not evidence for these weights.
5. **Training-schedule stability.** The 150k-step run degraded after ~30k steps on the same fixed
   eval (6.90 → 4.41 → 3.32). A 32k-step run with the same hyperparameters is still rising at the
   end. Nothing here establishes where the ceiling is, so the checkpoint fractions stay provisional
   and #9's "ship only if eval shows monotone skill" rule is the right gate.
6. **Non-aimed candidate quality at scale.** Class success rates come from the top-3 candidates by
   analytic makeability per position; the tails (thin cuts, 3-rail kicks) are measured only through
   the forced probes and the σ sweep.
7. **Cross-language feature equality.** The observation layout was verified against the Python env
   with a fixture position (15 of 64 indices, max abs diff 0.0 — after the check caught a real
   `/L` vs `/1270` divergence in the Rust encoder). The *candidate* feature rows were not
   cross-checked index by index; the two halves implement the written table independently.
8. **CI.** There is no CI-run measurement: this repo's `ci` check is a checkout-only placeholder by
   design (ADR 0001), and wiring real CI is out of scope for this campaign.

## 6. Handoff

- **#12 (spec assembly):** this file is the AI section's constants appendix; §1's rows replace the
  `—` cells of the §10 table on #9, and §2 stands in for the "provisional" adjectival hand-waving
  around latency, throughput and binary size.
- **#8 (physics):** the four numbers that gate the SLO and the cap are the real sim's per-shot cost,
  its event count, the real candidate-verification cost at `B`, and the real generation cost at 15
  balls; re-pin `B` and the σ set against them.
- **#11 (architecture):** the serve path measured here is the interface #11 specifies — ORT behind a
  feature/trait, one intra-op thread, ORT-free core, hashed model artifact loaded once (21–42 ms),
  and a pipe payload of `obs`/`cand`/mask that the Rust side encodes exactly once (encoding measured
  at 0.5 µs per decision, so the pipe's JSON overhead will dominate the encoding).
- **#10 (cue UX):** the strike the policy commits is the same `{aim, speed, spin, elevation}`
  declaration the human produces; the measured σ curve is what a difficulty slider is calibrating.
