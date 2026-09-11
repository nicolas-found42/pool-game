"""Export the candidate scorer to ONNX + write the cross-language golden vectors.

Usage
-----
    # early unblock artifact: architecturally final, randomly initialised
    python export_onnx.py --untrained

    # trained artifact (after train_ppo.py)
    python export_onnx.py --checkpoint runs/ppo_smoke/best.zip

Graph contract (mirrored by the Rust/ORT serving path):
    inputs : obs  float32 [1, 64], cand float32 [1, K, 16]  (K dynamic)
    output : logits float32 [1, K]   RAW logits, masking applied by the caller
    opset  : 17

Also writes, next to the golden json, ``<name>-meta.json`` with the model
identity (sha256, param count, opset, k, torch/onnx/onnxruntime versions).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import time

import numpy as np
import onnx
import onnxruntime as ort
import torch

from model import CAND_DIM, OBS_DIM, ScoringNet

RESULTS = pathlib.Path(__file__).resolve().parent.parent / "results"
ONNX_DIR = RESULTS / "onnx"
GOLDEN_PATH = RESULTS / "onnx-golden.json"
OPSET = 17
GOLDEN_K = 5
GOLDEN_SEED = 20260911


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def build_net(checkpoint: pathlib.Path | None) -> tuple[ScoringNet, str]:
    """Deterministically initialised net, or the scorer lifted out of an SB3 checkpoint."""
    torch.manual_seed(0)
    net = ScoringNet(OBS_DIM, CAND_DIM)
    if checkpoint is None:
        return net.eval(), "untrained-random-init (torch.manual_seed(0))"
    from sb3_contrib import MaskablePPO

    model = MaskablePPO.load(str(checkpoint), device="cpu")
    net.load_state_dict(model.policy.scorer.state_dict())
    net.eval()
    return net, f"trained-scorer-from:{checkpoint}"


def export(net: ScoringNet, out_path: pathlib.Path) -> None:
    dummy_obs = torch.zeros(1, OBS_DIM, dtype=torch.float32)
    dummy_cand = torch.zeros(1, GOLDEN_K, CAND_DIM, dtype=torch.float32)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    torch.onnx.export(
        net,
        (dummy_obs, dummy_cand),
        str(out_path),
        input_names=["obs", "cand"],
        output_names=["logits"],
        dynamic_axes={"obs": {0: "batch"}, "cand": {0: "batch", 1: "K"}, "logits": {0: "batch", 1: "K"}},
        opset_version=OPSET,
        do_constant_folding=True,
        dynamo=False,  # legacy TorchScript exporter: minimal, deterministic graph
    )


def golden_check(net: ScoringNet, onnx_path: pathlib.Path) -> dict:
    rng = np.random.default_rng(GOLDEN_SEED)
    obs = rng.uniform(-1.0, 1.0, size=(1, OBS_DIM)).astype(np.float32)
    cand = rng.uniform(-1.0, 1.0, size=(1, GOLDEN_K, CAND_DIM)).astype(np.float32)

    with torch.no_grad():
        expected = net(torch.from_numpy(obs), torch.from_numpy(cand)).numpy()

    session = ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
    got = session.run(["logits"], {"obs": obs, "cand": cand})[0]
    max_abs_err = float(np.max(np.abs(expected - got)))

    golden = {
        "obs": [float(v) for v in obs[0]],
        "cand": [[float(v) for v in row] for row in cand[0]],
        "expected_logits": [float(v) for v in expected[0]],
        "max_abs_err_pytorch_vs_ort": max_abs_err,
        "opset": OPSET,
        "k": GOLDEN_K,
    }
    # A second, non-square K proves the axis really is dynamic.
    cand_dyn = rng.uniform(-1.0, 1.0, size=(1, 9, CAND_DIM)).astype(np.float32)
    got_dyn = session.run(["logits"], {"obs": obs, "cand": cand_dyn})[0]
    with torch.no_grad():
        exp_dyn = net(torch.from_numpy(obs), torch.from_numpy(cand_dyn)).numpy()
    golden_dyn_err = float(np.max(np.abs(exp_dyn - got_dyn)))
    return golden, golden_dyn_err


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=pathlib.Path, default=None, help="SB3 .zip checkpoint (trained export)")
    parser.add_argument("--untrained", action="store_true", help="export a randomly initialised net (early artifact)")
    parser.add_argument("--out", type=pathlib.Path, default=None)
    parser.add_argument("--golden", type=pathlib.Path, default=None, help="golden json path (default results/onnx-golden.json)")
    args = parser.parse_args()

    if args.untrained and args.checkpoint:
        parser.error("pass either --untrained or --checkpoint")

    if args.checkpoint:
        name, golden_path = "policy-smoke.onnx", args.golden or GOLDEN_PATH
    else:
        name, golden_path = "policy-scratch.onnx", args.golden or GOLDEN_PATH
    out_path = args.out or (ONNX_DIR / name)

    t0 = time.time()
    net, provenance = build_net(args.checkpoint)
    export(net, out_path)
    golden, golden_dyn_err = golden_check(net, out_path)
    golden_path.parent.mkdir(parents=True, exist_ok=True)
    with golden_path.open("w") as fh:
        json.dump(golden, fh)  # full float precision, no rounding

    digest = sha256_file(out_path)
    (out_path.with_suffix(out_path.suffix + ".sha256")).write_text(f"{digest}  {out_path.name}\n")

    onnx_model = onnx.load(str(out_path))
    meta = {
        "model": provenance,
        "onnx_path": str(out_path),
        "onnx_bytes": out_path.stat().st_size,
        "sha256": digest,
        "opset": [o.version for o in onnx_model.opset_import],
        "scorer_params": net.n_params(),
        "golden_path": str(golden_path),
        "golden_k": GOLDEN_K,
        "ort_golden_max_abs_err": golden["max_abs_err_pytorch_vs_ort"],
        "ort_dynamic_k9_max_abs_err": golden_dyn_err,
        "golden_dynamic_axis_ok": golden_dyn_err < 1e-5,
        "versions": {"torch": torch.__version__, "onnx": onnx.__version__, "onnxruntime": ort.__version__},
        "export_seconds": round(time.time() - t0, 3),
    }
    meta_path = out_path.with_name(out_path.stem + "-meta.json")
    with meta_path.open("w") as fh:
        json.dump(meta, fh, indent=1)
    print(json.dumps(meta, indent=1))


if __name__ == "__main__":
    main()
