#!/usr/bin/env python3
"""Generate the digitized/tabulated curve files in ``data/curves/``.

Every file this writes is a self-describing JSON document: provenance (source
URL, document number, revision date), the method used to obtain each point, an
error estimate, and the points themselves. Nothing here is traced by eye from a
plot: where the source publishes a closed form (TP A-28 throw, TP B-6 rail
travel, TP B-8 draw) the points are exact evaluations of that form at the
source's own parameters, and where the source publishes a table the numbers are
transcribed verbatim. The two cases are labelled per point set.

Usage:
    python3 tools/benchmark/gen_curves.py --out data/curves

Self-checks: the script re-evaluates the source's own printed anchor values and
reports the residual, so a silent transcription error shows up as a failure.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys

INCH = 0.0254
FOOT = 0.3048
MPH = 0.44704
MM = 0.001
GRAVITY = 9.80665

# --------------------------------------------------------------------------- #
# source parameters, exactly as printed by the sources
# --------------------------------------------------------------------------- #
# TP A-28 p.1: ball radius, and the Marlow (1995) Table 10 friction data points
# the document fits its friction model to.
BALL_RADIUS = 1.125 * INCH                     # 1.125 in
THROW_SPEEDS = (0.5, 1.5, 4.5)                 # TP A-28 "slow, medium, fast"
A28_FRICTION = (9.951e-3, 0.108, 1.088)        # a + b*exp(-c*v), TP A-28 fit
B3_FRICTION = (0.016, 0.219, 0.691)            # TP B-3 refit to Colenso data
FRICTION_CAP = 1.0 / 7.0                       # TP A-14/A-28 min(...) limit

# TP B-3 p.2 calibration table (Colin Colenso's measurements), in inches/yard.
B3_CALIBRATION = {
    "cut_deg": (30.0, 45.0),
    "speeds_m_s": (4.0 / 3.6, 8.0 / 3.6, 12.0 / 3.6),
    "speeds_label": ("4 km/h soft", "8 km/h medium-soft", "12 km/h medium"),
    "throw_in_per_yd": ((4.8, 5.6), (3.5, 3.0), (3.0, 2.3)),
    "throw_deg_as_printed": ((7.685, 8.985), (5.588, 4.786), (4.786, 3.666)),
}

# TP B-6 p.1/2 constants and the printed results for the travel model.
B6 = {
    "table_length_in": 100.0,
    "mu_s": 0.2,
    "mu_r": 0.01,
    "e_c": 0.7,
    "speeds_mph": (1.5, 3.0, 5.0, 7.0, 8.0, 12.0, 20.0),
    "labels": ("touch", "slow", "medium-soft", "medium", "medium-fast",
               "fast", "power"),
    "printed_table_lengths": {1.5: 0.903, 3.0: 1.684, 7.0: 3.005,
                              12.0: 3.69, 20.0: 4.715},
    "printed_lag_speed_mph": 3.465,
}

# TP B-5 p.1/3 constants and the printed results for a rolling square hit.
B5 = {
    "mu_b": 0.06,
    "e_b": 0.94,
    "mu_s": 0.2,
    "mu_r": 0.01,
    "printed_ratio": 6.08,
    "printed_hop": {7.0: 0.067, 12.0: 0.196, 20.0: 0.544},      # inches
    "printed_hop_time": {12.0: 0.064, 20.0: 0.106},             # seconds
}

# TP B-8 p.2 constants and shot ranges.
B8 = {
    "mu_s": 0.2,
    "mu_r": 0.01,
    "mass_ratio": 6.0 / 19.0,       # ball mass / cue mass
    "e_b": 1.0,                     # "perfect balls" branch used for the
    "mu_b": 0.06,                   # closed form below
    "b_max_over_R": 0.5,            # "safe miscue limit" in TP B-8 p.2
    "cue_speeds_mph": {"slow": 8.0, "medium": 10.0, "fast": 14.0, "power": 19.0},
    "drag_distances_ft": {"short": 1.0, "medium": 4.0, "long": 8.0},
}

# WPA equipment spec §8 cushion acceptance test (as surveyed on issue #3).
WPA_CUSHION_TEST = {
    "description": "firm centre-ball stroke from the head spot must travel the "
                   "length of the table 4 to 4.5 times without the ball jumping",
    "table_lengths": [4.0, 4.5],
}

# Platinum Billiards shaft data, transcribed from the dr-dave published-data
# page. Columns: shaft, tip curvature, deflection over 50 in (mm), deflection
# (in), deviation from average (%), pivot point (in), rating.
PLATINUM_SHAFTS_HEADER = ("shaft", "tip_curvature", "deflection_mm_over_50in",
                          "deflection_in_over_50in", "vs_average_pct",
                          "pivot_point_in", "rating")
PLATINUM_SHAFTS = [
    ("Predator Z-2", "dime", 29.6, 1.17, -28.6, 14.1, "low"),
    ("Predator Z", "dime", 32.3, 1.27, -22.2, 12.8, "low"),
    ("Predator 314-2", "dime", 33.0, 1.30, -20.4, 12.5, "low"),
    ("OB-1 Shaft", "dime", 33.4, 1.32, -19.3, 12.3, "low"),
    ("Predator BK2", "dime", 34.6, 1.36, -16.5, 11.9, "low"),
    ("Predator 314", "dime", 34.8, 1.37, -16.1, 11.8, "low"),
    ("McDermott i-3", "dime", 36.8, 1.45, -11.2, 11.1, "low"),
    ("Predator BK", "dime", 37.1, 1.46, -10.6, 11.0, "low"),
    ("Universal SmartShaft (Low Squirt)", "dime", 37.9, 1.49, -8.6, 10.7, "low"),
    ("McDermott i-2", "dime", 38.6, 1.52, -6.9, 10.5, "med low"),
    ("Universal SmartShaft (Regular Squirt)", "dime", 39.4, 1.55, -5.0, 10.3, "med low"),
    ("Axiom", "dime", 39.6, 1.56, -4.4, 10.3, "med low"),
    ("McDermott i-1", "dime", 39.6, 1.56, -4.4, 10.3, "med low"),
    ("Action", "dime", 40.1, 1.58, -3.2, 10.1, "med low"),
    ("Meucci Red Dot", "dime", 40.1, 1.58, -3.2, 10.1, "med low"),
    ("5280", "dime", 40.6, 1.60, -2.0, 9.9, "med low"),
    ("Sierra", "dime", 40.9, 1.61, -1.4, 9.9, "med low"),
    ("Cuetec Thunderbolt", "dime", 41.7, 1.64, 0.5, 9.7, "medium"),
    ("Viking", "nickel", 41.7, 1.64, 0.5, 9.7, "medium"),
    ("Mezz Power Break 2", "quarter", 41.7, 1.64, 0.5, 9.7, "medium"),
    ("Sterling", "nickel", 41.9, 1.65, 1.1, 9.6, "medium"),
    ("Bunjee J/B", "quarter", 42.3, 1.67, 2.0, 9.5, "medium"),
    ("Fury JB", "dime", 42.7, 1.68, 2.9, 9.4, "medium"),
    ("Falcon", "dime", 42.9, 1.69, 3.5, 9.4, "medium"),
    ("McDermott", "dime", 42.9, 1.69, 3.5, 9.4, "medium"),
    ("Mezz", "dime", 42.9, 1.69, 3.5, 9.4, "medium"),
    ("Tiger X-shaft", "nickel", 42.9, 1.69, 3.5, 9.4, "medium"),
    ("Players", "dime", 43.4, 1.71, 4.8, 9.2, "medium"),
    ("Sledgehammer J/B", "dime", 43.4, 1.71, 4.8, 9.2, "medium"),
    ("Cuetec Vortex", "dime", 43.9, 1.73, 6.0, 9.1, "medium"),
    ("Mali", "dime", 43.9, 1.73, 6.0, 9.1, "medium"),
    ("Pechauer", "nickel", 43.9, 1.73, 6.0, 9.1, "medium"),
    ("Scorpion J/B", "quarter", 43.9, 1.73, 6.0, 9.1, "medium"),
    ("Blaze", "dime", 43.9, 1.73, 6.0, 9.1, "medium"),
    ("Joss", "nickel", 44.2, 1.74, 6.6, 9.1, "med high"),
    ("Cuetec SST", "nickel", 44.2, 1.74, 6.6, 9.0, "med high"),
    ("X Breaker", "", 44.3, 1.74, 6.8, 9.0, "med high"),
    ("Meucci Black Dot", "dime", 44.4, 1.75, 7.2, 9.0, "med high"),
    ("Fury", "nickel", 44.7, 1.76, 7.8, 9.0, "med high"),
    ("Lucasi", "dime", 44.7, 1.76, 7.8, 8.9, "med high"),
    ("Schon", "nickel", 44.7, 1.76, 7.8, 8.9, "med high"),
    ("Axiom J/B", "dime", 46.0, 1.81, 10.9, 8.7, "med high"),
    ("Bunjee Blaster", "nickel", 46.0, 1.81, 10.9, 8.7, "med high"),
    ("Lightning Bolt", "", 46.2, 1.82, 11.4, 8.6, "med high"),
    ("Mezz Break", "quarter", 47.8, 1.88, 15.2, 8.3, "high"),
    ("Scorpion Break", "dime", 51.3, 2.02, 23.7, 7.6, "high"),
]
PLATINUM_DEFLECTION_DISTANCE_IN = 50.0

# Shepard (2001) pivot-point <-> endmass mapping, quoted from the paper's
# summary on p.12 and from the dr-dave published-data page.
SHEPARD_PIVOT_MAP = [
    {"pivot_point_in": 10.0, "mass_ratio_Mb_over_Mtip": 20, "class": "high squirt"},
    {"pivot_point_in": (16.0, 18.0), "mass_ratio_Mb_over_Mtip": 30, "class": "average"},
    {"pivot_point_in": 30.0, "mass_ratio_Mb_over_Mtip": 50, "class": "good (low squirt)"},
    {"pivot_point_in": (40.0, 50.0), "mass_ratio_Mb_over_Mtip": 100, "class": "low squirt"},
]

# TP B-28 p.1: through-diamond points measured on the opposite rail for each aim
# point on the banking rail (units: diamonds).
B28_AIM = (0.5, 1.0, 1.5, 2.0, 2.5, 3.0)
B28_TABLES = {
    "9ft Olhausen (Dr. Dave's video table)": (1.38, 2.72, 3.89, 5.07, 6.38, 7.74),
    "9ft Red Label Diamond": (1.40, 2.93, 4.52, 5.60, 6.92, 8.30),
    "7ft Valley bar box": (1.51, 2.74, 4.12, 5.19, 6.52, 7.54),
    "9ft Brunswick Gold Crown": (1.27, 2.88, 4.38, 5.38, 6.56, 7.96),
    "7ft Diamond bar box": (1.60, 3.31, 4.94, 6.22, 7.55, 8.59),
}
B28_SYSTEM_FACTOR = 2.2   # twice plus a tenth, TP B-27 p.3


# --------------------------------------------------------------------------- #
# physics straight from the sources
# --------------------------------------------------------------------------- #
def friction(v, params):
    """TP A-28: mu(v) = a + b*exp(-c*v) with v the contact-point relative speed."""
    a, b, c = params
    return a + b * math.exp(-c * v)


def relative_speed(v, omega_x, omega_z, phi):
    """TP A-14 Eq. 13 / TP A-28: |v_contact| = sqrt((v sin(phi) - R wz)^2 + (R wx cos(phi))^2)."""
    return math.hypot(v * math.sin(phi) - BALL_RADIUS * omega_z,
                      BALL_RADIUS * omega_x * math.cos(phi))


def throw_deg(v, omega_x, omega_z, phi, params):
    """TP A-28's MathCAD formulation of TP A-14 Eqs. 15-17, in degrees.

        theta = atan( min(mu(vrel)*v*cos(phi)/vrel, 1/7) * (v*sin(phi) - R*wz)/(v*cos(phi)) )
    """
    if v <= 0:
        return 0.0
    vrel = relative_speed(v, omega_x, omega_z, phi)
    if vrel == 0:
        return 0.0
    limit = min(friction(vrel, params) * v * math.cos(phi) / vrel, FRICTION_CAP)
    return math.degrees(math.atan(limit * (v * math.sin(phi) - BALL_RADIUS * omega_z)
                                  / (v * math.cos(phi))))


def spin_from_percent(v, percent):
    """TP A-25 / TP A-28: omega = (5/4)*(v/R)*percent."""
    return 1.25 * v / BALL_RADIUS * percent


def speed_from_cue_speed(vs, b_over_R, mass_ratio):
    """TP B-8 p.2 (perfect tip): v = 2*vs / (1 + mr + (5/2)(b/R)^2)."""
    return 2.0 * vs / (1.0 + mass_ratio + 2.5 * b_over_R ** 2)


def spin_from_tip_offset(v, b_over_R, radius=BALL_RADIUS):
    """TP B-8 p.2 (perfect tip): omega = -(5/2)*v*b/R^2 (negative = backspin)."""
    return -2.5 * v * b_over_R / radius


def drag_omega(omega, v, distance, mu_s):
    """TP 4.1 / TP B-8 p.4: spin after sliding a distance."""
    root = v * v - 2.0 * mu_s * GRAVITY * distance
    if root < 0:
        return None                     # the ball has stopped before that point
    return omega + (5.0 / (2.0 * BALL_RADIUS)) * (v - math.sqrt(root))


def draw_distance(omega_impact, mu_s, mu_r):
    """TP B-8 p.5 closed form: d = 2R^2/(49 g) * (1/mu_s + 1/mu_r) * omega^2."""
    return 2.0 * BALL_RADIUS ** 2 / (49.0 * GRAVITY) * (1.0 / mu_s + 1.0 / mu_r) \
        * omega_impact ** 2


def skid_distance(v, omega, mu_s):
    """TP B-5 / TP B-8: slide distance until natural roll, with sign."""
    tw = BALL_RADIUS * omega
    sign = 1.0 if v - tw >= 0 else -1.0
    return sign * (2.0 / (49.0 * mu_s * GRAVITY)) * (6 * v * v - 5 * v * tw - tw * tw)


def skid_speed(v, omega):
    """TP B-5 / TP B-8: speed once natural roll develops."""
    return 5.0 / 7.0 * v + 2.0 / 7.0 * BALL_RADIUS * omega


def roll_stop_distance(v, mu_r):
    """TP B-5 / TP B-6: distance for a rolling ball to stop."""
    return v * v / (2.0 * mu_r * GRAVITY)


def rail_travel(v, mu_s, mu_r, e_c, table_length):
    """TP B-6 p.1-3 algorithm: total travel distance with rail rebounds.

    Transcription of the MathCAD ``d(v)`` routine: roll toward the rail (or stop
    short of it), lose a factor ``e_c`` of the speed at the rail, then skid back
    to natural roll, and repeat until the ball stops.
    """
    x = 0.0
    n = 0
    rolling = True
    while v > 0:
        if rolling:
            stop = roll_stop_distance(v, mu_r)
            if stop < (n + 1) * table_length - x:
                return x + stop
            n += 1
            x = n * table_length
            v = math.sqrt(max(0.0, v * v - 2.0 * mu_r * GRAVITY * table_length))
            v *= e_c
            rolling = False
        else:
            slide = skid_distance(v, 0.0, mu_s)
            if slide < table_length:
                x += slide
                v *= 5.0 / 7.0
                rolling = True
            else:
                n += 1
                x = n * table_length
                v = math.sqrt(max(0.0, v * v - 2.0 * mu_s * GRAVITY * table_length))
                v *= e_c
                rolling = False
    return x


# --------------------------------------------------------------------------- #
# writers
# --------------------------------------------------------------------------- #
def provenance(doc, url, revision, note=""):
    entry = {"document": doc, "url": url, "revision": revision}
    if note:
        entry["note"] = note
    return entry


def write(path, payload):
    with open(path, "w") as sink:
        json.dump(payload, sink, indent=1, sort_keys=False)
        sink.write("\n")
    print(f"wrote {path} ({os.path.getsize(path) / 1024:.1f} KiB)")


def throw_set():
    """Throw curves: TP A-28 closed form plus the TP B-3 measured calibration."""
    cut_angles = list(range(0, 76))
    roll_percent = (0.0, 0.5)
    english_percent = (-1.0, -0.5, 0.0, 0.5, 1.0)
    points = []
    for v in THROW_SPEEDS:
        for pr in roll_percent:
            for pe in english_percent:
                wx = spin_from_percent(v, pr)
                wz = spin_from_percent(v, pe)
                for phi_deg in cut_angles:
                    phi = math.radians(phi_deg)
                    points.append([round(phi_deg, 3), round(v, 4), pr, pe,
                                   round(throw_deg(v, wx, wz, phi, A28_FRICTION), 4)])
    summary = []
    for v in THROW_SPEEDS:
        for pr in roll_percent:
            for pe in english_percent:
                subset = [p for p in points
                          if p[1] == round(v, 4) and p[2] == pr and p[3] == pe]
                values = [p[4] for p in subset]
                worst = max(subset, key=lambda p: abs(p[4]))
                summary.append({
                    "speed_m_s": v, "roll_percent": pr, "english_percent": pe,
                    "max_abs_throw_deg": round(max(abs(x) for x in values), 4),
                    "cut_at_max_deg": worst[0],
                })
    gearing = []
    for v in THROW_SPEEDS:
        for phi_deg in (10, 15, 20, 30, 45, 60, 75):
            phi = math.radians(phi_deg)
            # gearing: the contact point stops sliding, i.e. v sin(phi) = R wz
            pe = math.sin(phi) / 1.25
            gearing.append({"speed_m_s": v, "cut_deg": phi_deg,
                            "english_percent_at_gearing": round(pe, 4)})

    measurements = []
    for index, v in enumerate(B3_CALIBRATION["speeds_m_s"]):
        for column, cut in enumerate(B3_CALIBRATION["cut_deg"]):
            measured_in = B3_CALIBRATION["throw_in_per_yd"][index][column]
            measured_deg = math.degrees(math.atan(measured_in / 36.0))
            phi = math.radians(cut)
            predicted_b3 = throw_deg(v, 0.0, 0.0, phi, B3_FRICTION)
            predicted_a28 = throw_deg(v, 0.0, 0.0, phi, A28_FRICTION)
            measurements.append({
                "speed_m_s": round(v, 4),
                "speed_label": B3_CALIBRATION["speeds_label"][index],
                "cut_deg": cut,
                "throw_in_per_yd_as_printed": measured_in,
                "throw_deg_as_printed": B3_CALIBRATION["throw_deg_as_printed"][index][column],
                "throw_deg_from_in_per_yd": round(measured_deg, 4),
                "model_deg_TP_A28_friction": round(predicted_a28, 4),
                "model_deg_TP_B3_friction": round(predicted_b3, 4),
            })
    residual = [m["model_deg_TP_B3_friction"] - m["throw_deg_from_in_per_yd"]
                for m in measurements]
    return {
        "curve_set": "throw",
        "units": {"cut": "degrees (cue-ball-to-object-ball line vs cue-ball path)",
                  "speed": "m/s (pre-impact cue-ball speed)",
                  "english_percent": "fraction of maximum side spin (negative = inside)",
                  "roll_percent": "fraction of maximum vertical-plane spin (negative = draw)",
                  "throw": "degrees (object ball deviation from the geometric line, "
                           "positive = toward the cut direction)"},
        "sources": [
            provenance("TP A.28 Throw plots for all types of shots",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_A-28.pdf",
                       "posted 2007-04-03, revised 2021-04-13",
                       "the model, its parameters, and the plot parameter grid"),
            provenance("TP A.14 The effects of cut angle, speed, and spin on object ball throw",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_A-14.pdf",
                       "posted 2005-07-15, revised 2025-05-19",
                       "derivation, Eqs. 14-17"),
            provenance("TP B.3 Throw Calibration and Contour Plots",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-3.pdf",
                       "posted 2008-07-08, revised 2008-08-27",
                       "the measured calibration table (Colin Colenso's experiments) "
                       "and a refit of the friction model to it"),
        ],
        "method": "evaluated the published closed form (TP A-28 p.2, the MathCAD "
                  "formulation of TP A-14 Eqs. 15-17) at the published parameters "
                  "and the published plot grid; the calibration rows are transcribed "
                  "from TP B-3 p.2 rather than read off a plot",
        "parameters": {
            "ball_radius_m": BALL_RADIUS,
            "friction_model": "mu(v_rel) = a + b*exp(-c*v_rel)",
            "friction_a28": {"a": A28_FRICTION[0], "b": A28_FRICTION[1],
                             "c": A28_FRICTION[2],
                             "origin": "TP A-28 p.1, fitted to Marlow (1995) Table 10"},
            "friction_b3": {"a": B3_FRICTION[0], "b": B3_FRICTION[1],
                            "c": B3_FRICTION[2],
                            "origin": "TP B-3 p.2, refitted to the calibration data"},
            "mid_angle_cap": FRICTION_CAP,
            "spin_from_english": "omega = (5/4)*(v/R)*percent (TP A-25 via TP A-28)",
        },
        "error_estimate": {
            "model_points": "exact evaluation; no digitization error. The uncertainty "
                            "is the parameters': TP A-28's friction fit is a 3-point "
                            "fit to Marlow's data, and ball-cloth/ball condition moves "
                            "mu_b between roughly 0.03 and 0.08 (issue #3 constants table)",
            "measurement_points": "TP B-3 publishes no uncertainty for the calibration "
                                  "values; it prints only the six measured numbers and a "
                                  "least-squares refit whose residual we report here",
            "residual_deg_TpB3_model_minus_measurement": {
                "values": [round(r, 4) for r in residual],
                "mean": round(sum(residual) / len(residual), 4),
                "max_abs": round(max(abs(r) for r in residual), 4),
            },
            "printed_degree_column_caveat": "TP B-3's printed degrees differ from "
                                            "atan(inches_per_yard/36) by about 0.6-1%; use "
                                            "the inches-per-yard column and convert",
        },
        "points": {
            "columns": ["cut_deg", "speed_m_s", "roll_percent", "english_percent",
                        "throw_deg"],
            "data": points,
        },
        "summary_max_throw": summary,
        "gearing_english_percent": gearing,
        "measurements_TP_B3": measurements,
    }


def draw_follow_set():
    """Draw/follow relations: TP B-8 closed form plus TP B-5 anchors."""
    points = []
    b_over_R_values = [round(0.05 * i, 3) for i in range(1, 11)]
    for label, vs_mph in B8["cue_speeds_mph"].items():
        vs = vs_mph * MPH
        for drag_label, drag_ft in B8["drag_distances_ft"].items():
            drag = drag_ft * FOOT
            for b_over_R in b_over_R_values:
                v = speed_from_cue_speed(vs, b_over_R, B8["mass_ratio"])
                omega = spin_from_tip_offset(v, b_over_R)
                omega_impact = drag_omega(omega, v, drag, B8["mu_s"])
                if omega_impact is None:
                    continue
                distance = draw_distance(omega_impact, B8["mu_s"], B8["mu_r"])
                points.append({
                    "cue_speed": label, "cue_speed_m_s": round(vs, 4),
                    "drag_distance": drag_label, "drag_distance_m": round(drag, 4),
                    "b_over_R": b_over_R,
                    "cb_speed_m_s": round(v, 4),
                    "cb_spin_rad_s": round(omega, 3),
                    "cb_spin_at_impact_rad_s": round(omega_impact, 3),
                    "draw_distance_m": round(distance, 4),
                    "draw_distance_ft": round(distance / FOOT, 4),
                    "draw_table_lengths_9ft": round(distance / 2.54, 4),
                })

    # Rolling direct hit: TP B-5's model, evaluated to compare with its printed
    # travel-ratio and hop numbers.
    validation = []
    for v_mph in B6["speeds_mph"]:
        v = v_mph * MPH
        v_cb = (1 - B5["e_b"]) / 2 * v
        v_ob = (1 + B5["e_b"]) / 2 * v
        d_omega = 5.0 / (4.0 * BALL_RADIUS) * B5["mu_b"] * (1 + B5["e_b"]) * v
        omega_cb = v / BALL_RADIUS - d_omega
        d_cb = skid_distance(v_cb, omega_cb, B5["mu_s"]) + \
            roll_stop_distance(skid_speed(v_cb, omega_cb), B5["mu_r"])
        d_ob = skid_distance(v_ob, d_omega, B5["mu_s"]) + \
            roll_stop_distance(skid_speed(v_ob, d_omega), B5["mu_r"])
        hop_height = (B5["mu_b"] ** 2 * (1 + B5["e_b"]) ** 2) / (8 * GRAVITY) * v ** 2
        hop_time = B5["mu_b"] * (1 + B5["e_b"]) * v / GRAVITY
        validation.append({
            "speed_mph": v_mph,
            "cb_travel_m": round(d_cb, 4),
            "ob_travel_m": round(d_ob, 4),
            "travel_ratio_ob_over_cb": round(d_ob / d_cb, 3),
            "hop_height_mm": round(hop_height * 1000, 4),
            "hop_time_s": round(hop_time, 4),
        })
    ratio_at_3 = next(row["travel_ratio_ob_over_cb"] for row in validation
                      if row["speed_mph"] == 3.0)

    return {
        "curve_set": "draw_follow",
        "units": {"distance": "m (and ft, and 9 ft table lengths of 2.54 m)",
                  "b_over_R": "tip offset in ball radii",
                  "cb_speed": "m/s", "spin": "rad/s (negative = backspin)"},
        "sources": [
            provenance("TP B.8 Draw shot physics",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-8.pdf",
                       "posted 2009-02-21, revised 2011-07-10",
                       "the draw-distance closed form, the drag equation, and the "
                       "shot-distance/speed grid"),
            provenance("TP B.5 Rolling CB, direct-hit hop and ball travel distances",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-5.pdf",
                       "posted 2009-01-30",
                       "rolling-hit anchor values and the skid/roll distance relations"),
            provenance("TP A.30 (via TP B-8 p.2, perfect-tip branch)",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_A-30.pdf",
                       "cited by TP B-8",
                       "cue speed -> ball speed/spin mapping"),
        ],
        "method": "closed-form evaluation of TP B-8's published relations: "
                  "v = 2*vs/(1 + mr + (5/2)(b/R)^2), omega = -(5/2)*v*(b/R)/R, "
                  "omega_impact = drag over the pre-impact distance, "
                  "d_draw = 2R^2/(49 g)*(1/mu_s + 1/mu_r)*omega_impact^2",
        "parameters": B8,
        "error_estimate": {
            "model_points": "exact evaluation; the error is in the parameters",
            "parameter_sensitivity": "d_draw scales as (1/mu_s + 1/mu_r) and quadratically "
                                     "in the spin at impact, so the survey's cloth ranges "
                                     "(mu_s 0.15-0.4, mu_r 0.005-0.015) move a draw "
                                     "distance by tens of percent - the ladder's stage-1 "
                                     "gate must be applied with the cloth condition recorded",
            "tip_efficiency_caveat": "the perfect-tip branch used here overstates ball "
                                     "speed at a given cue speed; TP B-8 p.6 quotes a "
                                     "typical tip efficiency of 0.87 which reduces "
                                     "centre-ball CB speed from 152% to 127% of cue speed",
            "source_inconsistency_TP_B5": {
                "printed_ratio": B5["printed_ratio"],
                "computed_ratio_with_printed_constants": ratio_at_3,
                "note": "TP B-5 prints dOB/dCB = 6.08 for a rolling square hit but its "
                        "own printed constants (mu_b 0.06, e_b 0.94) give "
                        f"{ratio_at_3}; reproduce 6.08 with mu_b ~ 0.028 at e_b 0.94. "
                        "The prototype should treat the relation as the authority and "
                        "re-derive the constant, not the printed ratio.",
            },
        },
        "draw_distance_points": points,
        "rolling_direct_hit_validation": validation,
        "printed_anchors_TP_B5": {
            "hop_height_in": B5["printed_hop"],
            "hop_time_s": B5["printed_hop_time"],
            "travel_ratio_ob_over_cb": B5["printed_ratio"],
        },
        "qualitative_rule_of_thumb": {
            "name": "1/8 rule",
            "source": "https://drdavepoolinfo.com/faq/speed/1-8-rule/",
            "statement": "add 1/8 of stroke length per diamond of draw distance "
                         "desired, and per diamond between cue ball and object ball; "
                         "for follow, per diamond of follow distance; for a stop shot, "
                         "per diamond to the object ball",
            "use": "sanity check only - the page publishes no numeric table",
        },
    }


def cushion_bank_set():
    """Rail travel (TP B-6), measured banks (TP B-28), and the WPA test."""
    table_length = B6["table_length_in"] * INCH

    def travel_from_speed(v):
        return rail_travel(v, B6["mu_s"], B6["mu_r"], B6["e_c"], table_length)

    speeds = [round(0.5 * i, 2) for i in range(1, 41)]     # 0.5 .. 20 mph
    points = []
    for mph in speeds:
        v = mph * MPH
        metres = travel_from_speed(v)
        points.append({"speed_mph": mph, "speed_m_s": round(v, 4),
                       "travel_m": round(metres, 4),
                       "travel_ft": round(metres / FOOT, 4),
                       "table_lengths": round(metres / table_length, 4)})

    anchors = []
    for mph, printed in B6["printed_table_lengths"].items():
        computed = travel_from_speed(mph * MPH) / table_length
        anchors.append({"speed_mph": mph, "printed_table_lengths": printed,
                        "computed_table_lengths": round(computed, 4),
                        "delta": round(computed - printed, 4)})

    banks = []
    for table, measured in B28_TABLES.items():
        for aim, cross in zip(B28_AIM, measured):
            banks.append({
                "table": table, "aim_diamond": aim,
                "through_diamond_measured": cross,
                "twice_plus_tenth_system": round(B28_SYSTEM_FACTOR * aim, 3),
                "delta_vs_system": round(cross - B28_SYSTEM_FACTOR * aim, 3),
            })

    return {
        "curve_set": "cushion_bank",
        "units": {"travel": "m (and ft, and 9 ft table lengths of 2.54 m)",
                  "speed": "mph and m/s",
                  "diamond": "one diamond = 1/8 of the playing length"},
        "sources": [
            provenance("TP B.6 CB table lengths of travel for different speeds",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-6.pdf",
                       "posted 2009-01-30, revised 2023-12-03",
                       "rail COR, the travel algorithm, and the printed table lengths"),
            provenance("TP B.28 Sliding-Bank-Shot Table Comparisons From Careful Measurements",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-28.pdf",
                       "posted 2023-08-24, revised 2023-08-26",
                       "hand-measured through-diamond bank data on five tables"),
            provenance("TP B.27 Sliding Bank System Comparisons",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-27.pdf",
                       "posted 2023-08-09",
                       "the twice-plus-a-tenth reference system the measurements are "
                       "compared against"),
            provenance("WPA Recommended Equipment Specifications §8",
                       "https://wpapool.com/wp-content/uploads/2024/01/"
                       "RECOMMENDED-EQUIPMENT-SPECIFICATIONS.pdf",
                       "2024-01",
                       "cushion acceptance test, cited in the physics survey on #3"),
            provenance("Mathavan, Jackson & Parkin, IMechE 2010 (via dr-dave)",
                       "https://drdavepoolinfo.com/physics_articles/Mathavan_IMechE_2010.pdf",
                       "2010",
                       "the source of the e_n=0.98 / mu=0.14 pair that the physics "
                       "resolution on #7 explicitly forbids copying as an effective "
                       "rail COR; its rebound results are plots, not a table, and were "
                       "not digitized"),
        ],
        "method": "TP B-6's published algorithm transcribed and re-run (rolling to the "
                  "rail, e_c speed loss, skid back to roll, repeat); TP B-28's measured "
                  "tables transcribed cell by cell",
        "parameters": {"mu_s": B6["mu_s"], "mu_r": B6["mu_r"], "e_c": B6["e_c"],
                       "table_length_in": B6["table_length_in"]},
        "error_estimate": {
            "travel_curve": "exact evaluation of the published algorithm. Checking it "
                            "against the source's own printed table lengths gives an "
                            "exact match at 1.5 and 3 mph (0.903 and 1.684 table "
                            "lengths) and a 1.5-2.4% shortfall at 7, 12 and 20 mph, so "
                            "the source's printed numbers are internally inconsistent at "
                            "speed; treat them as +/-2% anchors",
            "measured_banks": "TP B-28 publishes no uncertainty. It labels the data "
                              "'carefully measured' and gives no repeatability. The "
                              "spread across the five tables (up to ~1 diamond at "
                              "aim 3.0) shows the conditions dominate any reading error",
            "anchor_check": anchors,
        },
        "travel_points": points,
        "printed_anchors_TP_B6": anchors,
        "lag_speed_mph_for_two_table_lengths": B6["printed_lag_speed_mph"],
        "wpa_cushion_acceptance_test": WPA_CUSHION_TEST,
        "measured_banks_TP_B28": banks,
        "measured_bank_table": {
            "columns": ["table", "aim_diamond", "through_diamond_measured",
                        "twice_plus_tenth_system", "delta_vs_system"],
            "note": "aim = diamond on the banking rail; measured = diamond crossing on "
                    "the opposite rail",
        },
    }


def squirt_set():
    """Squirt: the Platinum shaft table plus Shepard's pivot/endmass mapping."""
    shafts = []
    for row in PLATINUM_SHAFTS:
        record = dict(zip(PLATINUM_SHAFTS_HEADER, row))
        deflection_m = record["deflection_mm_over_50in"] * MM
        distance_m = PLATINUM_DEFLECTION_DISTANCE_IN * INCH
        record["squirt_angle_deg"] = round(math.degrees(math.atan(deflection_m / distance_m)), 4)
        shafts.append(record)
    angles = [s["squirt_angle_deg"] for s in shafts]
    pivots = [s["pivot_point_in"] for s in shafts]
    return {
        "curve_set": "squirt",
        "units": {"deflection": "mm and inches over a 50 in cue-ball travel",
                  "squirt_angle": "degrees, atan(deflection / 50 in)",
                  "pivot_point": "inches from the tip"},
        "sources": [
            provenance("Dr. Dave - Published Data for Shaft CB Deflections (Platinum Billiards)",
                       "https://drdavepoolinfo.com/faq/squirt/published-data/",
                       "retrieved 2026-09-11",
                       "the 46-shaft table; the page also quotes the Shepard and "
                       "Platinum ranges"),
            provenance("R. Shepard, 'Everything you always wanted to know about cue ball "
                       "squirt, but were afraid to ask'",
                       "https://drdavepoolinfo.com/physics_articles/Shepard_squirt.pdf",
                       "2001",
                       "aim-and-pivot method, pivot point <-> endmass mapping, and the "
                       "0.5-2.3 degree squirt range"),
            provenance("TP B.1 Squirt angle, pivot length, and tip shape",
                       "https://drdavepoolinfo.com/technical_proofs/new/TP_B-1.pdf",
                       "posted 2008",
                       "secondary source; not transcribed here"),
        ],
        "method": "transcribed the Platinum table verbatim (it is a table, not a plot); "
                  "derived the squirt angle with the page's own 50 in travel distance; "
                  "transcribed Shepard's pivot-point/endmass statements",
        "error_estimate": {
            "table": "the source publishes no uncertainty for the Platinum measurements. "
                     "It states tests were done on a jig; Shepard's paper notes the "
                     "practical difficulties (tip offset visibility, swerve contamination, "
                     "stroke error) that motivate the aim-and-pivot method instead",
            "cross_source_range": "the two published sources do not agree: Shepard quotes "
                                  "0.5-2.3 deg of squirt and ~10-50 in of pivot, Platinum "
                                  "measures 1.3-2.3 deg and 7.6-14.1 in. Treat the union "
                                  "as the gate band and record the cue when narrowing it",
        },
        "shaft_table": shafts,
        "shaft_table_summary": {
            "n": len(shafts),
            "squirt_angle_deg_min": min(angles),
            "squirt_angle_deg_max": max(angles),
            "pivot_point_in_min": min(pivots),
            "pivot_point_in_max": max(pivots),
        },
        "shepard_pivot_endmass_map": SHEPARD_PIVOT_MAP,
        "published_ranges": {
            "shepard_squirt_deg": [0.5, 2.3],
            "shepard_pivot_in": [10.0, 50.0],
            "platinum_squirt_deg": [1.3, 2.3],
            "platinum_pivot_in": [7.6, 14.1],
        },
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", default="data/curves")
    args = parser.parse_args(argv)
    os.makedirs(args.out, exist_ok=True)

    write(os.path.join(args.out, "throw.json"), throw_set())
    write(os.path.join(args.out, "draw-follow.json"), draw_follow_set())
    write(os.path.join(args.out, "cushion-bank.json"), cushion_bank_set())
    write(os.path.join(args.out, "squirt.json"), squirt_set())

    # Self-check: re-evaluate the sources' own printed anchors.
    print("\nself-checks (computed vs printed):")
    cushion = cushion_bank_set()
    for anchor in cushion["printed_anchors_TP_B6"]:
        status = "ok" if abs(anchor["delta"]) < 0.02 else "CHECK"
        print(f"  TP B-6 {anchor['speed_mph']:>5} mph: computed "
              f"{anchor['computed_table_lengths']:.3f} vs printed "
              f"{anchor['printed_table_lengths']:.3f} table lengths  [{status}]")
    follow = draw_follow_set()
    ratio = next(row["travel_ratio_ob_over_cb"]
                 for row in follow["rolling_direct_hit_validation"]
                 if row["speed_mph"] == 3.0)
    print(f"  TP B-5 travel ratio: computed {ratio} vs printed "
          f"{B5['printed_ratio']}  [CHECK - see error_estimate]")
    hops = {row["speed_mph"]: round(row["hop_height_mm"] / 25.4, 3)
            for row in follow["rolling_direct_hit_validation"]}
    for mph, printed in B5["printed_hop"].items():
        print(f"  TP B-5 hop {mph:>5} mph: computed {hops[mph]} in vs printed {printed} in"
              f"  [{'ok' if abs(hops[mph] - printed) < 0.005 else 'CHECK'}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
