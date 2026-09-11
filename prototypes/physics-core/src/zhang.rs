//! Stage 5 (end-to-end Zhang replay) scaffolding: the prototype consumes
//! strikes extracted from the Kinovea tracks by tools/zhang_extract.py and
//! replays them, comparing rest positions after a written normalisation.

use crate::consts::*;
use crate::shots::ShotSetup;
use crate::sim::Sim;
use crate::strike::Strike;
use crate::table::Table;
use crate::vec::{v3, V3};
use std::io::Write;
use std::path::Path;

pub fn replay(strikes_path: &str, out: &Path, limit: usize) {
    if strikes_path.is_empty() {
        println!("zhang: no --strikes file given; skipping stage 5");
        return;
    }
    let src = std::fs::read_to_string(strikes_path).expect("strikes json");
    let j = crate::json::parse(&src).expect("parse");
    let rows = j.as_arr().cloned().unwrap_or_default();
    let mut handled = 0usize;
    let mut dev: Vec<f64> = Vec::new();
    let mut outcome_hits = 0usize;
    let mut outcome_total = 0usize;
    let mut lines: Vec<String> = Vec::new();
    for row in rows.iter() {
        if handled >= limit {
            break;
        }
        let cx = row.f("cue_x_mm");
        let cy = row.f("cue_y_mm");
        let ox = row.f("obj_x_mm");
        let oy = row.f("obj_y_mm");
        let speed = row.f("speed_mm_s");
        if !cx.is_finite() || !speed.is_finite() || speed <= 0.0 {
            continue;
        }
        let aim = (oy - cy).atan2(ox - cx);
        let setup = ShotSetup {
            profile: Profile::spec_default(),
            table: Table::new(),
            positions: vec![(0, v3(cx, cy, R)), (1, v3(ox, oy, R))],
            strike: Strike::new(aim.to_degrees(), speed, row.f("a_tip"), row.f("b_tip"), 0.0),
            frozen_note: None,
        };
        if setup.strike.validate().is_err() {
            continue;
        }
        let pivot = setup.profile.pivot_mm;
        let mut sim = Sim::new(setup.profile.clone(), Table::new(), &setup.positions);
        let res = setup.strike.resolve(pivot);
        sim.strike(res.v, res.w);
        sim.run_to_rest();
        let rest = sim.state_at(sim.outcome.t_rest);
        let want_cx = row.f("cue_rest_x_mm");
        let want_cy = row.f("cue_rest_y_mm");
        let d = ((rest[0].p.x - want_cx).powi(2) + (rest[0].p.y - want_cy).powi(2)).sqrt();
        if d.is_finite() {
            dev.push(d);
            handled += 1;
            outcome_total += 1;
            let hit = row.b("obj_pocketed") == rest[1].pocketed;
            if hit {
                outcome_hits += 1;
            }
            lines.push(format!(
                "{{\"shot\":\"{}\",\"rest_dev_mm\":{:.1},\"obj_pocketed_model\":{},\"obj_pocketed_data\":{},\"outcome_match\":{}}}",
                row.s("shot"),
                d,
                rest[1].pocketed,
                row.b("obj_pocketed"),
                hit
            ));
        }
    }
    dev.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if dev.is_empty() { f64::NAN } else { dev[dev.len() / 2] };
    let p90 = if dev.is_empty() {
        f64::NAN
    } else {
        dev[(((dev.len() as f64) * 0.9) as usize).min(dev.len() - 1)]
    };
    println!(
        "zhang replay: {} shots, rest-position deviation median {:.1} mm, p90 {:.1} mm; outcome agreement {}/{}",
        dev.len(),
        median,
        p90,
        outcome_hits,
        outcome_total
    );
    let mut f = std::fs::File::create(out.join("zhang-replay.txt")).unwrap();
    writeln!(
        f,
        "shots={}\nmedian_mm={:.2}\np90_mm={:.2}\noutcome_agreement={}/{}\n",
        dev.len(),
        median,
        p90,
        outcome_hits,
        outcome_total
    )
    .unwrap();
    let mut f = std::fs::File::create(out.join("zhang-replay.jsonl")).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
    let _ = V3::ZERO;
}
