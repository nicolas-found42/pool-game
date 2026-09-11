#!/usr/bin/env python3
"""Run scripted shots through pooltool and print the observed result.

pooltool (https://github.com/ekiefl/pooltool, JOSS 10.21105/joss.07301) is the
differential oracle for the pre-fit stage of the fitting ladder: an independent,
peer-reviewed implementation of the same event-driven analytic model, so our
engine can be diffed against it on synthetic shots before touching real tables.

Usage:
    ~/pool-game-data/pooltool-venv/bin/python tools/benchmark/pooltool_smoke.py
    ~/pool-game-data/pooltool-venv/bin/python tools/benchmark/pooltool_smoke.py --csv /tmp/cue.csv

Prints the pooltool version, the physics parameters actually in force, the shot
declarations, the event log, and the resting state of every ball. Exits non-zero
if a shot produced no events (i.e. pooltool did not run).
"""

from __future__ import annotations

import argparse
import json
import sys

# Straight-in stun, then a half-ball cut with follow -- one shot per ball-ball
# behaviour the collision stage of the ladder is gated on. `offset` displaces
# the object ball across the aim line: offset = R is the half-ball hit (30 deg
# cut), which is where cut-induced throw peaks.
SHOTS = [
    {"name": "straight_stun", "V0": 1.40, "phi": 90.0, "theta": 0.0,
     "a": 0.0, "b": 0.0, "gap": 0.20, "offset": 0.0},
    {"name": "half_ball_follow", "V0": 1.40, "phi": 90.0, "theta": 0.0,
     "a": 0.0, "b": 0.5, "gap": 0.15, "offset": 0.028575},
]


def departure_angle(system, ball_id: str, t0: float, dt: float = 0.002) -> float | None:
    """Direction a ball travels just after t0, in degrees from the aim line (+y)."""
    import math

    import pooltool as pt

    ball = system.balls[ball_id]
    states = pt.interpolate_ball_states(ball, [t0 + dt, t0 + 10 * dt])
    start, end = (state.rvw[0] for state in states)
    dx, dy = end[0] - start[0], end[1] - start[1]
    if math.hypot(dx, dy) < 1e-9:
        return None
    return math.degrees(math.atan2(dx, dy))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--csv", help="write the cue ball trajectory of the last shot here")
    parser.add_argument("--dt", type=float, default=0.001,
                        help="continuized sampling step for --csv (default 1 ms)")
    parser.add_argument("--table", choices=("default", "nine-foot"),
                        default="default",
                        help="pooltool ships a 7-foot Showood only; 'nine-foot' "
                             "builds the spec's 2540x1270 mm geometry explicitly")
    args = parser.parse_args(argv)

    import numpy as np
    import pooltool as pt
    from pooltool.objects import BallSet

    print(f"pooltool version: {pt.__version__}")
    print(f"python: {sys.version.split()[0]}")
    if args.table == "nine-foot":
        from pooltool.objects import PocketTableSpecs, Table

        specs = PocketTableSpecs(l=2.540, w=1.270)
        table = Table.from_table_specs(specs)
    else:
        table = pt.Table.default()
    ballset = BallSet("pooltool_pocket")
    print(f"table: {table.table_type} playing surface "
          f"{table.w * 1000:.1f} x {table.l * 1000:.1f} mm")
    print(f"cushion height: {table.cushion_segments.linear['3'].height * 1000:.3f} mm "
          f"({table.cushion_segments.linear['3'].height / 0.028575 / 2 * 100:.2f} % "
          f"of ball diameter)")

    failures = 0
    last_system = None
    for shot in SHOTS:
        params = pt.BallParams.default()
        print(f"\n=== shot {shot['name']} ===")
        print(f"ball params: R={params.R * 1000:.3f} mm m={params.m * 1000:.1f} g "
              f"u_s={params.u_s} u_r={params.u_r} "
              f"u_sp_proportionality={params.u_sp_proportionality} u_b={params.u_b} "
              f"e_b={params.e_b} e_c={params.e_c} f_c={params.f_c} g={params.g}")

        system = pt.System(
            cue=pt.Cue.default(),
            table=table,
            balls={
                "cue": pt.Ball.create("cue", xy=(table.w / 2, table.l / 2), ballset=ballset),
                "1": pt.Ball.create("1", xy=(table.w / 2, table.l / 2), ballset=ballset),
            },
        )
        system.balls["cue"].state.rvw[0] = [table.w / 2,
                                            table.l / 2 - shot["gap"] / 2, params.R]
        system.balls["1"].state.rvw[0] = [table.w / 2 + shot["offset"],
                                          table.l / 2 + shot["gap"] / 2, params.R]
        system.cue.set_state(V0=shot["V0"], phi=shot["phi"], theta=shot["theta"],
                             a=shot["a"], b=shot["b"])
        print("strike declaration: " + json.dumps(
            {k: shot[k] for k in ("V0", "phi", "theta", "a", "b")})
            + f"  object-ball lateral offset: {shot['offset'] * 1000:.3f} mm")

        pt.simulate(system, inplace=True)
        last_system = system
        print(f"simulated duration: {system.t:.4f} s, events: {len(system.events)}")
        for event in system.events[:12]:
            print(f"  t={event.time:9.6f}s  {event.event_type.value:<24} "
                  f"{[agent.id for agent in event.agents]}")
        if len(system.events) > 12:
            print(f"  ... {len(system.events) - 12} more")

        collisions = [e for e in system.events
                      if e.event_type == pt.EventType.BALL_BALL]
        if collisions:
            t_bb = collisions[0].time
            # The geometry-only answer for a cut at impact parameter p is
            # asin(p / 2R); anything beyond that is throw.
            p = min(abs(shot["offset"]), 2 * params.R)
            geometry = np.degrees(np.arcsin(p / (2 * params.R)))
            ob_angle = departure_angle(system, "1", t_bb)
            print(f"first ball-ball at t={t_bb:.6f}s; geometry-only OB cut angle "
                  f"= {geometry:.4f} deg")
            if ob_angle is not None:
                print(f"observed OB departure = {ob_angle:.4f} deg "
                      f"(cut-induced throw = {ob_angle - geometry:+.4f} deg)")
            cue_angle = departure_angle(system, "cue", t_bb)
            if cue_angle is not None:
                print(f"observed CB departure = {cue_angle:.4f} deg "
                      f"(tangent line = 90 deg for a stun, less for follow)")

        print("rest state:")
        for ball_id, ball in system.balls.items():
            x, y, z = ball.state.rvw[0]
            speed = float(np.linalg.norm(ball.state.rvw[1]))
            spin = float(np.linalg.norm(ball.state.rvw[2]))
            print(f"  {ball_id:>3}: pos=({x * 1000:8.3f}, {y * 1000:8.3f}, "
                  f"{z * 1000:6.3f}) mm  |v|={speed:.6f} m/s  "
                  f"|w|={spin:.6f} rad/s  state={ball.state.s}")
        if not len(system.events):
            failures += 1

    if args.csv and last_system is not None:
        cue = last_system.balls["cue"]
        times = np.arange(0.0, last_system.t + args.dt, args.dt)
        states = pt.interpolate_ball_states(cue, times, extrapolate=True)
        with open(args.csv, "w") as sink:
            sink.write("t,x_mm,y_mm,z_mm\n")
            for t, state in zip(times, states):
                x, y, z = state.rvw[0]
                sink.write(f"{t:.6f},{x * 1000:.4f},{y * 1000:.4f},{z * 1000:.4f}\n")
        print(f"\nwrote {len(times)} cue-ball samples to {args.csv}")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
