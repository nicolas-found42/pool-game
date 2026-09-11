"""Gymnasium shot-granularity env for the pool AI-spine prototype (ticket #16).

Prototype quality by design: the simulator uses ONLY the spec constants from
physics resolution #7 §2/§5 (no spin, no throw, no squirt, no cushion spin
transfer, no fitted physics), and the env exposes exactly the cross-language
tensor contract the Rust half mirrors.

One env step = one shot
----------------------
``step(action)`` selects candidate ``action`` from the candidate list computed
for the current state, runs the analytic toy sim to rest, applies the reward and
returns the next state. Episodes are the "N billiard balls, open table" drill:
pocket all ``n_objects`` balls within ``max_shots`` shots.

Spec constants
--------------
Playing surface 2540 x 1270 mm, origin at table centre, x along the long axis.
Ball radius R = 28.575 mm, mass 170 g. Ball-ball restitution e = 0.95.
Cushion normal restitution e_n = 0.75. Cloth rolling deceleration magnitude
a = mu_r * g with mu_r = 0.010, g = 9806.65 mm/s^2  ->  a = 98.0665 mm/s^2.
Sleep threshold 1 mm/s. Pocket mouths: corner 115.9 mm, side 128.6 mm.

Pocket model (documented simplification, mirrored in Rust)
---------------------------------------------------------
* Mouth centres = the 6 points (+-L/2, +-W/2) and (0, +-W/2).
* Corner pocket: the mouth line joins the two jaws; it sits ``mouth/2`` inside
  the corner along the corner diagonal, i.e. on ``{q : q.d_c = c.d_c - mouth/2}``
  with ``c`` the corner point and ``d_c`` the unit diagonal. A ball is pocketed
  when its CENTRE crosses that line while its lateral coordinate
  ``|(q - c).t_c| <= mouth/2`` (``t_c`` perpendicular to ``d_c``).
* Side pocket: the mouth is a gap in the long rail of width 128.6 mm. A ball is
  pocketed when its centre reaches the rail line (|y| >= W/2 - R, i.e. rail
  contact) with ``|x| <= mouth/2``.
* No jaw rattle, no pocket-hang: a ball that stops short inside the jaws stays
  on the table. Balls that are pocketed leave the table via capture and are
  removed.

Toy sim
-------
Event-driven: every ball rolls in a straight line under constant deceleration
a. Per iteration the earliest event time is computed analytically from the
current state -- rail contact (quadratic in arc length), corner-mouth-line
crossing (linear in arc length), or ball-ball contact (quadratic on relative
motion, constant-velocity approximation valid over one short step) -- and the
sim advances to it (step capped at 20 ms). If no interaction event happens
before the last ball stops, the sim fast-forwards each ball straight to its stop
position in one jump. Ball-ball impacts use the equal-mass impulse with
restitution e along the line of centres; cushion impacts reverse the normal
velocity component and scale it by e_n (tangential component unchanged).

Observation tensor ``obs`` -- float32, shape [64], all values clipped to [-1, 1]
------------------------------------------------------------------------------
Slots 0..15 (48 values), 16 canonical ball slots, cue = slot 0, objects = 1..15;
for slot ``i``: ``[3i+0] = x_i / (L/2)``, ``[3i+1] = y_i / (W/2)``,
``[3i+2] = present_i`` (1.0 on table, else 0.0). Absent slots report (0, 0) with
present = 0 (pocketed balls are parked at the origin and flagged absent).

Context scalars, indices 48..63 in this exact order (this table is the
cross-language contract; Rust implements the identical layout):

  | idx | value                                                             | kind     |
  |-----|-------------------------------------------------------------------|----------|
  | 48  | shot_index / max_shots                                            | computed |
  | 49  | balls_remaining / n_objects                                        | computed |
  | 50  | 1.0 (cue ball present)                                             | constant |
  | 51  | mean over live object balls of (dist to nearest mouth) / 1270.0    | computed |
  | 52  | min over live object balls of cos(angle((CB->OB), (OB->nearest pocket))) | computed |
  | 53  | 1.0 if a legal direct pot candidate exists, else 0.0               | computed |
  | 54  | n_legal_candidates / 32.0 (clipped to 1.0)                         | computed |
  | 55  | mean makeability of legal candidates (0.0 if none)                 | computed |
  | 56  | best makeability of legal candidates (0.0 if none)                 | computed |
  | 57  | 1.0 if the episode is in its last third of shots, else 0.0         | computed |
  | 58  | 0.0 (reserved: ball-in-hand domain flag)                           | constant |
  | 59  | 0.0 (reserved: group state)                                        | constant |
  | 60  | 0.0 (reserved: on-8 flag)                                          | constant |
  | 61  | 0.0 (reserved: score differential)                                 | constant |
  | 62  | 1.0 (case = open table)                                            | constant |
  | 63  | 0.0 (fouls so far / 3)                                             | constant |

The four reserved slots are prototype constants 0.0 (or 1.0 for 62) so the Rust
encoder matches trivially; the case flag is constant because the drill is always
open table.

Candidate tensor ``cand`` -- float32 [K, 16], one row per candidate, EXACT order
------------------------------------------------------------------------------
  | col | feature                                                                        |
  |-----|--------------------------------------------------------------------------------|
  | 0   | kind_pot (1/0)                                                                 |
  | 1   | kind_safety (1/0)                                                              |
  | 2   | kind_bank (1/0)                                                                |
  | 3   | kind_kick (1/0)                                                                |
  | 4   | kind_combo (1/0)                                                               |
  | 5   | cos(cut angle) at first contact                                                |
  | 6   | cut_angle / pi                                                                 |
  | 7   | (CB -> contact distance) / L                                                   |
  | 8   | (OB -> pocket distance) / L                                                    |
  | 9   | clearance margin = clip(min_clear / (4R), 0, 1), min_clear = min over other
  |     | balls of (centre distance - 2R) on the cue path and the object path            |
  | 10  | rails / 3                                                                      |
  | 11  | intermediates (0 or 1) -- always 0: the prototype generates no combos          |
  | 12  | makeability heuristic in [0, 1]                                                |
  | 13  | leave heuristic in [0, 1]                                                      |
  | 14  | predicted contact point x / (L/2)                                              |
  | 15  | predicted contact point y / (W/2)                                              |

Candidates are generated per shot as direct pots, one-rail banks, one-rail kicks
and safeties, in that priority order (quota 16/8/3/3), padded to ``K_MAX = 32``
rows with zeros + mask 0, and truncated at K_MAX (documented prototype limit).
The candidate order is sorted by makeability inside each kind. ``contact point``
is the ghost-ball position, i.e. the cue-ball CENTRE at first contact.

Action / masking
----------------
``action_space = Discrete(K_MAX)``; ``action_masks()`` returns a float32 [K_MAX]
array, 1.0 = legal. Illegal actions are accepted defensively as a "pass": they
do not execute a shot, cost -0.1, and consume a shot slot. A last-resort safety
(the softest possible full hit on the nearest object ball) is always generated
and always legal, so the mask is never empty.

Shot parameters chosen by a candidate
-------------------------------------
A candidate fixes direction and speed of the cue ball; only the aim direction is
perturbed at execution: ``dir_exec = rotate(dir, N(0, sigma_aim))`` with
``sigma_aim = 0.0015 rad`` (the same perturbation model the difficulty levels
use). No speed noise. Speeds are analytic: the object ball must arrive at its
target with a 60 mm/s margin, divided by the cut/first-contact transfer factor
``(1+e)/2 * cos(cut)``; banks estimate the two legs without the cushion loss;
safeties use a fixed 320 mm/s. All speeds are capped at ``V_MAX = 1600 mm/s``.

Reward
------
    +2.0 per object ball pocketed
    +1.0 bonus when the drill is cleared (table empty)
    +0.05 if the cue ball contacted an object ball at all (legal hit)
    +0.10 * progress, progress = clip(mean_dist_before - mean_dist_after, -1, 1)
          with mean_dist = mean over live object balls of (distance to nearest
          mouth) / (L/2), before vs after the shot (potted balls leave the mean)
    -1.00 if the cue ball was pocketed (scratch); it is respawned on the head
          spot (-L/4, 0), nudged by 2R + 2 mm along +x while occupied
    -0.10 for an illegal (pass) action

Potting is weighted 20x the dense safety terms on purpose: with a broad PPO
policy the sparse pot signal is otherwise swamped by the always-available
"legal hit" reward and the policy converges to a safe bank-only game (measured
during the prototype sweep).

Cue ball respawn is a fixed spot, never ball-in-hand (slot 58 stays 0.0).
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib

import gymnasium as gym
import numpy as np
from gymnasium import spaces

# --------------------------------------------------------------------------
# spec constants (physics resolution #7 §2/§5) -- never fitted
# --------------------------------------------------------------------------
L = 2540.0
W = 1270.0
HALF_L = L / 2.0
HALF_W = W / 2.0
R = 28.575
BALL_MASS = 170.0  # g, only relevant to document the equal-mass impulse
E_BALL = 0.95
E_CUSHION = 0.75
MU_R = 0.010
G = 9806.65
A_DECEL = MU_R * G  # 98.0665 mm/s^2
SLEEP_V = 1.0  # mm/s
MOUTH_CORNER = 115.9
MOUTH_SIDE = 128.6

# --------------------------------------------------------------------------
# prototype / contract constants
# --------------------------------------------------------------------------
N_OBJECTS = 3
MAX_SHOTS = 8
K_MAX = 32
OBS_DIM = 64
CAND_DIM = 16
SIGMA_AIM = 0.0015  # rad, Gaussian aim noise at execution
V_MAX = 1600.0  # mm/s cap on any cue-ball shot
V_ARRIVE = 60.0  # mm/s margin the object ball should still have at the pocket
V_SAFETY = 320.0  # mm/s fixed safety speed
DT_CAP = 0.02  # s, max analytic step
DT_MIN = 5e-4  # s, floor for a step
MAX_SIM_ITERS = 4000  # per shot; outcome flagged when hit
INF = float("inf")
POCKETS = ((HALF_L, HALF_W), (HALF_L, -HALF_W), (-HALF_L, HALF_W), (-HALF_L, -HALF_W), (0.0, HALF_W), (0.0, -HALF_W))
POCKET_IS_CORNER = (True, True, True, True, False, False)
POCKET_FACTOR = tuple(0.9 if c else 1.0 for c in POCKET_IS_CORNER)
HEAD_SPOT = (-L / 4.0, 0.0)

_KIND_POT, _KIND_SAFETY, _KIND_BANK, _KIND_KICK, _KIND_COMBO = 0, 1, 2, 3, 4


# --------------------------------------------------------------------------
# small vector helpers (pure python: n <= 16 balls, avoids numpy overhead)
# --------------------------------------------------------------------------
def _seg_point_dist(ax, ay, bx, by, px, py):
    """Distance from point p to segment ab."""
    dx, dy = bx - ax, by - ay
    ll = dx * dx + dy * dy
    if ll <= 1e-12:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / ll
    t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def _t_at_delta(speed: float, delta: float) -> float:
    """Time for a ball at ``speed`` to travel arc length ``delta`` under a=A_DECEL."""
    if delta <= 0.0:
        return 0.0
    disc = speed * speed - 2.0 * A_DECEL * delta
    if disc < 0.0:
        return INF
    return (speed - math.sqrt(disc)) / A_DECEL


def _stop_distance(speed: float) -> float:
    return speed * speed / (2.0 * A_DECEL)


# --------------------------------------------------------------------------
# analytic toy sim
# --------------------------------------------------------------------------
def simulate_shot(xs, ys, cue_idx, direction, speed, alive=None):
    """Roll the balls from the current state until rest.

    ``xs``/``ys`` are mutated in place (positions at rest), ``speed`` is the cue
    ball's initial speed along ``direction`` (unit vector). Returns a dict with
    ``pocketed`` (list of ball indices), ``first_contact`` (index of the first
    object ball the cue ball touched, or None), ``cushion_hits``, ``scratch``,
    ``sim_iters``, ``hit_iter_cap`` and ``sim_time``.
    """
    n = len(xs)
    if alive is None:
        alive = [True] * n
    else:
        alive = list(alive)
    vx = [0.0] * n
    vy = [0.0] * n
    dx, dy = direction
    vx[cue_idx], vy[cue_idx] = dx * speed, dy * speed

    t = 0.0
    first_contact = None
    cushion_hits = 0
    pocketed: list[int] = []
    it = 0
    while it < MAX_SIM_ITERS:
        it += 1
        moving = [i for i in range(n) if alive[i] and vx[i] * vx[i] + vy[i] * vy[i] > SLEEP_V * SLEEP_V]
        if not moving:
            break

        # --- event times -------------------------------------------------
        speeds = {i: math.hypot(vx[i], vy[i]) for i in moving}
        stop_at = {i: speeds[i] / A_DECEL for i in moving}
        t_last_stop = max(stop_at.values())
        t_event = INF
        for i in moving:
            s = speeds[i]
            ux, uy = vx[i] / s, vy[i] / s
            # rails
            if ux > 1e-12:
                t_event = min(t_event, _t_at_delta(s, ((HALF_L - R) - xs[i]) / ux))
            elif ux < -1e-12:
                t_event = min(t_event, _t_at_delta(s, (-(HALF_L - R) - xs[i]) / ux))
            if uy > 1e-12:
                t_event = min(t_event, _t_at_delta(s, ((HALF_W - R) - ys[i]) / uy))
            elif uy < -1e-12:
                t_event = min(t_event, _t_at_delta(s, (-(HALF_W - R) - ys[i]) / uy))
            # corner mouth lines (linear in arc length along the straight path)
            for cx, cy in POCKETS[:4]:
                d_cx, d_cy = (math.copysign(1.0, cx) / math.sqrt(2.0), math.copysign(1.0, cy) / math.sqrt(2.0))
                denom = ux * d_cx + uy * d_cy
                if denom <= 1e-12:
                    continue
                offset = (HALF_L + HALF_W) / math.sqrt(2.0) - MOUTH_CORNER / 2.0
                delta = (offset - (xs[i] * d_cx + ys[i] * d_cy)) / denom
                if delta <= 0.0:
                    continue
                t_cx, t_cy = -d_cy, d_cx  # perpendicular to the diagonal
                lat = (xs[i] - cx + delta * ux) * t_cx + (ys[i] - cy + delta * uy) * t_cy
                if abs(lat) <= MOUTH_CORNER / 2.0:
                    t_event = min(t_event, _t_at_delta(s, delta))
        # ball-ball (constant-velocity approximation over one step)
        for ii in range(n):
            if not alive[ii] or (vx[ii] == 0.0 and vy[ii] == 0.0):
                continue
            for jj in range(ii + 1, n):
                if not alive[jj]:
                    continue
                rx, ry = xs[ii] - xs[jj], ys[ii] - ys[jj]
                ux, uy = vx[ii] - vx[jj], vy[ii] - vy[jj]
                aa = ux * ux + uy * uy
                if aa < 1e-9:
                    continue
                bb = 2.0 * (rx * ux + ry * uy)
                if bb >= 0.0:
                    continue
                cc = rx * rx + ry * ry - 4.0 * R * R
                disc = bb * bb - 4.0 * aa * cc
                if disc <= 0.0:
                    continue
                tt = (-bb - math.sqrt(disc)) / (2.0 * aa)
                if tt > 0.0:
                    t_event = min(t_event, tt)

        # --- fast-forward when nothing can happen before every ball stops
        if t_event >= t_last_stop - 1e-12:
            for i in moving:
                s = speeds[i]
                travel = _stop_distance(s)
                xs[i] += vx[i] / s * travel
                ys[i] += vy[i] / s * travel
                vx[i] = vy[i] = 0.0
            t += t_last_stop
            break

        dt = min(DT_CAP, t_event)
        if dt < DT_MIN:
            dt = DT_MIN

        # --- advance -----------------------------------------------------
        for i in moving:
            s = speeds[i]
            travel = s * dt - 0.5 * A_DECEL * dt * dt
            d_stop = _stop_distance(s)
            s_new = s - A_DECEL * dt
            if travel >= d_stop or s_new <= SLEEP_V:
                travel, s_new = d_stop, 0.0
            xs[i] += vx[i] / s * travel
            ys[i] += vy[i] / s * travel
            if s_new <= 0.0:
                vx[i] = vy[i] = 0.0
            else:
                vx[i] = vx[i] / s * s_new
                vy[i] = vy[i] / s * s_new
        t += dt

        # --- ball-ball impulses (equal mass, restitution e) --------------
        for _pass in range(4):
            hit = False
            for ii in range(n):
                if not alive[ii]:
                    continue
                for jj in range(ii + 1, n):
                    if not alive[jj]:
                        continue
                    dx_, dy_ = xs[jj] - xs[ii], ys[jj] - ys[ii]
                    dist = math.hypot(dx_, dy_)
                    if dist >= 2.0 * R or dist <= 1e-9:
                        continue
                    nx, ny = dx_ / dist, dy_ / dist
                    vnr = (vx[ii] - vx[jj]) * nx + (vy[ii] - vy[jj]) * ny
                    if vnr <= 0.0:
                        continue
                    if first_contact is None and (ii == cue_idx or jj == cue_idx):
                        other = jj if ii == cue_idx else ii
                        if other != cue_idx:
                            first_contact = other
                    vn_i = vx[ii] * nx + vy[ii] * ny
                    vn_j = vx[jj] * nx + vy[jj] * ny
                    vn_i_new = 0.5 * (1.0 - E_BALL) * vn_i + 0.5 * (1.0 + E_BALL) * vn_j
                    vn_j_new = 0.5 * (1.0 + E_BALL) * vn_i + 0.5 * (1.0 - E_BALL) * vn_j
                    vx[ii] += (vn_i_new - vn_i) * nx
                    vy[ii] += (vn_i_new - vn_i) * ny
                    vx[jj] += (vn_j_new - vn_j) * nx
                    vy[jj] += (vn_j_new - vn_j) * ny
                    # separate to exactly 2R
                    overlap = 2.0 * R - dist
                    xs[ii] -= nx * overlap * 0.5
                    ys[ii] -= ny * overlap * 0.5
                    xs[jj] += nx * overlap * 0.5
                    ys[jj] += ny * overlap * 0.5
                    hit = True
            if not hit:
                break
        for i in range(n):
            if alive[i] and (vx[i] * vx[i] + vy[i] * vy[i]) < SLEEP_V * SLEEP_V:
                vx[i] = vy[i] = 0.0

        # --- rails and pockets -------------------------------------------
        for i in range(n):
            if not alive[i] or (vx[i] == 0.0 and vy[i] == 0.0):
                continue
            for axis in ("x", "y"):
                if axis == "x":
                    lim = HALF_L - R
                    if abs(xs[i]) <= lim:
                        continue
                    sign = 1.0 if xs[i] > 0.0 else -1.0
                    qx, qy = lim * sign, ys[i]
                else:
                    lim = HALF_W - R
                    if abs(ys[i]) <= lim:
                        continue
                    sign = 1.0 if ys[i] > 0.0 else -1.0
                    qx, qy = xs[i], lim * sign
                pocket = False
                if axis == "y" and abs(qx) <= MOUTH_SIDE / 2.0:
                    pocket = True  # side pocket gap in the long rail
                else:
                    for k in range(4):
                        cx, cy = POCKETS[k]  # corner capture via the mouth line
                        d_cx, d_cy = math.copysign(1.0, cx) / math.sqrt(2.0), math.copysign(1.0, cy) / math.sqrt(2.0)
                        if qx * d_cx + qy * d_cy < (HALF_L + HALF_W) / math.sqrt(2.0) - MOUTH_CORNER / 2.0:
                            continue
                        t_cx, t_cy = -d_cy, d_cx
                        if abs((qx - cx) * t_cx + (qy - cy) * t_cy) <= MOUTH_CORNER / 2.0:
                            pocket = True
                            break
                if pocket:
                    alive[i] = False
                    vx[i] = vy[i] = 0.0
                    pocketed.append(i)
                    break
                if axis == "x":
                    xs[i] = lim * sign
                    vx[i] = -vx[i] * E_CUSHION
                else:
                    ys[i] = lim * sign
                    vy[i] = -vy[i] * E_CUSHION
                cushion_hits += 1

    return {
        "pocketed": pocketed,
        "first_contact": first_contact,
        "cushion_hits": cushion_hits,
        "scratch": cue_idx in pocketed,
        "sim_iters": it,
        "hit_iter_cap": it >= MAX_SIM_ITERS,
        "sim_time": t,
    }


# --------------------------------------------------------------------------
# candidate generation
# --------------------------------------------------------------------------
def _nearest_pocket_dist(x, y):
    return min(math.hypot(x - px, y - py) for px, py in POCKETS)


def _pot_speed(d_ob):
    return min(V_MAX, math.sqrt(2.0 * A_DECEL * d_ob + V_ARRIVE * V_ARRIVE))


def _cue_speed(v_obj, cos_cut, d_cue):
    transfer = 0.5 * (1.0 + E_BALL) * max(cos_cut, 1e-3)
    v_contact2 = (v_obj / transfer) ** 2
    return min(V_MAX, math.sqrt(v_contact2 + 2.0 * A_DECEL * d_cue))


def generate_candidates(xs, ys, live, cue_idx, shot_index=0, max_shots=MAX_SHOTS):
    """Return (rows, mask, meta): float32 [K_MAX,16], bool [K_MAX], list of dicts per row."""
    objects = [i for i in live if i != cue_idx]
    cx, cy = xs[cue_idx], ys[cue_idx]
    rows: list[list[float]] = []
    metas: list[dict] = []

    def add(kind, cos_cut, d_cue, d_ob, rail_hits, clear, make, leave, contact, speed, legal, note=""):
        rows.append(
            [
                1.0 if kind == _KIND_POT else 0.0,
                1.0 if kind == _KIND_SAFETY else 0.0,
                1.0 if kind == _KIND_BANK else 0.0,
                1.0 if kind == _KIND_KICK else 0.0,
                1.0 if kind == _KIND_COMBO else 0.0,
                cos_cut,
                math.acos(max(-1.0, min(1.0, cos_cut))) / math.pi,
                d_cue / L,
                d_ob / L,
                max(0.0, min(1.0, clear / (4.0 * R))),
                rail_hits / 3.0,
                0.0,  # intermediates: no combos in the prototype
                make,
                leave,
                contact[0] / HALF_L,
                contact[1] / HALF_W,
            ]
        )
        metas.append({"kind": kind, "dir": None, "speed": speed, "legal": legal, "note": note})

    # --- direct pots ------------------------------------------------------
    pots = []
    for o in objects:
        ox, oy = xs[o], ys[o]
        for p_idx, (px, py) in enumerate(POCKETS):
            ux, uy = px - ox, py - oy
            d_ob = math.hypot(ux, uy)
            if d_ob <= 1e-6:
                continue
            ux, uy = ux / d_ob, uy / d_ob
            gx, gy = ox - 2.0 * R * ux, oy - 2.0 * R * uy  # ghost-ball centre
            vx_, vy_ = gx - cx, gy - cy
            d_cue = math.hypot(vx_, vy_)
            if d_cue <= 1e-6:
                continue
            vx_, vy_ = vx_ / d_cue, vy_ / d_cue
            cos_cut = vx_ * ux + vy_ * uy
            if cos_cut <= 0.2:
                continue
            clear_cue = min(
                [_seg_point_dist(cx, cy, gx, gy, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]
            ) - 2.0 * R
            clear_ob = min([_seg_point_dist(ox, oy, px, py, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]) - 2.0 * R
            clear = min(clear_cue, clear_ob)
            legal = clear > 0.0
            v_obj = _pot_speed(d_ob)
            speed = _cue_speed(v_obj, cos_cut, d_cue)
            make = min(1.0, cos_cut) ** 1.5 * POCKET_FACTOR[p_idx] * (0.5 + 0.5 * max(0.0, min(1.0, clear / (6.0 * R))))
            make *= 1.0 - 0.5 * min(1.0, d_ob / L)
            pots.append((make, p_idx, o, cos_cut, d_cue, d_ob, clear, (gx, gy), speed, legal, vx_, vy_))
    pots.sort(key=lambda r: -r[0])
    for make, _p, _o, cos_cut, d_cue, d_ob, clear, contact, speed, legal, ux, uy in pots[:16]:
        add(_KIND_POT, cos_cut, d_cue, d_ob, 0, clear, make, _leave_pot(contact), contact, speed, legal)
        metas[-1]["dir"] = (ux, uy)  # cue-ball travel direction: cue -> ghost-ball centre

    # --- one-rail banks ---------------------------------------------------
    banks = []
    for o in objects:
        ox, oy = xs[o], ys[o]
        for p_idx, (px, py) in enumerate(POCKETS):
            for rail, (mx, my, axis) in enumerate(((0.0, HALF_W, "y"), (0.0, -HALF_W, "y"), (HALF_L, 0.0, "x"), (-HALF_L, 0.0, "x"))):
                tx, ty = (px, 2.0 * my - py) if axis == "y" else (2.0 * mx - px, py)  # mirrored target
                vx_, vy_ = tx - ox, ty - oy
                d_t = math.hypot(vx_, vy_)
                if d_t <= 1e-6:
                    continue
                vx_, vy_ = vx_ / d_t, vy_ / d_t
                if axis == "y":
                    if abs(vy_) < 1e-6 or math.copysign(1.0, my) != math.copysign(1.0, vy_):
                        continue
                    t_k = (my - oy) / vy_
                else:
                    if abs(vx_) < 1e-6 or math.copysign(1.0, mx) != math.copysign(1.0, vx_):
                        continue
                    t_k = (mx - ox) / vx_
                if t_k <= 0.0:
                    continue
                kx, ky = ox + vx_ * t_k, oy + vy_ * t_k
                if axis == "y" and (abs(kx) > HALF_L - R or abs(kx) <= MOUTH_SIDE / 2.0 + R):
                    continue  # knee in the jaw or off the rail
                if axis == "x" and abs(ky) > HALF_W - R:
                    continue
                d_first = math.hypot(kx - ox, ky - oy)
                d_rest = math.hypot(px - kx, py - ky)
                ux, uy = vx_, vy_  # ball leaves toward the knee
                gx, gy = ox - 2.0 * R * ux, oy - 2.0 * R * uy
                dvx, dvy = gx - cx, gy - cy
                d_cue = math.hypot(dvx, dvy)
                if d_cue <= 1e-6:
                    continue
                dvx, dvy = dvx / d_cue, dvy / d_cue
                cos_cut = dvx * ux + dvy * uy
                if cos_cut <= 0.2:
                    continue
                clear = min([_seg_point_dist(cx, cy, gx, gy, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]) - 2.0 * R
                clear = min(clear, min([_seg_point_dist(ox, oy, kx, ky, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]) - 2.0 * R)
                legal = clear > 0.0
                v_obj = _pot_speed(d_first + d_rest)  # ignores cushion loss (prototype)
                speed = _cue_speed(v_obj, cos_cut, d_cue)
                make = min(1.0, cos_cut) ** 1.5 * 0.55 * POCKET_FACTOR[p_idx] * (0.5 + 0.5 * max(0.0, min(1.0, clear / (6.0 * R))))
                make *= 1.0 - 0.5 * min(1.0, d_first / L)
                leave = max(0.0, min(1.0, 1.0 - _nearest_pocket_dist(gx, gy) / HALF_L))
                banks.append((make, cos_cut, d_cue, d_first + d_rest, clear, (gx, gy), speed, legal, (dvx, dvy)))
    banks.sort(key=lambda r: -r[0])
    for make, cos_cut, d_cue, d_ob, clear, contact, speed, legal, (ux, uy) in banks[:8]:
        add(_KIND_BANK, cos_cut, d_cue, d_ob, 1, clear, make, _leave_pot(contact), contact, speed, legal)
        metas[-1]["dir"] = (ux, uy)

    # --- one-rail kicks (cue ball off one rail into the object) -----------
    kicks = []
    for o in objects:
        ox, oy = xs[o], ys[o]
        for p_idx, (px, py) in enumerate(POCKETS):
            ux0, uy0 = px - ox, py - oy
            d_ob = math.hypot(ux0, uy0)
            if d_ob <= 1e-6:
                continue
            ux0, uy0 = ux0 / d_ob, uy0 / d_ob
            gx, gy = ox - 2.0 * R * ux0, oy - 2.0 * R * uy0
            for mx, my, axis in ((0.0, HALF_W, "y"), (0.0, -HALF_W, "y"), (HALF_L, 0.0, "x"), (-HALF_L, 0.0, "x")):
                tx, ty = (gx, 2.0 * my - gy) if axis == "y" else (2.0 * mx - gx, gy)
                vx_, vy_ = tx - cx, ty - cy
                d_t = math.hypot(vx_, vy_)
                if d_t <= 1e-6:
                    continue
                vx_, vy_ = vx_ / d_t, vy_ / d_t
                if axis == "y":
                    if abs(vy_) < 1e-6 or math.copysign(1.0, my) != math.copysign(1.0, vy_):
                        continue
                    t_k = (my - cy) / vy_
                else:
                    if abs(vx_) < 1e-6 or math.copysign(1.0, mx) != math.copysign(1.0, vx_):
                        continue
                    t_k = (mx - cx) / vx_
                if t_k <= 0.0:
                    continue
                kx, ky = cx + vx_ * t_k, cy + vy_ * t_k
                if axis == "y" and (abs(kx) > HALF_L - R or abs(kx) <= MOUTH_SIDE / 2.0 + R):
                    continue
                if axis == "x" and abs(ky) > HALF_W - R:
                    continue
                d_cue = math.hypot(kx - cx, ky - cy) + math.hypot(gx - kx, gy - ky)
                cos_cut = vx_ * ux0 + vy_ * uy0  # direction of travel at contact ~ knee leg
                if cos_cut <= 0.2:
                    continue
                clear = min([_seg_point_dist(cx, cy, kx, ky, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]) - 2.0 * R
                legal = clear > 0.0
                v_obj = _pot_speed(d_ob)
                speed = min(V_MAX, math.sqrt((v_obj / (0.5 * (1.0 + E_BALL) * max(cos_cut, 1e-3))) ** 2 + 2.0 * A_DECEL * d_cue))
                make = min(1.0, cos_cut) ** 1.5 * 0.35 * POCKET_FACTOR[p_idx] * (0.5 + 0.5 * max(0.0, min(1.0, clear / (6.0 * R))))
                leave = max(0.0, min(1.0, 1.0 - _nearest_pocket_dist(gx, gy) / HALF_L))
                kicks.append((make, cos_cut, d_cue, d_ob, clear, (gx, gy), speed, legal, (vx_, vy_)))
    kicks.sort(key=lambda r: -r[0])
    for make, cos_cut, d_cue, d_ob, clear, contact, speed, legal, (ux, uy) in kicks[:3]:
        add(_KIND_KICK, cos_cut, d_cue, d_ob, 1, clear, make, _leave_pot(contact), contact, speed, legal)
        metas[-1]["dir"] = (ux, uy)

    # --- safeties ---------------------------------------------------------
    safeties = []
    for o in objects:
        ox, oy = xs[o], ys[o]
        vx_, vy_ = ox - cx, oy - cy
        d_cue = math.hypot(vx_, vy_)
        if d_cue <= 1e-6:
            continue
        vx_, vy_ = vx_ / d_cue, vy_ / d_cue
        clear = min([_seg_point_dist(cx, cy, ox, oy, xs[k], ys[k]) for k in live if k not in (cue_idx, o)] or [INF]) - 2.0 * R
        legal = clear > 0.0
        after_x, after_y = ox + vx_ * 300.0, oy + vy_ * 300.0
        leave = max(0.0, min(1.0, 1.0 - _nearest_pocket_dist(after_x, after_y) / HALF_L))
        contact = (ox - 2.0 * R * vx_, oy - 2.0 * R * vy_)
        make = 0.15 * (0.5 + 0.5 * max(0.0, min(1.0, clear / (6.0 * R))))
        safeties.append((leave, make, d_cue, clear, contact, legal, (vx_, vy_)))
    for leave, make, d_cue, clear, contact, legal, (ux, uy) in safeties[:3]:
        add(_KIND_SAFETY, 1.0, d_cue, 0.0, 0, clear, make, leave, contact, V_SAFETY, legal)
        metas[-1]["dir"] = (ux, uy)

    # last-resort safety: softest full hit on the nearest object ball, always legal
    if objects:
        nearest = min(objects, key=lambda o: (xs[o] - cx) ** 2 + (ys[o] - cy) ** 2)
        ox, oy = xs[nearest], ys[nearest]
        vx_, vy_ = ox - cx, oy - cy
        d_cue = max(1e-6, math.hypot(vx_, vy_))
        vx_, vy_ = vx_ / d_cue, vy_ / d_cue
        contact = (ox - 2.0 * R * vx_, oy - 2.0 * R * vy_)
        rows.append([0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, d_cue / L, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, contact[0] / HALF_L, contact[1] / HALF_W])
        metas.append({"kind": _KIND_SAFETY, "dir": (vx_, vy_), "speed": V_SAFETY, "legal": True, "note": "last-resort safety"})

    # --- assemble padded K_MAX tensors -----------------------------------
    n_real = min(len(rows), K_MAX)
    rows = rows[:K_MAX]
    metas = metas[:K_MAX]
    for m in metas:
        if m["dir"] is None:  # unreachable for generated rows, kept defensive
            m["dir"] = (1.0, 0.0)
            m["legal"] = False
    cand = np.zeros((K_MAX, CAND_DIM), dtype=np.float32)
    mask = np.zeros((K_MAX,), dtype=np.float32)
    mask[:n_real] = [1.0 if metas[i]["legal"] else 0.0 for i in range(n_real)]
    for i in range(n_real):
        cand[i] = np.clip(np.asarray(rows[i], dtype=np.float32), -1.0, 1.0)
    if n_real and not mask.any():
        mask[0] = 1.0  # defensive: the last-resort safety keeps the mask non-empty
        metas[0]["legal"] = True
    json_masks = mask
    return cand, json_masks, metas


def _leave_pot(contact):
    return max(0.0, min(1.0, 1.0 - _nearest_pocket_dist(contact[0], contact[1]) / HALF_L))


# --------------------------------------------------------------------------
# env
# --------------------------------------------------------------------------
class PoolShotEnv(gym.Env):
    """Shot-granularity pool env: pick a candidate, the toy sim resolves it."""

    metadata = {"render_modes": []}

    def __init__(self, n_objects: int = N_OBJECTS, max_shots: int = MAX_SHOTS, sim_seed: int | None = None):
        super().__init__()
        self.n_objects = int(n_objects)
        self.max_shots = int(max_shots)
        assert 1 <= self.n_objects <= 15
        assert 1 <= self.max_shots <= 64
        self.observation_space = spaces.Dict(
            {
                "obs": spaces.Box(low=-1.0, high=1.0, shape=(OBS_DIM,), dtype=np.float32),
                "cand": spaces.Box(low=-1.0, high=1.0, shape=(K_MAX, CAND_DIM), dtype=np.float32),
                "mask": spaces.Box(low=0.0, high=1.0, shape=(K_MAX,), dtype=np.float32),
            }
        )
        self.action_space = spaces.Discrete(K_MAX)
        self._sim_seed = sim_seed
        self._rack_rng = np.random.default_rng(sim_seed)
        self._noise_rng = np.random.default_rng(None if sim_seed is None else sim_seed + 1000003)
        self.reset(seed=sim_seed)

    # -- state -------------------------------------------------------------
    def _rack(self, rng):
        """Initial positions: cue on the head spot, objects in the far quarter."""
        xs = [0.0] * (1 + self.n_objects)
        ys = [0.0] * (1 + self.n_objects)
        xs[0] = HEAD_SPOT[0] + rng.uniform(-40.0, 40.0)
        ys[0] = rng.uniform(-40.0, 40.0)
        base = [(L * 0.26, 0.0), (L * 0.33, 95.0), (L * 0.40, -75.0), (L * 0.46, 40.0), (L * 0.50, -130.0)]
        for k in range(self.n_objects):
            bx, by = base[k % len(base)]
            for _try in range(40):
                px = bx + rng.uniform(-45.0, 45.0)
                py = by + rng.uniform(-45.0, 45.0)
                if all(math.hypot(px - xs[j], py - ys[j]) > 2.0 * R + 8.0 for j in range(1 + k)):
                    xs[1 + k], ys[1 + k] = px, py
                    break
            else:
                xs[1 + k], ys[1 + k] = bx, by
        return xs, ys

    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is not None:
            # a fresh seed restarts both streams; unseeded resets keep drawing new racks
            self._rack_rng = np.random.default_rng(seed)
            self._noise_rng = np.random.default_rng(int(seed) + 1000003)
        self._xs, self._ys = self._rack(self._rack_rng)
        self._present = [True] * (1 + self.n_objects)
        self._shot_index = 0
        self._fouls = 0
        self._n_object_start = self.n_objects
        self._cand, self._mask, self._meta = generate_candidates(
            self._xs, self._ys, [i for i in range(len(self._xs)) if self._present[i]], 0, 0, self.max_shots
        )
        return self._observation(), self._info()

    def action_masks(self) -> np.ndarray:
        """sb3-contrib hook: 1.0 = legal."""
        return self._mask.astype(bool)

    # -- tensors -----------------------------------------------------------
    def _slots(self):
        """16 canonical slots: cue = 0, objects = 1..15, in id order."""
        n = len(self._xs)
        slots = []
        for i in range(16):
            if i < n:
                slots.append((self._xs[i], self._ys[i], 1.0 if self._present[i] else 0.0))
            else:
                slots.append((0.0, 0.0, 0.0))
        return slots

    def _observation(self) -> dict:
        obs = np.zeros((OBS_DIM,), dtype=np.float32)
        for i, (x, y, present) in enumerate(self._slots()):
            obs[3 * i + 0] = x / HALF_L
            obs[3 * i + 1] = y / HALF_W
            obs[3 * i + 2] = present
        live_objects = [i for i in range(1, len(self._xs)) if self._present[i]]
        # 48..51
        obs[48] = self._shot_index / self.max_shots
        obs[49] = len(live_objects) / self._n_object_start
        obs[50] = 1.0
        if live_objects:
            obs[51] = float(np.mean([_nearest_pocket_dist(self._xs[i], self._ys[i]) for i in live_objects])) / 1270.0
            cosines = []
            cx, cy = self._xs[0], self._ys[0]
            for i in live_objects:
                ox, oy = self._xs[i], self._ys[i]
                p = min(POCKETS, key=lambda q: (q[0] - ox) ** 2 + (q[1] - oy) ** 2)
                ax, ay = ox - cx, oy - cy
                bx, by = p[0] - ox, p[1] - oy
                na, nb = math.hypot(ax, ay), math.hypot(bx, by)
                cosines.append(0.0 if na < 1e-9 or nb < 1e-9 else (ax * bx + ay * by) / (na * nb))
            obs[52] = float(np.clip(min(cosines), -1.0, 1.0))
        # 53..56
        legal = np.flatnonzero(self._mask > 0.0)
        kinds = [self._meta[i]["kind"] for i in legal]
        makes = np.asarray([self._cand[i, 12] for i in legal], dtype=np.float64) if len(legal) else np.empty(0)
        obs[53] = 1.0 if any(k == _KIND_POT for k in kinds) else 0.0
        obs[54] = min(1.0, len(legal) / 32.0)
        obs[55] = float(makes.mean()) if len(makes) else 0.0
        obs[56] = float(makes.max()) if len(makes) else 0.0
        # 57..63
        obs[57] = 1.0 if self._shot_index >= (2.0 / 3.0) * self.max_shots else 0.0
        obs[58] = 0.0
        obs[59] = 0.0
        obs[60] = 0.0
        obs[61] = 0.0
        obs[62] = 1.0
        obs[63] = 0.0  # fouls are tracked but stay a constant in this prototype
        return {"obs": np.clip(obs, -1.0, 1.0).astype(np.float32), "cand": self._cand, "mask": self._mask}

    def _info(self) -> dict:
        return {
            "shot_index": self._shot_index,
            "live_objects": sum(1 for i in range(1, len(self._xs)) if self._present[i]),
            "fouls": self._fouls,
        }

    # -- step --------------------------------------------------------------
    def step(self, action: int):
        action = int(action)
        if not (0 <= action < K_MAX) or self._mask[action] <= 0.0:
            # defensively accepted "pass": no shot, costs a shot slot
            self._shot_index += 1
            reward = -0.10
            truncated = self._shot_index >= self.max_shots
            terminated = False
            info = self._info()
            info["pass"] = True
            if truncated:
                info["TimeLimit.truncated"] = True
            return self._observation(), reward, terminated, truncated, info

        meta = self._meta[action]
        dirx, diry = meta["dir"]
        theta = self._noise_rng.normal(0.0, SIGMA_AIM)
        c, s = math.cos(theta), math.sin(theta)
        direction = (dirx * c - diry * s, dirx * s + diry * c)

        before = [i for i in range(1, len(self._xs)) if self._present[i]]
        before_dist = (
            float(np.mean([_nearest_pocket_dist(self._xs[i], self._ys[i]) for i in before])) if before else 0.0
        )

        outcome = simulate_shot(self._xs, self._ys, 0, direction, meta["speed"], alive=self._present)
        for i in outcome["pocketed"]:
            self._present[i] = False
            self._xs[i], self._ys[i] = 0.0, 0.0  # absent slots report (0, 0)

        n_potted = len([i for i in outcome["pocketed"] if i != 0])
        reward = 2.0 * n_potted
        if outcome["first_contact"] is not None:
            reward += 0.05
        after = [i for i in range(1, len(self._xs)) if self._present[i]]
        after_dist = float(np.mean([_nearest_pocket_dist(self._xs[i], self._ys[i]) for i in after])) if after else 0.0
        reward += 0.10 * float(np.clip((before_dist - after_dist) / HALF_L, -1.0, 1.0))
        if outcome["scratch"]:
            reward -= 1.00
            self._fouls += 1
            self._present[0] = True
            self._xs[0], self._ys[0] = HEAD_SPOT
            occupied = [i for i in range(1, len(self._xs)) if self._present[i]]
            guard = 0
            while any(math.hypot(self._xs[0] - self._xs[i], self._ys[0] - self._ys[i]) <= 2.0 * R + 2.0 for i in occupied):
                self._xs[0] += 2.0 * R + 2.0
                guard += 1
                if guard > 200:
                    break
        self._shot_index += 1
        cleared = not after
        if cleared:
            reward += 1.0
        terminated = cleared
        truncated = (not cleared) and self._shot_index >= self.max_shots

        self._cand, self._mask, self._meta = generate_candidates(
            self._xs, self._ys, [i for i in range(len(self._xs)) if self._present[i]], 0, self._shot_index, self.max_shots
        )
        info = self._info()
        info.update(
            {
                "pocketed": n_potted,
                "scratch": outcome["scratch"],
                "shot_kind": meta["kind"],
                "sim_iters": outcome["sim_iters"],
                "hit_iter_cap": outcome["hit_iter_cap"],
                "sim_time": outcome["sim_time"],
                "cushion_hits": outcome["cushion_hits"],
            }
        )
        if truncated:
            info["TimeLimit.truncated"] = True
        return self._observation(), float(reward), bool(terminated), bool(truncated), info


# --------------------------------------------------------------------------
# fixture / smoke main
# --------------------------------------------------------------------------
def write_obs_fixture(path: pathlib.Path, seed: int = 20260911) -> dict:
    """One deterministic position + the obs vector the env computes for it.

    Shape (consumed by the Rust encoder cross-check):
    {"cue":[x,y],"balls":[[x,y,present],...16 slots in id order],
     "shot_index":i,"max_shots":n,"obs":[64 floats]}
    Slots 1..n_objects carry the object balls; slot 0 is the cue ball.
    """
    env = PoolShotEnv(sim_seed=seed)
    obs = env._observation()
    slots = env._slots()
    fixture = {
        "cue": [float(env._xs[0]), float(env._ys[0])],
        "balls": [[float(x), float(y), float(p)] for x, y, p in slots],
        "shot_index": int(env._shot_index),
        "max_shots": int(env.max_shots),
        "obs": [float(v) for v in obs["obs"]],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as fh:
        json.dump(fixture, fh, indent=1)
    return fixture


def _main() -> None:
    parser = argparse.ArgumentParser(description="env.py utilities")
    parser.add_argument("--fixture", type=pathlib.Path, default=None, help="write the obs fixture json here")
    parser.add_argument("--seed", type=int, default=20260911)
    args = parser.parse_args()
    if args.fixture:
        fx = write_obs_fixture(args.fixture, args.seed)
        print(json.dumps({"written": str(args.fixture), "cue": fx["cue"], "obs_head": fx["obs"][:6]}, indent=1))


if __name__ == "__main__":
    _main()
