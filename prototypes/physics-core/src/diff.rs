//! Differential run against pooltool (see tools/pooltool_diff.py): the same
//! four synthetic shots in the same frame, written in the same JSON shape so
//! the two engines can be diffed before any real-data fitting.

use crate::consts::*;
use crate::facts::derive;
use crate::shots::{break_setup, two_ball_setup, Arrangement, ShotSetup};
use crate::sim::{FactKind, Sim};
use crate::strike::Strike;
use crate::table::Table;
use crate::vec::{v3, V3};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

fn run(setup: &ShotSetup) -> (Sim, crate::sim::Outcome, crate::facts::ShotFacts) {
    let pivot = setup.profile.pivot_mm;
    let mut sim = Sim::new(setup.profile.clone(), setup.table.clone(), &setup.positions);
    let res = setup.strike.resolve(pivot);
    sim.strike(res.v, res.w);
    let o = sim.run_to_rest();
    let f = derive(&sim);
    (sim, o, f)
}

fn seed1() -> Arrangement {
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
    arr
}

fn shot_json(name: &str, setup: &ShotSetup, break_speed: f64) -> String {
    let (sim, o, f) = run(setup);
    let mut rest = String::new();
    for (i, b) in sim.balls.iter().enumerate() {
        if i > 0 {
            rest.push(',');
        }
        let id = if b.id == 0 {
            "\"cue\"".to_string()
        } else {
            format!("\"{}\"", b.id)
        };
        rest.push_str(&format!(
            "{}:{{\"x_mm\":{:.3},\"y_mm\":{:.3},\"speed\":{:.6}}}",
            id,
            b.p.x,
            b.p.y,
            b.v.len() / 1000.0
        ));
    }
    let pocketed: Vec<String> = f
        .pocketed
        .iter()
        .map(|b| format!("\"{b}\""))
        .collect();
    let bb = sim
        .facts
        .iter()
        .filter(|x| matches!(x.kind, FactKind::BallBall { .. }))
        .count();
    let cu = sim
        .facts
        .iter()
        .filter(|x| matches!(x.kind, FactKind::RailContact { ball: 0, .. }))
        .count();
    let cushion = sim
        .facts
        .iter()
        .filter(|x| matches!(x.kind, FactKind::RailContact { .. }))
        .count();
    let _ = cu;
    format!(
        "{{\"name\":\"{name}\",\"t_rest\":{:.4},\"events\":{},\"ball_ball\":{},\"cushion\":{},\"pocketed\":[{}],\"rest\":{{{}}},\"note\":\"break_speed {break_speed}\"}}",
        o.t_rest, o.events, bb, cushion, pocketed.join(","), rest
    )
}

pub fn run_diff(out: &Path) {
    let profile = Profile::spec_default();
    let mut shots: Vec<String> = Vec::new();
    // 1. straight stun 1.4 m/s
    let s = two_ball_setup(profile.clone(), v3(0.0, 0.0, R), v3(200.0, 0.0, R), 1, 0.0, 1400.0, 0.0, 0.0, 0.0);
    shots.push(shot_json("straight_stun_1p4", &s, 0.0));
    // 2. 30 degree cut with follow
    let s = two_ball_setup(profile.clone(), v3(0.0, 0.0, R), v3(200.0, 28.575, R), 1, 0.0, 1400.0, 0.0, 0.5, 0.0);
    shots.push(shot_json("cut30_follow_1p4", &s, 0.0));
    // 3. rolling ball square into the far cushion (aim +x)
    let s = ShotSetup {
        profile: profile.clone(),
        table: Table::new(),
        positions: vec![(0, v3(0.0, 400.0, R))],
        strike: Strike::new(0.0, 1500.0, 0.0, 0.4, 0.0),
        frozen_note: None,
    };
    shots.push(shot_json("cushion_roll_1p5", &s, 0.0));
    // 4. the rack break
    let s = break_setup(
        profile.clone(),
        &seed1(),
        v3(-800.0, 0.0, R),
        0.0,
        6200.0,
        0.0,
        0.0,
        0.0,
    );
    shots.push(shot_json("break_rack", &s, 6.2));
    let json = format!(
        "{{\"engine\":\"physics-core\",\"params\":{{\"u_s\":{},\"u_r\":{},\"u_b\":{},\"e_b\":{},\"e_c\":{},\"f_c\":{},\"g\":{}}},\"shots\":[{}]}}",
        profile.mu_s,
        profile.mu_r,
        profile.mu_b.eval(1000.0),
        profile.e_b,
        profile.e_cushion,
        profile.mu_cushion,
        G / 1000.0,
        shots.join(",")
    );
    let mut f = std::fs::File::create(out.join("rust-diff.json")).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    println!("wrote {}/rust-diff.json", out.display());
    for l in &shots {
        println!("{l}");
    }
}
