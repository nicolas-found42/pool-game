#!/usr/bin/env python3
"""Differential check against pooltool on synthetic shots (ladder pre-stage).

Runs the same four synthetic shots through pooltool with the spec's own
coefficients pushed in explicitly (pooltool's defaults do not match the spec:
its table is a 7 ft Showood, u_b 0.05, e_c 0.85, g 9.81), maps the results back
into the spec's frame, and writes out/pooltool-diff.json for comparison with
the Rust prototype's `diff` subcommand.

    ~/pool-game-data/pooltool-venv/bin/python prototypes/physics-core/tools/pooltool_diff.py \
        --out prototypes/physics-core/out
"""

from __future__ import annotations

import argparse
import json
import sys

R = 0.028575
FRAME = 0.635  # pooltool x = 635 + ours y ; pooltool y = 1270 + ours x


def to_pt(x_mm: float, y_mm: float) -> tuple[float, float]:
    return (FRAME + y_mm / 1000.0, 2 * FRAME + x_mm / 1000.0)


def to_ours(x_m: float, y_m: float) -> tuple[float, float]:
    return ((y_m - 2 * FRAME) * 1000.0, (x_m - FRAME) * 1000.0)


def rack_positions() -> list[tuple[str, float, float]]:
    """The prototype's preset rack (rack-fixtures seed 1), in the spec frame."""
    slots = {
        "1": "1.0", "2": "4.1", "3": "3.2", "4": "2.0", "5": "5.2", "6": "5.3",
        "7": "5.4", "8": "3.1", "9": "2.1", "10": "5.1", "11": "4.0",
        "12": "4.3", "13": "4.2", "14": "5.0", "15": "3.0",
    }
    sqrt3 = 1.7320508075688772
    out = []
    for ball, slot in slots.items():
        r, k = (int(v) for v in slot.split("."))
        x = 635.0 + (r - 1) * sqrt3 * 28.575
        y = (k - (r - 1) / 2.0) * 2 * 28.575
        out.append((ball, x, y))
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="out")
    ap.add_argument("--break-speed", type=float, default=6.2)
    args = ap.parse_args(argv)

    import numpy as np
    import pooltool as pt
    from pooltool.objects import BallSet, PocketTableSpecs, Table

    table = Table.from_table_specs(PocketTableSpecs(l=2.540, w=1.270))
    ballset = BallSet("pooltool_pocket")

    # spec profile pushed in explicitly (#7 §5); BallParams is frozen attrs,
    # so build a new instance from the defaults with the spec's values.
    import attrs
    base = pt.BallParams.default()
    overrides = dict(u_s=0.20, u_r=0.010, u_b=0.06, e_b=0.95, e_c=0.75,
                     f_c=0.20, g=9.80665, m=0.170, R=0.028575)
    fields = {f.name for f in attrs.fields(type(base))}
    print("BallParams fields:", sorted(fields))
    params = attrs.evolve(base, **{k: v for k, v in overrides.items() if k in fields})
    print("params in force:", {k: getattr(params, k) for k in sorted(overrides) if k in fields})

    results = {"engine": "pooltool", "version": pt.__version__,
               "table_mm": [table.w * 1000, table.l * 1000],
               "params": {k: getattr(params, k) for k in sorted(overrides) if k in fields},
               "shots": []}

    def run(name, balls, cue_state, note=""):
        for b in balls.values():
            b.params = params
        system = pt.System(cue=pt.Cue.default(), table=table, balls=balls)
        system.cue.set_state(**cue_state)
        pt.simulate(system, inplace=True)
        rest = {}
        for bid, b in system.balls.items():
            x, y = to_ours(float(b.state.rvw[0][0]), float(b.state.rvw[0][1]))
            rest[bid] = {
                "x_mm": x, "y_mm": y,
                "speed": float(np.linalg.norm(b.state.rvw[1])),
            }
        pocketed = sorted(bid for bid, b in system.balls.items() if b.state.s == 4)
        ev = [e.event_type.value for e in system.events]
        bb = [e for e in system.events if e.event_type.value == "ball_ball"]
        cushion = [e for e in system.events if "cushion" in e.event_type.value]
        results["shots"].append({
            "name": name, "note": note, "t_rest": float(system.t),
            "events": len(system.events), "ball_ball": len(bb),
            "cushion": len(cushion), "pocketed": pocketed, "rest": rest,
            "event_types": {k: ev.count(k) for k in sorted(set(ev))},
        })
        if name.startswith("straight"):
            for e in system.events[:14]:
                print("   event", round(e.time, 5), e.event_type.value,
                      [a.id for a in e.agents])
        print(f"{name}: t={system.t:.3f}s events={len(system.events)} "
              f"ball_ball={len(bb)} cushion={len(cushion)} pocketed={pocketed}")
        for bid in sorted(rest):
            print(f"   {bid:>3} ({rest[bid]['x_mm']:8.2f}, {rest[bid]['y_mm']:8.2f})")

    # 1. straight stun
    balls = {
        "cue": pt.Ball.create("cue", xy=to_pt(0.0, 0.0), ballset=ballset),
        "1": pt.Ball.create("1", xy=to_pt(200.0, 0.0), ballset=ballset),
    }
    run("straight_stun_1p4", balls,
        dict(V0=1.4, phi=90.0, theta=0.0, a=0.0, b=0.0),
        note="cue -> object head-on, 1.4 m/s, stun")

    # 2. 30 degree cut with follow (offset one ball radius)
    balls = {
        "cue": pt.Ball.create("cue", xy=to_pt(0.0, 0.0), ballset=ballset),
        "1": pt.Ball.create("1", xy=to_pt(200.0, 28.575), ballset=ballset),
    }
    run("cut30_follow_1p4", balls,
        dict(V0=1.4, phi=90.0, theta=0.0, a=0.0, b=0.5),
        note="30 deg cut, follow 0.5R")

    # 3. rolling ball into the long cushion, measure the rebound ratio
    balls = {"cue": pt.Ball.create("cue", xy=to_pt(0.0, 400.0), ballset=ballset)}
    run("cushion_roll_1p5", balls,
        dict(V0=1.5, phi=90.0, theta=0.0, a=0.0, b=0.4),
        note="rolling ball square into the far cushion")

    # 4. the rack break
    balls = {"cue": pt.Ball.create("cue", xy=to_pt(-800.0, 0.0), ballset=ballset)}
    for ball, x, y in rack_positions():
        balls[ball] = pt.Ball.create(ball, xy=to_pt(x, y), ballset=ballset)
    run("break_rack", balls,
        dict(V0=args.break_speed, phi=90.0, theta=0.0, a=0.0, b=0.0),
        note=f"full rack break at {args.break_speed} m/s, level, centre")

    with open(f"{args.out}/pooltool-diff.json", "w") as sink:
        json.dump(results, sink, indent=1)
    print(f"wrote {args.out}/pooltool-diff.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
