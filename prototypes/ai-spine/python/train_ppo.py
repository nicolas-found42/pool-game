"""SB3 MaskablePPO smoke run on the pool shot env.

    python train_ppo.py --timesteps 120000 --n-envs 8 --seed 42

Trains the "N-ball open table" drill (default 3 object balls, 8 shots) on CPU
only. Everything is seeded; SB3's ``seed=`` covers torch/numpy/action sampling,
the envs get ``sim_seed=seed+rank`` and each evaluation episode uses a fixed
rack (``reset(seed=eval_seed + episode)``) so the random baseline, the
checkpoint evaluations and the final evaluation all see the SAME rack sequence.

Outputs (under ``runs/<run-name>/``):
  best.zip / final.zip            trained checkpoints (SB3)
  ckpt_{010,050,100}.zip          checkpoint fractions of the run
  train_metrics.json              curve, checkpoints, evaluations, gate
"""

from __future__ import annotations

import argparse
import json
import pathlib
import time

import numpy as np
from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.utils import safe_mean
from stable_baselines3.common.vec_env import DummyVecEnv, SubprocVecEnv, VecMonitor

from env import MAX_SHOTS, N_OBJECTS, PoolShotEnv
from model import CandidateScoringPolicy

HERE = pathlib.Path(__file__).resolve().parent
GATE_MARGIN = 0.5  # fixed before the run: random baseline + this margin


def _f(value):
    """Logger values are numpy float32; JSON wants plain floats."""
    return None if value is None else float(value)


def make_env(rank: int, seed: int, n_objects: int, max_shots: int):
    def _init():
        env = PoolShotEnv(n_objects=n_objects, max_shots=max_shots, sim_seed=seed + rank)
        env.reset(seed=seed + rank)
        return env

    return _init


def evaluate_random(n_objects: int, max_shots: int, n_episodes: int, seed: int) -> list[float]:
    """Uniform random legal action per shot, one fixed rack per episode."""
    env = PoolShotEnv(n_objects=n_objects, max_shots=max_shots, sim_seed=seed)
    rng = np.random.default_rng(seed)
    returns: list[float] = []
    for ep in range(n_episodes):
        obs, _ = env.reset(seed=seed + ep)
        total, done = 0.0, False
        while not done:
            action = int(rng.choice(np.flatnonzero(env.action_masks())))
            obs, reward, terminated, truncated, _info = env.step(action)
            total += reward
            done = terminated or truncated
        returns.append(total)
    return returns


def evaluate_model(
    model: MaskablePPO, n_objects: int, max_shots: int, n_episodes: int, seed: int, deterministic: bool = True
) -> list[float]:
    """Greedy-masked policy over the same fixed rack sequence as the random baseline."""
    env = PoolShotEnv(n_objects=n_objects, max_shots=max_shots, sim_seed=seed)
    returns: list[float] = []
    for ep in range(n_episodes):
        obs, _ = env.reset(seed=seed + ep)
        total, done = 0.0, False
        while not done:
            action, _ = model.predict(obs, deterministic=deterministic, action_masks=env.action_masks())
            obs, reward, terminated, truncated, _info = env.step(int(action))
            total += reward
            done = terminated or truncated
        returns.append(total)
    return returns


def return_stats(returns: list[float], max_shots: int) -> dict:
    arr = np.asarray(returns, dtype=np.float64)
    return {
        "mean": float(arr.mean()),
        "std": float(arr.std()),
        "min": float(arr.min()),
        "max": float(arr.max()),
        "per_shot_mean": float(arr.mean() / max_shots),
    }


