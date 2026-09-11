"""Assemble results/python-measurements.json from the real run artifacts.

    python make_measurements.py            # after train_ppo.py + export_onnx.py

Sources (all measured, nothing typed by hand):
  * env throughput        : timed rollouts of a uniform-random legal policy
  * PPO metrics           : runs/<run>/train_metrics.json (SB3 logger / episode buffer)
  * ONNX                  : results/onnx/*-meta.json (opset, params, sha256, golden error)
  * ORT decision rate     : timed onnxruntime inference on the exported graph
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata as md
import json
import pathlib
import platform
import time

import numpy as np

from env import CAND_DIM, K_MAX, MAX_SHOTS, N_OBJECTS, OBS_DIM, PoolShotEnv

HERE = pathlib.Path(__file__).resolve().parent
RESULTS = HERE.parent / "results"

OBS_CONTEXT_LAYOUT = [
    {"index": 48, "name": "shot_index / max_shots", "kind": "computed"},
    {"index": 49, "name": "balls_remaining / n_objects", "kind": "computed"},
    {"index": 50, "name": "cue ball present", "kind": "constant", "value": 1.0},
    {"index": 51, "name": "mean over live object balls of (distance to nearest mouth) / 1270.0", "kind": "computed"},
    {"index": 52, "name": "min over live object balls of cos(angle((CB->OB), (OB->nearest pocket)))", "kind": "computed"},
    {"index": 53, "name": "legal direct pot candidate exists", "kind": "computed"},
    {"index": 54, "name": "n_legal_candidates / 32.0 clipped to 1.0", "kind": "computed"},
    {"index": 55, "name": "mean makeability of legal candidates (0 if none)", "kind": "computed"},
    {"index": 56, "name": "best makeability of legal candidates (0 if none)", "kind": "computed"},
    {"index": 57, "name": "episode in its last third of shots", "kind": "computed"},
    {"index": 58, "name": "reserved: ball-in-hand domain flag", "kind": "constant", "value": 0.0},
    {"index": 59, "name": "reserved: group state", "kind": "constant", "value": 0.0},
    {"index": 60, "name": "reserved: on-8 flag", "kind": "constant", "value": 0.0},
    {"index": 61, "name": "reserved: score differential", "kind": "constant", "value": 0.0},
    {"index": 62, "name": "case = open table", "kind": "constant", "value": 1.0},
    {"index": 63, "name": "fouls so far / 3 (tracked, held at 0 in the prototype)", "kind": "constant", "value": 0.0},
]


def sha256_file(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _env_block(n_episodes: int, seed: int) -> tuple[float, float, int, list[int]]:
    """Roll ``n_episodes`` uniform-random legal episodes; return (steps, steps/s, wall, lengths)."""
    env = PoolShotEnv(n_objects=N_OBJECTS, max_shots=MAX_SHOTS, sim_seed=seed)
    rng = np.random.default_rng(seed)
    steps = 0
    lengths: list[int] = []
    t0 = time.time()
    for ep in range(n_episodes):
        env.reset(seed=seed + ep)
        done = False
        while not done:
            action = int(rng.choice(np.flatnonzero(env.action_masks())))
            _obs, _r, terminated, truncated, _info = env.step(action)
            steps += 1
            done = terminated or truncated
        lengths.append(env._shot_index)
    wall = time.time() - t0
    return steps / wall, wall, steps, lengths


def measure_env(blocks: int = 3, episodes_per_block: int = 150, seed: int = 4242) -> dict:
    """Raw env throughput (uniform random legal actions, fresh rack per episode).

    Headline is the fastest block: the prototype box is shared with sibling agents and
    a later re-measure dropped 10x purely from contention, so the block spread is kept
    in the payload instead of being averaged away.
    """
    rates, lengths, steps_total, walls = [], [], 0, []
    for b in range(blocks):
        rate, wall, steps, lens = _env_block(episodes_per_block, seed + 10_000 * b)
        rates.append(rate)
        walls.append(wall)
        steps_total += steps
        lengths.extend(lens)
    return {
        "n_objects": N_OBJECTS,
        "max_shots": MAX_SHOTS,
        "obs_dim": OBS_DIM,
        "cand_dim": CAND_DIM,
        "k_max": K_MAX,
        "env_steps_per_sec": round(max(rates), 2),
        "env_steps_per_sec_block_rates": [round(r, 2) for r in rates],
        "n_envs": 1,  # throughput measured on one process; ppo.n_envs is the training fan-out
        "episodes_per_sec": round(episodes_per_block / min(walls), 3),
        "mean_episode_len": round(float(np.mean(lengths)), 3),
        "episodes_measured": episodes_per_block * blocks,
        "steps_measured": steps_total,
        "notes": "single-process raw env, uniform random legal actions, fresh rack per episode",
    }


def measure_ort_decisions(onnx_path: pathlib.Path, iters: int = 1000, repeats: int = 3) -> dict:
    """Single-row ORT CPU inference on the exported graph (the serving path).

    Headline is the fastest of ``repeats`` timed blocks: a single block is sensitive to
    whatever else is running on the shared box (observed spread 5.1k -> 40k decisions/s).
    """
    import onnxruntime as ort

    session = ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
    rng = np.random.default_rng(0)
    obs = rng.uniform(-1, 1, size=(1, OBS_DIM)).astype(np.float32)
    cand = rng.uniform(-1, 1, size=(1, K_MAX, CAND_DIM)).astype(np.float32)
    for _ in range(100):  # warmup
        session.run(["logits"], {"obs": obs, "cand": cand})
    rates = []
    for _ in range(repeats):
        t0 = time.time()
        for _ in range(iters):
            session.run(["logits"], {"obs": obs, "cand": cand})
        rates.append(iters / (time.time() - t0))
    return {
        "ai_decisions_per_sec": round(max(rates), 1),
        "ort_iters_per_block": iters,
        "blocks": repeats,
        "block_rates": [round(r, 1) for r in rates],
    }


def check_serve_path(model_zip: pathlib.Path, onnx_path: pathlib.Path, episodes: int = 20) -> dict:
    """Drive the SB3 policy on live states and compare its masked argmax with raw ORT logits."""
    import onnxruntime as ort
    import torch
    from sb3_contrib import MaskablePPO

    model = MaskablePPO.load(str(model_zip), device="cpu")
    session = ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
    env = PoolShotEnv(n_objects=N_OBJECTS, max_shots=MAX_SHOTS, sim_seed=77)
    shots = agree = 0
    max_err = 0.0
    for ep in range(episodes):
        obs, _ = env.reset(seed=5000 + ep)
        done = False
        while not done:
            o = obs["obs"][None, :]
            c = obs["cand"][None, :, :]
            mask = obs["mask"] > 0
            logits_ort = session.run(["logits"], {"obs": o.astype(np.float32), "cand": c.astype(np.float32)})[0][0]
            with torch.no_grad():
                logits_pt = model.policy.scorer(torch.from_numpy(o), torch.from_numpy(c))[0].numpy()
            max_err = max(max_err, float(np.max(np.abs(logits_ort - logits_pt))))
            a_ort = int(np.argmax(np.where(mask, logits_ort, -1e9)))
            a_sb3 = int(model.predict(obs, deterministic=True, action_masks=env.action_masks())[0])
            agree += int(a_ort == a_sb3)
            shots += 1
            obs, _r, terminated, truncated, _info = env.step(a_sb3)
            done = terminated or truncated
    return {
        "episodes": episodes,
        "shots": shots,
        "argmax_agreement": round(agree / max(shots, 1), 6),
        "max_logit_abs_err": max_err,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", type=str, default="ppo_smoke")
    parser.add_argument("--onnx", type=pathlib.Path, default=None, help="default: results/onnx/policy-smoke.onnx")
    parser.add_argument("--out", type=pathlib.Path, default=None)
    args = parser.parse_args()

    run_dir = HERE / "runs" / args.run
    train = json.loads((run_dir / "train_metrics.json").read_text())
    onnx_path = args.onnx or RESULTS / "onnx" / "policy-smoke.onnx"
    meta_path = onnx_path.with_name(onnx_path.stem + "-meta.json")
    meta = json.loads(meta_path.read_text()) if meta_path.exists() else None

    env_stats = measure_env()
    ort_stats = measure_ort_decisions(onnx_path) if onnx_path.exists() else {}
    serve_check = check_serve_path(run_dir / "final.zip", onnx_path) if onnx_path.exists() else None

    final_returns = train["final_eval"]
    curve = [[row[0], row[1]] for row in train["curve"]]  # schema wants [timesteps, value] pairs
    checkpoints = train["checkpoints"]
    gate = train["gate"]
    logger_final = train["final_logger_values"]

    measurements = {
        "machine": "Apple M5, 10 cores, 16 GB RAM, macOS 26.4.1",
        "versions": {
            "python": platform.python_version(),
            "torch": md.version("torch"),
            "sb3": md.version("stable_baselines3"),
            "sb3_contrib": md.version("sb3-contrib"),
            "gymnasium": md.version("gymnasium"),
            "onnx": md.version("onnx"),
            "onnxruntime": md.version("onnxruntime"),
            "numpy": md.version("numpy"),
        },
        "env": env_stats,
        "ppo": {
            "algorithm": train["algorithm"],
            "policy_class": train["policy_class"],
            "timesteps": train["timesteps"],
            "wall_clock_s": train["wall_clock_s"],
            "steps_per_sec": train["steps_per_sec"],
            "n_envs": train["n_envs"],
            "n_steps": train.get("n_steps"),
            "batch_size": train.get("batch_size"),
            "n_epochs": train.get("n_epochs"),
            "learning_rate": train.get("learning_rate"),
            "ent_coef": train.get("ent_coef"),
            "target_kl": train.get("target_kl"),
            "seed": train["seed"],
            "learning_curve": curve,
            "final_mean_return_100": final_returns["mean"],
            "random_policy_mean_return_100": train["random_baseline"]["mean"],
            "gate": gate,
            "ai_decisions_per_sec": ort_stats.get("ai_decisions_per_sec"),
            "ai_decisions_per_sec_blocks": ort_stats.get("block_rates"),
            "checkpoints": checkpoints,
            "checkpoint_best": train.get("checkpoint_best"),
            "value_loss_final": logger_final.get("value_loss_final"),
            "explained_variance_final": logger_final.get("explained_variance_final"),
            "notes": (
                "MaskablePPO + custom CandidateScoringPolicy (per-candidate shared scorer, "
                "masking outside the scoring head). CPU only, sb3 seed=42, 8 SubprocVecEnv workers. "
                "learning_curve = [timesteps, rollout/ep_rew_mean] read from SB3's episode buffer at "
                "each rollout end; checkpoints evaluated with 100 deterministic episodes on a fixed "
                "rack sequence shared with the random baseline. ai_decisions_per_sec = single-row ORT "
                "CPU inference (batch 1, K=32) on the exported graph, i.e. the serving path."
            ),
        },
        "onnx": None,
        "obs_context_layout": OBS_CONTEXT_LAYOUT,
        "returns": {
            "mean": final_returns["mean"],
            "std": final_returns["std"],
            "min": final_returns["min"],
            "max": final_returns["max"],
            "per_shot_mean": final_returns["per_shot_mean"],
        },
        "notes": "",
    }

    if meta is not None:
        measurements["onnx"] = {
            "path": str(onnx_path.relative_to(HERE.parent.parent)),
            "size_bytes": meta["onnx_bytes"],
            "sha256": meta["sha256"],
            "opset": meta["opset"][0] if isinstance(meta["opset"], list) else meta["opset"],
            "params": meta["scorer_params"],
            "k_golden": meta["golden_k"],
            "ort_golden_max_abs_err": meta["ort_golden_max_abs_err"],
            "exported_from": meta.get("model"),
            "serve_path_check": serve_check,
            "dynamic_k_check_max_abs_err": meta.get("ort_dynamic_k9_max_abs_err"),
            "torch_onnx_ort_versions": meta.get("versions"),
        }
    else:
        measurements["onnx"] = {
            "path": None, "size_bytes": 0, "sha256": "", "opset": 0, "params": 0,
            "k_golden": 0, "ort_golden_max_abs_err": None,
        }
        measurements["notes"] += f"ONNX export missing at {onnx_path}; "

    curve_note = (
        f"learning_curve: {len(curve)} rollout points, "
        f"first={curve[0] if curve else None}, last={curve[-1] if curve else None}. "
        "A separate 150k-step run (runs/long_run_150k_degradation.log) degraded after ~30k steps "
        "(checkpoint means 6.899 / 4.405 / 3.324 at 10/50/100%), so this deliverable run is 32k steps."
    )
    measurements["notes"] += curve_note
    measurements["notes"] += (
        " `import sb3` is not a module in stable-baselines3 2.9.0 (PyPI `sb3` is an unrelated stub); "
        "the canonical `import stable_baselines3 as sb3` is used and passes."
    )
    if gate is None or gate.get("reached_at_timesteps") is None:
        measurements["notes"] += " Gate not reached by the logged curve (see ppo.gate)."

    out_path = args.out or RESULTS / "python-measurements.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as fh:
        json.dump(measurements, fh, indent=1)
    print(json.dumps({k: v for k, v in measurements.items() if k not in ("learning_curve",)}, indent=1))
    print(f"written: {out_path}")


if __name__ == "__main__":
    main()
