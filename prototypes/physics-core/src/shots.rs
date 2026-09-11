//! Shot construction helpers and the named presets used by the visual
//! artefacts and the acceptance hooks.

use crate::consts::*;
use crate::facts::{derive, ShotFacts};
use crate::json::Json;
use crate::sim::{Outcome, Sim};
use crate::strike::Strike;
use crate::table::{parse_slot, rack_slot, Table};
use crate::vec::{v3, V3};
use std::collections::BTreeMap;

pub type Arrangement = BTreeMap<String, String>;

/// Ball id -> rack position from an arrangement (ball number -> "r.k").
pub fn arrangement_positions(arr: &Arrangement) -> Vec<(u8, V3)> {
    let mut out: Vec<(u8, V3)> = Vec::new();
    for (ball, slot) in arr.iter() {
        let id: u8 = ball.parse().unwrap();
        let (r, k) = parse_slot(slot);
        let slot = rack_slot(r, k);
        out.push((id, v3(slot.x, slot.y, R)));
    }
    out.sort_by_key(|(id, _)| *id);
    out
}

pub fn read_arrangement(v: &Json) -> Arrangement {
    let mut m = BTreeMap::new();
    if let Some(o) = v.as_obj() {
        for (k, val) in o {
            m.insert(k.clone(), val.as_str().unwrap_or("").to_string());
        }
    }
    m
}

pub struct ShotSetup {
    pub profile: Profile,
    pub table: Table,
    pub positions: Vec<(u8, V3)>,
    pub strike: Strike,
    pub frozen_note: Option<String>,
}

pub struct ShotResult {
    pub sim: Sim,
    pub facts: ShotFacts,
    pub outcome: Outcome,
    pub aim_deg: f64,
}

/// Run one shot to rest without a timeline (search / sweep mode).
pub fn run_shot(setup: &ShotSetup, record_timeline: bool) -> ShotResult {
    let mut sim = Sim::new(setup.profile.clone(), setup.table.clone(), &setup.positions);
    sim.record_timeline = record_timeline;
    sim.annotate_frozen();
    let res = setup.strike.resolve(setup.profile.pivot_mm);
    sim.strike(res.v, res.w);
    let outcome = sim.run_to_rest();
    let facts = derive(&sim);
    ShotResult {
        sim,
        facts,
        outcome,
        aim_deg: setup.strike.aim.y.atan2(setup.strike.aim.x).to_degrees(),
    }
}

/// Aim angle in degrees from a cue-ball position to a target point.
pub fn aim_to(from: V3, to: V3) -> f64 {
    (to.y - from.y).atan2(to.x - from.x).to_degrees()
}

/// A rack-only arrangement plus the cue ball at `cue`.
pub fn break_setup(
    profile: Profile,
    arr: &Arrangement,
    cue: V3,
    aim_deg: f64,
    speed: f64,
    a_tip: f64,
    b_tip: f64,
    elevation_deg: f64,
) -> ShotSetup {
    let mut positions = vec![(0u8, v3(cue.x, cue.y, R))];
    positions.extend(arrangement_positions(arr));
    ShotSetup {
        profile,
        table: Table::new(),
        positions,
        strike: Strike::new(aim_deg, speed, a_tip, b_tip, elevation_deg),
        frozen_note: None,
    }
}

/// A single object ball on the table plus the cue ball (stroked shots).
pub fn two_ball_setup(
    profile: Profile,
    cue: V3,
    obj: V3,
    obj_id: u8,
    aim_deg: f64,
    speed: f64,
    a_tip: f64,
    b_tip: f64,
    elevation_deg: f64,
) -> ShotSetup {
    ShotSetup {
        profile,
        table: Table::new(),
        positions: vec![(0u8, v3(cue.x, cue.y, R)), (obj_id, v3(obj.x, obj.y, R))],
        strike: Strike::new(aim_deg, speed, a_tip, b_tip, elevation_deg),
        frozen_note: None,
    }
}

/// Preset shots for the rendered frames and the acceptance hooks.
pub fn preset(name: &str, profile: Profile) -> ShotSetup {
    match name {
        "break" => {
            // the golden fixture rack (seed 1 of rack-fixtures.json)
            let mut arr: Arrangement = BTreeMap::new();
            for (ball, slot) in [
                ("1", "1.0"),
                ("2", "4.1"),
                ("3", "3.2"),
                ("4", "2.0"),
                ("5", "5.2"),
                ("6", "5.3"),
                ("7", "5.4"),
                ("8", "3.1"),
                ("9", "2.1"),
                ("10", "5.1"),
                ("11", "4.0"),
                ("12", "4.3"),
                ("13", "4.2"),
                ("14", "5.0"),
                ("15", "3.0"),
            ] {
                arr.insert(ball.to_string(), slot.to_string());
            }
            break_setup(profile, &arr, v3(-800.0, 0.0, R), 0.0, 6200.0, 0.0, 0.0, 0.0)
        }
        "draw" => {
            // long straight draw: full hit, below centre
            let cue = v3(-635.0, 0.0, R);
            let obj = v3(635.0, 0.0, R);
            let aim = aim_to(cue, obj);
            two_ball_setup(profile, cue, obj, 1, aim, 3000.0, 0.0, -0.4, 0.0)
        }
        "follow" => {
            let cue = v3(-635.0, 0.0, R);
            let obj = v3(635.0, 0.0, R);
            let aim = aim_to(cue, obj);
            two_ball_setup(profile, cue, obj, 1, aim, 3000.0, 0.0, 0.5 * 0.9, 0.0)
        }
        "cut" => {
            // a half-ball cut with follow: the throw shot
            let cue = v3(-635.0, 0.0, R);
            let obj = v3(200.0, 28.575, R);
            let aim = aim_to(cue, obj);
            two_ball_setup(profile, cue, obj, 1, aim, 1500.0, 0.0, 0.25, 0.0)
        }
        "bank" => {
            // one-cushion bank at 45 degrees with running english
            let cue = v3(-400.0, -300.0, R);
            let obj = v3(200.0, 200.0, R);
            let aim = aim_to(cue, obj);
            two_ball_setup(profile, cue, obj, 1, aim, 2500.0, 0.35, 0.2, 0.0)
        }
        other => panic!("unknown preset {other}"),
    }
}