class MetricsCallback(BaseCallback):
    """Per-rollout curve + checkpoint fractions 10/50/100% with 100-episode evals."""

    def __init__(self, run_dir: pathlib.Path, total_timesteps: int, eval_kwargs: dict, eval_episodes: int, verbose: int = 0):
        super().__init__(verbose)
        self.run_dir = run_dir
        self.total_timesteps = total_timesteps
        self.eval_kwargs = eval_kwargs
        self.eval_episodes = eval_episodes
        self.curve: list[list[float]] = []
        self.checkpoints: list[dict] = []
        self.t_start = time.time()
        self._milestones = [0.10, 0.50, 1.00]
        self._done_milestones: set[float] = set()

    def _on_step(self) -> bool:
        return True

    def _on_rollout_end(self) -> None:
        """SB3 clears ``logger.name_to_value`` on every dump, so the curve is read from
        ``ep_info_buffer`` -- the same source SB3 itself uses for ``rollout/ep_rew_mean``."""
        t = int(self.model.num_timesteps)
        values = self.model.logger.name_to_value
        ep_rew = values.get("rollout/ep_rew_mean")
        if ep_rew is None and len(self.model.ep_info_buffer) > 0:
            ep_rew = safe_mean([ep_info["r"] for ep_info in self.model.ep_info_buffer])
        if ep_rew is not None:
            self.curve.append([t, float(ep_rew), round(time.time() - self.t_start, 3)])
        for frac in self._milestones:
            if frac in self._done_milestones or t < frac * self.total_timesteps:
                continue
            self._done_milestones.add(frac)
            stamp = f"{int(round(frac * 100)):03d}"
            self.model.save(self.run_dir / f"ckpt_{stamp}.zip")
            t_eval = time.time()
            returns = evaluate_model(self.model, **self.eval_kwargs, n_episodes=self.eval_episodes)
            self.checkpoints.append(
                {
                    "fraction": frac,
                    "timesteps": t,
                    "mean_return_100": float(np.mean(returns)),
                    "wall_clock_s": round(time.time() - self.t_start, 3),
                    "eval_wall_clock_s": round(time.time() - t_eval, 3),
                }
            )
            if self.verbose:
                print(f"[ckpt {stamp}] t={t} mean_return_100={np.mean(returns):.3f}", flush=True)

    def final_logger_values(self) -> dict:
        values = self.model.logger.name_to_value
        return {
            "value_loss_final": _f(values.get("train/value_loss")),
            "explained_variance_final": _f(values.get("train/explained_variance")),
            "entropy_loss_final": _f(values.get("train/entropy_loss")),
            "approx_kl_final": _f(values.get("train/approx_kl")),
            "ep_rew_mean_final": _f(values.get("rollout/ep_rew_mean")),
        }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timesteps", type=int, default=120_000)
    parser.add_argument("--n-envs", type=int, default=8)
    parser.add_argument("--n-objects", type=int, default=N_OBJECTS)
    parser.add_argument("--max-shots", type=int, default=MAX_SHOTS)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--n-steps", type=int, default=64)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--ent-coef", type=float, default=0.01)
    parser.add_argument("--vf-coef", type=float, default=0.5)
    parser.add_argument("--target-kl", type=float, default=None)
    parser.add_argument("--n-epochs", type=int, default=10)
    parser.add_argument("--eval-episodes", type=int, default=100)
    parser.add_argument("--start-method", choices=["spawn", "fork", "dummy"], default="spawn")
    parser.add_argument("--run-name", type=str, default="ppo_smoke")
    parser.add_argument("--device", type=str, default="cpu")
    args = parser.parse_args()

    run_dir = HERE / "runs" / args.run_name
    run_dir.mkdir(parents=True, exist_ok=True)
    fns = [make_env(rank, args.seed, args.n_objects, args.max_shots) for rank in range(args.n_envs)]
    if args.start_method == "dummy" or args.n_envs == 1:
        vec_env = DummyVecEnv(fns)
    else:
        vec_env = SubprocVecEnv(fns, start_method=args.start_method)
    # SB3 only auto-wraps environments it vectorises itself: an already-vectorised env
    # must be wrapped in VecMonitor by hand or no episode statistics are collected.
    vec_env = VecMonitor(vec_env)

    eval_kwargs = {"n_objects": args.n_objects, "max_shots": args.max_shots, "seed": 900_000}

    t0 = time.time()
    random_returns = evaluate_random(args.n_objects, args.max_shots, args.eval_episodes, seed=900_000)
    random_stats = return_stats(random_returns, args.max_shots)
    eval_baseline_s = time.time() - t0
    print(f"random baseline: mean={random_stats['mean']:.3f} std={random_stats['std']:.3f} ({eval_baseline_s:.1f}s)", flush=True)

    model = MaskablePPO(
        CandidateScoringPolicy,
        vec_env,
        n_steps=args.n_steps,
        batch_size=args.batch_size,
        learning_rate=args.lr,
        ent_coef=args.ent_coef,
        gamma=0.99,
        gae_lambda=0.95,
        vf_coef=args.vf_coef,
        target_kl=args.target_kl,
        n_epochs=args.n_epochs,
        max_grad_norm=0.5,
        seed=args.seed,
        device=args.device,
        verbose=1,
    )
    callback = MetricsCallback(run_dir, args.timesteps, eval_kwargs, args.eval_episodes, verbose=1)
    model.learn(total_timesteps=args.timesteps, callback=callback, progress_bar=False)
    train_wall = time.time() - t0 - eval_baseline_s
    model.save(run_dir / "final")

    final_returns = evaluate_model(model, **eval_kwargs, n_episodes=args.eval_episodes)
    final_stats = return_stats(final_returns, args.max_shots)
    best = max(callback.checkpoints, key=lambda c: c["mean_return_100"]) if callback.checkpoints else None

    # gate: fixed rule, applied to the logged curve after the fact
    threshold = random_stats["mean"] + GATE_MARGIN
    gate_ts, gate_wall = None, None
    streak = 0
    for row in callback.curve:
        streak = streak + 1 if row[1] >= threshold else 0
        if streak >= 3:
            gate_ts, gate_wall = row[0], row[2]
            break

    metrics = {
        "algorithm": f"MaskablePPO(sb3-contrib {_version('sb3_contrib')})",
        "policy_class": type(model.policy).__name__,
        "timesteps": int(model.num_timesteps),
        "wall_clock_s": round(train_wall, 3),
        "steps_per_sec": round(model.num_timesteps / train_wall, 2),
        "n_envs": args.n_envs,
        "env_start_method": args.start_method,
        "n_steps": args.n_steps,
        "batch_size": args.batch_size,
        "n_epochs": args.n_epochs,
        "learning_rate": args.lr,
        "ent_coef": args.ent_coef,
        "vf_coef": args.vf_coef,
        "target_kl": args.target_kl,
        "seed": args.seed,
        "curve": callback.curve,  # [timesteps, rollout/ep_rew_mean, wall_clock_s] -- SB3's ep buffer
        "checkpoints": callback.checkpoints,
        "random_baseline": {"episodes": args.eval_episodes, "mean": random_stats["mean"], "std": random_stats["std"], "wall_clock_s": round(eval_baseline_s, 3)},
        "final_eval": {"episodes": args.eval_episodes, "deterministic": True, **final_stats},
        "gate": {
            "name": f"rollout/ep_rew_mean >= random_baseline_mean + {GATE_MARGIN} for 3 consecutive rollouts",
            "threshold": threshold,
            "reached_at_timesteps": gate_ts,
            "wall_clock_to_gate_s": gate_wall,
        },
        "final_logger_values": callback.final_logger_values(),
        "checkpoint_best": {k: best[k] for k in ("fraction", "timesteps", "mean_return_100")} if best else None,
    }
    with (run_dir / "train_metrics.json").open("w") as fh:
        json.dump(metrics, fh, indent=1)
    vec_env.close()
    print(json.dumps({k: v for k, v in metrics.items() if k != "curve"}, indent=1))
    print(f"curve points: {len(callback.curve)} (written to {run_dir / 'train_metrics.json'})")


def _version(pkg: str) -> str:
    import importlib.metadata as md

    return md.version(pkg)


if __name__ == "__main__":
    main()
