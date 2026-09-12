# AI pipeline: measured constants and calibration

Status: **accepted as the AI section's constants appendix at #16** (2026-09-11), merged into the
assembled spec at #12, feeding the AI
section of the spec (the resolution on #9). The gate accepted the measured cost envelope and pinned
`B` = 16–24 work units; the rows marked *provisional* below stayed provisional, each with its
disposition and the measurement that settles it recorded in the resolution on #16. Every number here
is indicative and gets re-pinned against the real simulation (#8) at implementation: the prototype's
table model is spec frame/geometry constants only, with no fitted physics, no rules layer, and no
shipping code anywhere.

- Prototype: `prototypes/ai-spine/` (Rust generator/planner/toy sim/`ort` serve path; Python env and
  PPO smoke run). Commands and toolchain: `prototypes/ai-spine/README.md`.
- Raw records: `prototypes/ai-spine/results/*.json` (throughput, per-decision cost, eval
  calibration, serve latency, candidate dump, ONNX golden vectors).
- Hardware for **every** measurement in this document: **Apple M5, 10 cores, 16 GB RAM, macOS
  26.4.1 (aarch64)**, release build, single process, one `ort` intra-op thread.
- **Measurement conditions, stated plainly:** this worktree ran beside two other agent workloads
  on the same machine, so the 1-minute load average moved between 13 and 48 on 10 cores. Every
  published record is the **fastest of a repeated series** (8 × `measure all`, 6 × each serve build).
  The numbers below come from the quietest run of the series; across the loaded runs the same cells
  were up to ~2.3× slower, so treat every timing as an **upper bound** and expect a re-pin on a truly
  idle machine. The conclusions that matter (SLO headroom, cap saturation, binary size, golden
  vectors) are unaffected — they hold with more margin on the loaded runs, and the shortest/longest
  observed bands are recorded in §2.
Labels: **measured** = read directly off a run recorded in `results/`; **derived** = arithmetic on
measured values with the rule stated; **provisional** = not measured here, carried from #9 or
needing measurement that is out of the prototype's scope.

## 1. Calibration table (#9 §10, filled; dispositions ruled at #16)

| Constant | Provisional | Pinned by this prototype | Label | Disposition at #16 |
|---|---|---|---|---|
| Work-unit budget `B` (sim evals / decision) | — | **4–8** for quality (executed pot rate saturates: 0.63 → 0.93 (3-ball) and 0.70 → 0.88 (6-ball) at 4 evals, flat to 64); **16–24** recommended for the spec, bounded by the SLO at the real sim's per-shot cost | derived (quality) / provisional (real-sim bound) | **pinned 16–24**; the work-unit cap is the only budget unit. The SLO and `B` hold at ≥ 100× the measured unit cost — the floor (2.48 µs/shot, 9.2 events/shot) is measured, the margin is derived |
| `R` / `C` (cushion contacts / intermediate balls) | 3 / 1 | Kept. Measured: rails cost 0.673 ms vs 0.034 ms per 15-ball generation at depth 3 vs 0 — cheap; forced success 0.625 (banks), 0.229 (kicks), 0.875 (combinations); **selection share 0 %** in the prototype's drills (direct pots and safeties always outscore them) | measured (cost, success, share) / provisional (that the caps bind on real racks) | **pinned 3 / 1, unexercised**: the first league run reports the rail families' selection share on real rack positions; that report, not this prototype, justifies keeping `R` = 3 |
| σ magnitudes per level (aim) | — | Measured easy-pot make rate vs aim σ (400 trials per point): 0 → 1.000, 0.5 mrad → 0.973, 1 mrad → 0.932, 2 mrad → 0.830, 4 mrad → 0.660, 8 mrad → 0.458, 16 mrad → 0.295. Level assignment **derived** from that curve: Pro 0.5 mrad (≥ 0.97), Advanced 1–2 mrad (0.83–0.93), Intermediate 4 mrad (0.66), Beginner 8–16 mrad (0.30–0.46) | measured (curve) / derived (assignment) | **accepted** as the difficulty scale; σ for speed / spin / elevation re-measured on the real sim, one axis at a time |
| σ for speed / spin a,b / elevation | — | Speed σ assumed at 2.5 × aim σ; **never measured** — the toy model has no spin or elevation to perturb | provisional | **provisional, shape kept, magnitudes dropped**: one per-level factor scales all four axes, speed at 2.5 × aim, stated as an assumption |
| Checkpoint fractions | 10 / 50 / 100 % | **Not pinnable from this prototype.** A 150k-step smoke run evaluated at those fractions degrades (mean return 6.899 / 4.405 / 3.324 on a fixed 100-episode eval); a 32k-step run of the same configuration is monotone (6.770 / 6.972 / 7.104). The fractions depend on where the real schedule plateaus, which nothing here establishes | provisional | **dropped from the spec**; the monotone-skill rule replaces them (`ai.md` §6) |
| League `M` / `N` | order 8–16 | Sizing rule from measured throughput: a league round costs `M·N·shots_per_rack·(decision + shot)`. At the measured 0.16–0.31 ms decision and 8 shots/rack, `M·N = 16` is ~20–40 ms of CPU in the prototype; at the measured Python env throughput (1,209 steps/s) the same round is ~0.1 s. The binding term is the **real** sim's per-shot cost and whatever fraction of the training budget a round may take — `M·N` = 8–16 remains plausible, but it is an envelope argument, not a measurement | derived | **sizing rule kept, value left open**: `M·N` set from the real sim's per-shot cost and a league round's wall clock, reported by the first league run. (1,439 steps/s is the raw env probe; 1,209 steps/s is the 150k run's SB3 fps and sizes the round.) |
| Net sizes, PPO hyperparameters | — | Smoke net: cand-branch `16→64→64`, ctx-branch `64→64→64`, combine `128→64→1`, value `128→64→1`; **30,210 trained params incl. the value head / 21,889 in the exported scorer** (the value head is deliberately not exported); ONNX 91,668 B; PPO hyperparameters and throughput from §4 | measured | **accepted**; the value head's width and the param split are corrected here per #16 §6 |
| Reward weights and normalization | — | **Not pinned, and shown to matter:** the drill reward needed one rebalance before PPO learned at all (the first shaping let the policy collapse into a bank-only safe game after ~8k steps; potting 2.0 vs legal-hit 0.05 fixed it). The spec's adjudication-driven reward (§9 §5) is a different object. Measured return range: mean 7.13, sd 0.34, 0.89 per shot | provisional | **provisional, one constraint pinned**: a terminal, per-ball reward must dominate the per-shot legal-hit term, and the reward stays adjudication-derived (`ai.md` §5) |
| Eval suite seeds and gate thresholds | provisional in #9 §9 | Seeds and measured anchor rates in §3; the `≥95 %` easy-pot gate is **attainable** — the scripted anchor scores 40/40 = 1.000 at zero noise and 0.973 at the Pro σ | measured (anchor) / derived (thresholds) | **accepted**: seeds and thresholds pinned; the rack win-rate gate is restated as a milestone gate (`ai.md` §10) |

## 2. Measurements (the ticket's list)

### 2.1 Candidate generation throughput

Per **decision's** candidate set for one position, mean over 12 seeded positions per cell, depth =
maximum rail contacts in the constructions (`depth 0` = direct + combination + safety only), release.

| Balls | Rail depth | Generation | Constructions/s | Candidates kept (after LOS) | After dedup | Banks kept (1/2/3 rails) | Kicks kept (1/2/3 rails) |
|---|---|---|---|---|---|---|---|
| 3 | 0 | 0.007 ms | 10.4 M | 22 | 22 | — | — |
| 3 | 1 | 0.026 ms | 8.2 M | 56 | 55 | 12 / — / — | 21 / — / — |
| 3 | 2 | 0.074 ms | 8.8 M | 119 | 117 | 12 / 18 / — | 21 / 44 / — |
| 3 | 3 | 0.177 ms | 11.0 M | 168 | 165 | 12 / 18 / 13 | 21 / 44 / 35 |
| 6 | 0 | 0.009 ms | 26.4 M | 29 | 29 | — | — |
| 6 | 1 | 0.039 ms | 13.3 M | 72 | 72 | 13 / — / — | 29 / — / — |
| 6 | 2 | 0.128 ms | 10.8 M | 148 | 146 | 13 / 20 / — | 29 / 55 / — |
| 6 | 3 | 0.295 ms | 13.5 M | 200 | 197 | 13 / 20 / 12 | 29 / 55 / 39 |
| 15 | 0 | 0.034 ms | 40.6 M | 42 | 42 | — | — |
| 15 | 1 | 0.121 ms | 17.2 M | 74 | 74 | 8 / — / — | 23 / — / — |
| 15 | 2 | 0.295 ms | 14.4 M | 118 | 117 | 8 / 8 / — | 23 / 36 / — |
| 15 | 3 | 0.673 ms | 15.9 M | 142 | 140 | 8 / 8 / 2 | 23 / 36 / 20 |

Loaded-run band for the heaviest cell (15 balls, depth 3): 0.673–0.886 ms across the series.

Reading: the rail families cost ~20× the direct-only generation at full rack (0.673 ms vs 0.034 ms)
and are still under a millisecond; a crowded table prunes hard (banks 42 → 20 kept at 6 balls;
20 → 11 at 15 balls) because line-of-sight kills multi-leg paths first. Per-class offered rates at
6 balls (untruncated, 60 positions): direct 5.9, bank 42.5, kick 109.8, combination 6.0, safety
escape 7.9, roll-up 3.8, two-way 1.0 candidates per position.

**Top-K shortlist cost.** Sort + truncate to K = 6: mean **0.24 µs**, p50 0.08 µs, p99 1.33 µs — free
next to a sim evaluation (a sim shot is ~2.5 µs).

### 2.2 Sim verification + micro refinement per decision; the 1 s SLO

Per decision, 60 (3-ball) / 40 (6-ball) seeded drill positions per row, scripted planner.
"Executed pot" is the committed declaration re-simulated noise-free — the quality that cap buys.

| Balls | `B` (cap) | Total mean | Total p99 | Generate | Verify | Micro | Sim evals used | Executed pot |
|---|---|---|---|---|---|---|---|---|
| 3 | 0 | 0.139 ms | 0.146 | 0.138 | 0 | 0 | 0 | 0.633 |
| 3 | 4 | 0.144 ms | 0.159 | 0.137 | 0.006 | 0 | 4 | **0.933** |
| 3 | 8 | 0.146 ms | 0.153 | 0.134 | 0.008 | 0.003 | 8 | 0.933 |
| 3 | 16 | 0.155 ms | 0.166 | 0.133 | 0.007 | 0.013 | 16 | 0.933 |
| 3 | 24 | 0.160 ms | 0.170 | 0.133 | 0.007 | 0.018 | 21 | 0.933 |
| 3 | 64 | 0.161 ms | 0.179 | 0.135 | 0.007 | 0.017 | 21 | 0.933 |
| 6 | 0 | 0.269 ms | 0.319 | 0.268 | 0 | 0 | 0 | 0.700 |
| 6 | 4 | 0.275 ms | 0.297 | 0.262 | 0.012 | 0 | 4 | **0.875** |
| 6 | 8 | 0.284 ms | 0.308 | 0.261 | 0.016 | 0.005 | 8 | 0.875 |
| 6 | 16 | 0.301 ms | 0.320 | 0.262 | 0.016 | 0.024 | 16 | 0.875 |
| 6 | 24 | 0.314 ms | 0.347 | 0.261 | 0.016 | 0.036 | 21 | 0.875 |
| 6 | 64 | 0.318 ms | 0.352 | 0.261 | 0.018 | 0.037 | 21 | 0.875 |

Loaded-run band for the same cells: 3-ball p99 0.15–0.39 ms, 6-ball p99 0.30–0.82 ms across the series.

- **The SLO holds with a very large margin on this machine**: worst p99 in *this* run is 0.35 ms
  against the 1 s budget. In the most contended run of the series (load average 48) the same 6-ball
  cell reached p99 0.82 ms and a single decision peaked at 29.6 ms — still ~34× inside the budget
  under a 5× oversubscribed CPU.
- Micro refinement costs **~20–40 µs** and does not change the outcome in these drills (the top
  seeds pot without it) — its value is robustness, not measurable quality here.
- Generation dominates: 0.13 ms of the 0.16 ms decision at 3 balls, 0.26 ms of 0.31 ms at 6 balls —
  and generation is pure geometry, so it is the one cost the real sim cannot inflate.
- One micro-refinement pass is 15 sim evaluations; with the default cap 24 the shortlist takes 6.
- **Reproducibility of the harness itself:** two full `measure all` runs produce byte-identical
  candidate dumps and eval tables; only wall-clock fields differ.

**Sim cost per shot (the unit that scales the SLO).** Toy model: **mean 2.48 µs per shot**
(p50 2.29, p90 4.08), **9.2 events per shot** (p50 8, p90 14, p99 21) — i.e. ~0.27 µs per event. The real event-driven sim adds spin modes, jaw collisions, simultaneity groups and the
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
- **Reading the dump (corrected at #16 §6.3):** in all three committed positions the **top-ranked
  candidate pots** (0.731 / 0.738 / 0.614, `verified_pot` true); the inversion sits *below* the top — in
  the 6-ball position rank 1 (0.594) misses where rank 2 (0.569) pots, and in the easy-pot position ranks
  1–5 (makeability 0.725–0.726) **all miss** while rank 0 (0.614) and rank 7 (0.254) pot. Rank is the
  index into the sim-verified shortlist, ordered by the pre-sim `seed_score`; `makeability` is the
  analytic heuristic printed beside it. The qualitative finding stands — the analytic seed score is not
  makeability, which is what earns the verification pass its work units — and the earlier
  "top-ranked misses where the third pots" reading belongs to the wrong ranks.

### 2.4 ONNX → `ort`

| Quantity | Value |
|---|---|
| Model | `policy-smoke.onnx` (the trained smoke policy, exported from the run's `final.zip`), 91,668 B, sha256 `1553dac9…b250f`, opset 17, **21,889 exported scoring params** (the trained net carries 30,210 incl. the value head, which is deliberately not exported); the untrained checkpoint `policy-scratch.onnx` (sha256 `5220b8f2…ebf2e5`, byte-identical architecture) is kept beside it |
| ORT (Rust, in-process) vs Python golden vectors | max abs error **4.77e-7** at k = 5 — the same error the Python side's own ORT run reports (4.77e-7), i.e. the Rust path reproduces the export bit-for-bit at f32 granularity |
| ORT (Python) golden checks | 4.77e-7 at K = 5, 1.91e-6 at K = 9 (dynamic candidate axis verified) |
| Session load | 29.2 ms, once at startup |
| Policy path, cross-implementation | driving the SB3 policy on 20 live episodes, the Rust ORT masked argmax agreed on **83/83 shots**, max logit abs error 2.4e-6 — the strongest end-to-end check of the export, on top of the golden vectors |
| Inference per decision (32 candidates) | **min 0.014 ms, p50 0.016, p90 0.017, p99 0.022 ms**, max 0.038 (loaded-run band: p50 0.039–0.056, p99 0.70–2.9) |
| Generation per decision (same loop) | min 0.086, p50 0.097 ms |
| Encoding per decision | p50 0.00025 ms |
| Total per-decision path, ORT build | **min 0.102, p50 0.114, p90 0.120, p99 0.139 ms**, max 0.191 |
| Total per-decision path, fallback build (same binary, no ORT) | min 0.087, p50 0.098, p90 0.109, **p99 0.127 ms**, max 0.163 |
| ORT's own contribution to the decision | **+0.015 ms** at p50 (0.114 vs 0.098) — the policy network is noise next to generation |
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
| Env throughput | **1,439 env steps/s** single process (~183 episodes/s, mean episode 7.9 shots) with uniformly random legal actions (block rates 1,409 / 1,439 / 1,438 across the measurement's blocks) — **this is the raw env-probe throughput**. The shipped env is #11's pipe to the headless binary, where one decision costs 0.10–0.14 ms of Rust — so this number measures the prototype's inner loop, not the design. The 150k run's SB3 fps (**1,209 steps/s**, §1's `M`/`N` row) is the training loop's rate and is what sizes a league round; the two are different measurements of different loops, not a contradiction |
| Policy | `MaskablePPO` (sb3-contrib 2.9.0) with a custom per-candidate shared-scoring policy head; 8 envs, `n_steps 64`, `batch 256`, `n_epochs 10`, `lr 1e-4`, `ent_coef 0.005`, `target_kl 0.03`, `seed 42`, CPU |
| Steps/s | **1,501** on a quiet box (32,256 steps in 21.5 s); **989** when the same bit-exact re-run shared the box (32.6 s) — same checkpoints and same ONNX sha256 in both, so this is load, not variance |
| Drills | 3-ball open table; episode = one rack, reward `+2.0` per ball potted, `+1.0` clear, `+0.05` legal hit, `+0.10 × progress`, `−1.0` scratch, `−0.10` illegal |
| Learning curve | 2.34 mean return at 512 steps → 7.13 at 32,256 (63 rollout points, monotone from ~9k); random policy **2.22**; greedy heuristic policy 4.3; a full clear is 7.0 + progress |
| Drill gate reached | `ep_rew_mean ≥ random + 0.5` for 3 consecutive rollouts at **9,216 steps**, 6.4–7.5 s wall clock depending on load |
| Checkpoints 10 / 50 / 100 % | mean return 6.770 / 6.972 / 7.104 on a fixed 100-episode eval — monotone in this 32k run **but not in the 150k run** (6.899 / 4.405 / 3.324; see §1 and §5) |
| Value head | final value loss 0.084, explained variance **0.970** |
| Return distribution (final policy, 100 episodes) | mean 7.126, sd 0.337, min 6.206, max 7.408 → **0.891 reward per shot** |
| AI decisions/s (policy forward pass) | 56,324/s (Python ORT, batch 1, K = 32) and 62,500/s implied by the Rust measurement above — the net is not the constraint; the env/sim is |
| ONNX export of the trained policy | `results/onnx/policy-smoke.onnx`, 91,668 B, sha256 `1553dac9…b250f`, opset 17, **21,889 exported scoring params** (30,210 trained incl. the value head; see §2.4) |
| Scripted planner decisions/s (Rust, incl. sim-in-the-loop micro) | **~8,800/s** (3-ball clearance drill, 0.113 ms/decision mean) |

Note the asymmetry: the learned policy's *decisions* are ~4× cheaper than the scripted planner's,
because the planner pays for candidate generation and sim verification while the policy pays for a
forward pass over candidates someone else generated. Both sit far under the SLO.

## 5. What could not be measured, and the sensitivity

1. **Anything involving the real physics.** No spin, throw, squirt, sliding phase, jaw geometry or
   simultaneity: the toy's per-shot cost (2.48 µs, 9.2 events) is a floor, and the SLO margin scales
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
