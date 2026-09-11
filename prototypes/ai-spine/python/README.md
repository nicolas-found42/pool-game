# Pool AI spine — Python half (prototype, ticket #16)

Throwaway prototype: a Gymnasium shot-granularity env over a spec-constants-only
analytic toy sim, a MaskablePPO smoke run that visibly learns, and an ONNX export
of the candidate scorer for the Rust/`ort` serving path.

## Toolchain

`uv`-managed CPython 3.12 (the system Python 3.14 is not used):

```bash
cd prototypes/ai-spine/python
uv venv --python 3.12 .venv
uv pip install -p .venv/bin/python gymnasium sb3-contrib torch onnx onnxruntime numpy
# torch 2.14.0 (CPU), stable-baselines3 2.9.0, sb3-contrib 2.9.0, gymnasium 1.3.0,
# onnx 1.22.0, onnxruntime 1.30.0, numpy 2.5.3
```

Sanity check:

```bash
.venv/bin/python -c "import torch, sb3, sb3_contrib, gymnasium, onnx, onnxruntime"
.venv/bin/python -c "from gymnasium.utils.env_checker import check_env; import env; check_env(env.PoolShotEnv(sim_seed=1), skip_render_check=True)"
```

## Commands (with measured runtimes on the Apple M5)

```bash
# 1. env fixture (deterministic position + obs vector for the Rust encoder cross-check)
.venv/bin/python env.py --fixture ../results/obs-fixture.json          # ~1 s

# 2. early unblock artifact: architecturally final, random weights
.venv/bin/python export_onnx.py --untrained                            # ~27 s (incl. torch import)

# 3. PPO smoke run (150k steps, 8 envs, CPU)
.venv/bin/python train_ppo.py --timesteps 150000 --n-envs 8 --eval-episodes 100 \
    --lr 1e-4 --ent-coef 0.005 --target-kl 0.03 --seed 42 --run-name ppo_smoke
# ~2.5 min total: ~1500 steps/s of training plus checkpoint/random/final evaluations
# writes runs/ppo_smoke/{train_metrics.json,ckpt_010.zip,ckpt_050.zip,ckpt_100.zip,final.zip}

# 4. trained ONNX export + ORT golden check
.venv/bin/python export_onnx.py --checkpoint runs/ppo_smoke/final.zip   # ~25 s

# 5. assemble results/python-measurements.json (env throughput, ORT decision rate)
.venv/bin/python make_measurements.py
```

## Files

| file | what |
|------|------|
| `env.py` | analytic toy sim + `PoolShotEnv` (Gymnasium). Docstring carries the spec constants, the pocket model, the full obs layout (indices 0..63) and the candidate feature table (columns 0..15) |
| `model.py` | `ScoringNet` (the exported architecture) + `CandidateScoringPolicy` (SB3 MaskablePPO policy with the shared per-candidate scorer as its action head) |
| `train_ppo.py` | MaskablePPO smoke run: seeded, CPU, `VecMonitor`-wrapped `SubprocVecEnv`, per-rollout curve, checkpoint fractions 10/50/100 % |
| `export_onnx.py` | `--untrained` / `--checkpoint` ONNX export + sha256 + golden json + meta json |
| `make_measurements.py` | assembles `../results/python-measurements.json` from the run artifacts and live measurements |

## Env in one paragraph

One env step = one shot. The env generates candidates (direct pots, one-rail
banks, one-rail kicks, safeties; padded to `K_MAX = 32`), the agent picks one by
index via `Discrete(32)` with the mask in the observation, and the toy sim rolls
the balls to rest (rolling deceleration `a = mu_r*g = 98.0665 mm/s^2`, equal-mass
ball-ball impulse with `e = 0.95`, cushion normal restitution `e_n = 0.75`, pocket
capture at the 6 mouth centres, sleep at 1 mm/s). Only the aim direction is
perturbed at execution: `N(0, 0.0015 rad)`. Episode = the 3-ball open-table
drill, cleared or 8 shots. Reward: `+2.0` per object ball potted, `+1.0` clear
bonus, `+0.05` legal hit, `+0.10 * progress`, `-1.0` scratch (cue respawned on
the head spot), `-0.10` illegal action.

## Model / ONNX contract

```
ctx    = Linear(64->64) ReLU Linear(64->64) ReLU          on obs[64]
c_k    = Linear(16->64) ReLU Linear(64->64) ReLU          on cand[k,16]   (shared)
logit_k= Linear(128->64) ReLU Linear(64->1)               on [ctx, c_k]
value  = Linear(128->64) ReLU Linear(64->1)               on [ctx, mean_k c_k]   (not exported)
```

ONNX: single graph, opset 17, inputs `obs` `[1,64]` and `cand` `[1,K,16]` (axes 0
and 1 dynamic), output `logits` `[1,K]` **raw**; masking stays outside the graph.
30,210 scorer parameters (value head excluded). `onnx-golden.json` holds the
golden vectors (K=5) and the ORT-vs-PyTorch max abs error.

## What the prototype does NOT model (all documented in `env.py`)

- No spin, throw, squirt, cushion spin transfer, jaw rattle or pocket hang; the
  sim is spec-frame geometry plus the constants above only.
- No combos (candidate column 11 `intermediates` is always 0) and no multi-rail
  banks/kicks (candidates are single-rail).
- The bank speed estimate ignores the cushion loss; safety/leave heuristics are
  crude closed-form proxies.
- Ball-ball contact times are found with a constant-velocity approximation over
  one short step (`DT_CAP = 20 ms`); impacts are resolved at the sampled contact
  point, so a glancing contact can be off by a millimetre or two.
- Obs slots 58..61 and 63 are reserved constants (0.0/1.0) by design so the Rust
  encoder matches trivially; slot 62 is always 1.0 (open table).
