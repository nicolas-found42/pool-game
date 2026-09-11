# AI-spine prototype (wayfinder #16)

Throwaway prototype of the decision pipeline's spine, built to measure costs and pin the
calibration constants of the AI section of the spec (`docs/spec/ai-constants.md`; the AI resolution
lives on issue #9). **Nothing here ships.** No physics fitting (#8), no rules engine, no Bevy shell,
no 8-ball-complete env, no full training run.

## Layout

| Path | What it is |
|---|---|
| `rust/` | Candidate generator, scripted planner + micro, toy analytic table model, encoders, measurement harness, ONNX serve path (`ort`) |
| `python/` | Gymnasium shot-granularity env, SB3 PPO smoke run, ONNX export (see `python/README.md`) |
| `results/` | Raw measurement records (JSON) and the candidate-set debug dump |

The two halves meet at the encoding contract (64-float observation, 16-float candidate row) and the
ONNX graph signature — both fixed in this README and implemented independently on each side.

## Toolchain

- Rust **1.98.1** (stable, `aarch64-apple-darwin`), release profile.
- `ort = "=2.0.0-rc.13"` behind the non-default `onnx` feature, features
  `["std", "download-binaries", "tls-native", "ndarray"]`, one intra-op thread
  (`Session::builder().with_intra_threads(1)`) — the pins #9 §6 asks for. `download-binaries`
  requires a TLS feature; without `tls-native` the build fails in `ort-sys`'s build script.
  ONNX Runtime is **statically linked** (`otool -L` shows no `libonnxruntime` dylib).
- Python via `uv` (Python 3.12; the system 3.14 has no SB3/gymnasium wheels): see `python/README.md`.
- All measurements on: **Apple M5, 10 cores, 16 GB RAM, macOS 26.4.1** (`aarch64`).

## Commands

Everything below is run from `prototypes/ai-spine/rust` after `cargo build --release`.
Runtimes are wall clock on the machine above.

| Command | What it measures | Runtime |
|---|---|---|
| `./target/release/measure all --out-dir ../results` | all four sections below + the candidate dump | ~0.6 s |
| `./target/release/measure gen --out-dir ../results` | candidate generation throughput by shot class and rail depth; shortlist sort cost | ~0.1 s |
| `./target/release/measure decision --out-dir ../results` | per-decision cost breakdown, work-unit-cap sweep, executed quality per cap, SLO check, raw sim cost | ~0.3 s |
| `./target/release/measure eval --out-dir ../results` | eval-bar calibration: easy-pot suite, per-class success, safety legality, σ sweep, clearance drill, determinism, selection mix | ~0.2 s |
| `./target/release/measure dump --out-dir ../results` | candidate-set debug dump (per-candidate scores for 3 positions) | ~0.05 s |
| `./target/release/measure fixture --fixture ../results/obs-fixture.json --out-dir ../results` | cross-language check of the 64-float observation layout against the Python env's fixture | ~0.05 s |
| `./target/release/serve --positions 500 --json ../results/serve-latency-noort.json` | per-decision latency of generation + encoding + selection **without** ONNX Runtime | ~0.3 s |
| `cargo build --release --features onnx` then `./target/release/serve --model ../results/onnx/policy-smoke.onnx --golden ../results/onnx-golden.json --positions 500 --json ../results/serve-latency-onnx.json` | same workload with the ORT policy + golden-vector check | ~0.3 s (+ ~30 s build) |

Binary-size delta: build both ways and compare the same output path (copy the first aside, since
Cargo overwrites it):

```sh
cargo build --release                 && cp target/release/serve /tmp/serve-noort
cargo build --release --features onnx && cp target/release/serve /tmp/serve-onnx
stat -f "%z %N" /tmp/serve-noort /tmp/serve-onnx
```

Measured: 696,784 B (no ORT) vs 26,552,480 B (ORT) → **+25,855,696 B**.

## Encoding contract (shared with `python/`)

`obs` — 64 float32, spec frame, nothing quantized:

| Index | Meaning |
|---|---|
| `3*i + 0,1,2` for `i` in 0..16 | slot `i`: `x/(L/2)`, `y/(W/2)`, present (slot 0 = cue ball, 1..15 = object balls) |
| 48 | `shot_index / max_shots` |
| 49 | `balls_remaining / n_initial` |
| 50 | 1.0 (cue ball present) |
| 51 | mean over live object balls of (distance to nearest pocket mouth) / 1270 |
| 52 | min over live object balls of (cos of the angle between (CB→OB) and (OB→best pocket)) |
| 53 | 1.0 if a direct-pot candidate exists in the masked set |
| 54 | legal candidates / 32, clipped |
| 55 | mean candidate makeability |
| 56 | best candidate makeability |
| 57 | 1.0 in the last third of the shot budget |
| 58–61 | reserved (ball-in-hand domain, group state, on-8, score differential) — constants in the prototype |
| 62 | 1.0 (case = open table) |
| 63 | fouls so far / 3 |

`cand` — one row of 16 float32 per candidate, in this order:
`kind_pot`, `kind_safety`, `kind_bank`, `kind_kick`, `kind_combo`, `cut_cos`, `cut_angle/π`,
`d_cue_contact/L`, `d_obj_pocket/L`, `clearance/(4R)` clipped, `rails/3`, `intermediates`,
`makeability`, `leave`, `contact_x/(L/2)`, `contact_y/(W/2)`.

ONNX graph: inputs `obs` f32 `[1,64]`, `cand` f32 `[1,K,16]` (candidate axis dynamic), output
`logits` f32 `[1,K]` **raw** — masking happens outside the graph (the shell's rules-layer mask, the
`ort` side's masked argmax). Opset 17.

## Toy model vs the real sim

The toy table model is deliberately thin: spec frame and geometry constants only (mm frame, playing
surface 2540 × 1270, `R = 28.575`, corner/side mouths 115.9/128.6, `e_bb = 0.95`, cushion
`e_n = 0.75`, `μr = 0.010`, sleep 1 mm/s), pockets as capture discs, event-driven but with
constant-velocity collision detection plus conservative advancement to contact. No spin, throw,
squirt, sliding phase, jaw geometry, or simultaneity groups — all of that is #8's. Per-decision
costs measured here are therefore *floors*: the real event-driven sim is slower per shot by an
unknown factor, which is exactly why the work-unit cap (not a wall clock) is the budget.

## Results

- `results/gen-throughput.json` — throughput by ball count and rail depth, per-class and per-depth counts.
- `results/decision-cost.json` — per-decision cost, work-cap sweep, executed quality, raw sim cost.
- `results/eval-calibration.json` — eval-bar calibration, σ sweep, clearance drill, determinism, selection mix.
- `results/candidate-dump.json` — candidate-set debug dump (per-candidate scores, 3 positions) + an encoded sample.
- `results/serve-latency-noort.json`, `results/serve-latency-onnx.json` — serve-path latency and binary sizes.
- `results/onnx/` — exported models (+ sha256) and `results/onnx-golden.json`.

The numbers, their provenance labels (*measured* / *derived* / *still provisional*) and the
hardware they came from are in `docs/spec/ai-constants.md`.

**Measurement conditions:** this worktree ran beside two other agent workloads on the same machine,
so the 1-minute load average was 13–48 on 10 cores the whole time. Every published record is the
fastest of 5–6 runs (`measure all` was run repeatedly; the serve latencies likewise), and the same
cells varied by up to ~2.3× across runs. Every timing here is therefore an **upper bound** — which
is why the SLO conclusion is worth anything: it survives that pessimism. `min` fields in the serve
records are the least-contended samples.

Not committed: training checkpoints (`python/runs/**/*.zip`, ~15 MB) and the virtualenv — see
`.gitignore` beside this file. The small ONNX artifacts that are committed (91,668 B each) carry
their sha256 beside them.
