//! Diagnostics and the rendered artefacts: energy behaviour, tunnelling,
//! the break acceptance hook, the two pinned numbers (sleep thresholds and the
//! rest-position tolerance), preset event logs and PNG frames.

use crate::consts::*;
use crate::facts::derive;
use crate::render::*;
use crate::shots::{preset, ShotSetup};
use crate::sim::{BallState, FactKind, Sim};
use crate::table::Table;
use crate::vec::{v3, V3};
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn kinetic_energy(_profile: &Profile, s: &BallState) -> f64 {
    let v2 = s.v.dot(s.v);
    // rotational: 1/2 w . I w  with I = (2/5) m R^2 on every axis
    let w2 = s.w.dot(s.w);
    0.5 * M_BALL * v2 + 0.5 * I_BALL * w2 + M_BALL * G * (s.p.z - R)
}

pub struct RunSummary {
    pub t_rest: f64,
    pub events: u64,
    pub groups: u64,
    pub ke0: f64,
    pub ke_rest: f64,
    pub max_pen_mm: f64,
    pub min_gap_mm: f64,
    pub max_hop_mm: f64,
    pub runaway: Option<String>,
    pub ke_jump_max: f64,
    pub first_rail_t: f64,
}

pub fn run_and_summarize(setup: &ShotSetup, timeline: bool) -> (Sim, RunSummary) {
    let profile = setup.profile.clone();
    let mut sim = Sim::new(profile.clone(), setup.table.clone(), &setup.positions);
    sim.record_timeline = timeline;
    sim.annotate_frozen();
    let res = setup.strike.resolve(profile.pivot_mm);
    sim.strike(res.v, res.w);
    let st0 = sim.state_at(0.0);
    let ke0: f64 = st0.iter().map(|s| kinetic_energy(&profile, s)).sum();
    let mut ke_jump_max = 0.0f64;
    let mut prev_ke = ke0;
    let mut first_rail_t = f64::INFINITY;
    // energy walk over the timeline (each entry marks a law change)
    let outcome = sim.run_to_rest();
    let snaps = sim.timeline.clone();
    for (t, laws) in &snaps {
        let states: Vec<BallState> = sim.state_at(*t);
        let ke: f64 = states.iter().map(|s| kinetic_energy(&profile, s)).sum();
        if ke > prev_ke {
            let jump = (ke - prev_ke) / ke0.max(1e-9);
            if jump > ke_jump_max {
                ke_jump_max = jump;
            }
        }
        prev_ke = ke;
        let _ = laws;
    }
    for f in &sim.facts {
        if let FactKind::RailContact { ball, .. } = f.kind {
            if ball != 0 && f.t < first_rail_t {
                first_rail_t = f.t;
            }
        }
    }
    let states = sim.state_at(outcome.t_rest);
    let ke_rest: f64 = states.iter().map(|s| kinetic_energy(&profile, s)).sum();
    let sum = RunSummary {
        t_rest: outcome.t_rest,
        events: outcome.events,
        groups: outcome.groups,
        ke0,
        ke_rest,
        max_pen_mm: outcome.max_penetration_mm,
        min_gap_mm: outcome.min_pair_gap_mm,
        max_hop_mm: outcome.max_hop_mm,
        runaway: outcome.runaway.clone(),
        ke_jump_max,
        first_rail_t,
    };
    (sim, sum)
}

pub fn write_trajectory(sim: &Sim, t_rest: f64, path: &Path, dt: f64) {
    let mut f = File::create(path).expect("traj file");
    writeln!(f, "t,ball,x_mm,y_mm,z_mm,v_mm_s,mode").unwrap();
    let mut t = 0.0;
    while t <= t_rest + 1e-9 {
        let states = sim.state_at(t);
        for (i, s) in states.iter().enumerate() {
            if s.pocketed || s.off_table {
                continue;
            }
            writeln!(
                f,
                "{:.5},{},{:.3},{:.3},{:.3},{:.1},{:?}",
                t,
                i,
                s.p.x,
                s.p.y,
                s.p.z,
                s.v.len(),
                s.mode
            )
            .unwrap();
        }
        t += dt;
    }
}

pub fn write_facts(sim: &Sim, path: &Path) {
    let mut f = File::create(path).expect("facts file");
    writeln!(f, "seq,t,group,kind,ball,detail").unwrap();
    for fact in &sim.facts {
        let (kind, ball, detail) = match fact.kind {
            FactKind::BallBall { a, b } => ("ball_ball", a, format!("{b}")),
            FactKind::RailContact {
                ball,
                rail,
                frozen_no_count,
            } => (
                "rail_contact",
                ball,
                format!(
                    "{}{}",
                    crate::facts::rail_name(rail),
                    if frozen_no_count { ",frozen_no_count" } else { "" }
                ),
            ),
            FactKind::JawContact { ball, pocket } => ("jaw_contact", ball, format!("p{pocket}")),
            FactKind::Pocketed { ball, pocket } => ("pocketed", ball, format!("p{pocket}")),
            FactKind::OffTable { ball } => ("off_table", ball, String::new()),
            FactKind::Kick { ball, cause } => ("kick", ball, format!("{cause:?}")),
            FactKind::Rest { ball } => ("rest", ball, String::new()),
            FactKind::SlideToRoll { ball } => ("slide_to_roll", ball, String::new()),
            FactKind::RollToStop { ball } => ("roll_to_stop", ball, String::new()),
            FactKind::SpinDown { ball } => ("spin_down", ball, String::new()),
            FactKind::Freeze { ball } => ("freeze", ball, String::new()),
            FactKind::Depenetration { a, b } => ("depenetration", a, format!("{b}")),
        };
        writeln!(
            f,
            "{},{:.6},{},{},{},{}",
            fact.seq, fact.t, fact.group, kind, ball, detail
        )
        .unwrap();
    }
}

/// Run a preset, dump the event log and trajectory, print a one-line summary.
pub fn run_preset(name: &str, out: &Path) {
    let setup = preset(name, Profile::spec_default());
    let (sim, sum) = run_and_summarize(&setup, true);
    let facts = derive(&sim);
    write_facts(&sim, &out.join(format!("{name}.facts.csv")));
    write_trajectory(&sim, sum.t_rest, &out.join(format!("{name}.traj.csv")), 0.005);
    println!(
        "shot {name}: t_rest={:.3}s events={} groups={} ke_loss={:.1}% max_pen={:.4}mm min_gap={:.4}mm hop={:.1}mm rails={} distinct={} pocketed={:?} off={:?} cue_pocketed={} runaway={:?}",
        sum.t_rest,
        sum.events,
        sum.groups,
        100.0 * (sum.ke0 - sum.ke_rest) / sum.ke0,
        sum.max_pen_mm,
        sum.min_gap_mm,
        sum.max_hop_mm,
        facts.rail_contacts_total,
        facts.distinct_object_balls_to_rails,
        facts.pocketed,
        facts.off_table,
        facts.cue_ball_pocketed,
        sum.runaway
    );
}

/// The break acceptance hook: energy behaviour, no tunnelling, the >= 4 balls
/// to rails predicate, and reproducibility.
pub fn break_report(out: &Path) {
    let profile = Profile::spec_default();
    let setup = preset("break", profile);
    let (sim, sum) = run_and_summarize(&setup, true);
    let facts = derive(&sim);
    // determinism: same declaration twice, bit-identical final state
    let (sim2, sum2) = run_and_summarize(&setup, false);
    let mut bitwise = sum.t_rest.to_bits() == sum2.t_rest.to_bits();
    for (a, b) in sim.balls.iter().zip(sim2.balls.iter()) {
        if a.p.x.to_bits() != b.p.x.to_bits()
            || a.p.y.to_bits() != b.p.y.to_bits()
            || a.v.x.to_bits() != b.v.x.to_bits()
            || a.pocketed != b.pocketed
        {
            bitwise = false;
        }
    }
    let four_to_rails = facts.distinct_object_balls_to_rails >= 4;
    println!("--- break acceptance hook (#7 §6)");
    println!("arrangement        : rack-fixtures seed 1");
    println!(
        "rest               : {:.3} s, {} events, {} groups",
        sum.t_rest, sum.events, sum.groups
    );
    println!(
        "energy             : KE0={:.2} J KE_rest={:.2} J loss={:.1}%  max jump between segments={:.3}%",
        sum.ke0 / 1e9,
        sum.ke_rest / 1e9,
        100.0 * (sum.ke0 - sum.ke_rest) / sum.ke0,
        100.0 * sum.ke_jump_max
    );
    println!(
        "penetration        : max {:.5} mm (0 = no tunnelling); min ball gap {:.5} mm (>= 0 = no overlap)",
        sum.max_pen_mm, sum.min_gap_mm
    );
    println!("max hop above cloth: {:.2} mm", sum.max_hop_mm);
    println!(
        "rails              : {} contacts over {} distinct object balls; per-ball {:?}",
        facts.rail_contacts_total, facts.distinct_object_balls_to_rails, facts.per_ball_rails
    );
    println!(
        "pocketed {:?} off_table {:?} cue_pocketed {} -> >=4-to-rails predicate: {}",
        facts.pocketed, facts.off_table, facts.cue_ball_pocketed, four_to_rails
    );
    println!("bit-identical rerun : {bitwise}");
    println!("runaway            : {:?}", sum.runaway);
    let mut f = File::create(out.join("break-acceptance.txt")).expect("file");
    writeln!(
        f,
        "t_rest_s={:.4}\nevents={}\ngroups={}\nke0_J={:.4}\nke_rest_J={:.4}\nke_loss_pct={:.3}\nmax_penetration_mm={:.6}\nmin_pair_gap_mm={:.6}\nmax_hop_mm={:.3}\nrail_contacts_total={}\ndistinct_object_balls_to_rails={}\npocketed={:?}\noff_table={:?}\ncue_ball_pocketed={}\nbit_identical_rerun={}\nfour_to_rails_predicate={}\nrunaway={:?}\n",
        sum.t_rest,
        sum.events,
        sum.groups,
        sum.ke0,
        sum.ke_rest,
        100.0 * (sum.ke0 - sum.ke_rest) / sum.ke0,
        sum.max_pen_mm,
        sum.min_gap_mm,
        sum.max_hop_mm,
        facts.rail_contacts_total,
        facts.distinct_object_balls_to_rails,
        facts.pocketed,
        facts.off_table,
        facts.cue_ball_pocketed,
        bitwise,
        four_to_rails,
        sum.runaway
    )
    .unwrap();
    write_facts(&sim, &out.join("break.facts.csv"));
    write_trajectory(&sim, sum.t_rest, &out.join("break.traj.csv"), 0.005);
}

/// Pin the two numbers #7 §6 defers: sleep thresholds and rest tolerance.
pub fn pin_numbers(out: &Path) {
    println!("--- sleep threshold pinning");
    let mut lines = String::new();
    let mut reference: Option<Vec<crate::sim::Ball>> = None;
    for (v_th, w_th) in [
        (0.01, 0.0001),
        (0.1, 0.001),
        (1.0, 0.01),
        (10.0, 0.1),
        (50.0, 0.5),
    ] {
        let setup = preset("break", Profile::spec_default());
        let pivot = setup.profile.pivot_mm;
        let mut sim = Sim::new(setup.profile.clone(), setup.table.clone(), &setup.positions);
        sim.annotate_frozen();
        let res = setup.strike.resolve(pivot);
        sim.strike(res.v, res.w);
        sim.sleep_v = v_th;
        sim.sleep_w = w_th;
        let o = sim.run_to_rest();
        // deviation of every ball's rest position from the strictest run
        let mut max_dev = 0.0f64;
        if let Some(reference) = reference.as_ref() {
            for (a, b) in reference.iter().zip(sim.balls.iter()) {
                if a.pocketed != b.pocketed || a.off_table != b.off_table {
                    max_dev = f64::INFINITY;
                    continue;
                }
                let d = (b.p - a.p).len();
                if d > max_dev {
                    max_dev = d;
                }
            }
        } else {
            reference = Some(sim.balls.iter().map(|b| b.clone()).collect());
        }
        let line = format!(
            "sleep_v={:>6} mm/s sleep_w={:>8} rad/s -> t_rest={:.4}s events={} groups={} max_pen={:.5}mm max_rest_deviation_vs_strictest={:.6}mm",
            v_th, w_th, o.t_rest, o.events, o.groups, o.max_penetration_mm, max_dev
        );
        println!("{line}");
        lines.push_str(&line);
        lines.push('\n');
    }
    std::fs::write(out.join("sleep-pinning.txt"), lines).unwrap();
}

pub fn seed1_arrangement() -> crate::shots::Arrangement {
    let mut arr: crate::shots::Arrangement = std::collections::BTreeMap::new();
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

/// Energy/speed trace of the break at selected times (debug).
pub fn ke_trace(times: &[f64]) {
    let setup = preset("break", Profile::spec_default());
    let profile = setup.profile.clone();
    let (sim, sum) = run_and_summarize(&setup, true);
    println!("t_rest={:.4} events={}", sum.t_rest, sum.events);
    for &t in times {
        let st = sim.state_at(t);
        let ke: f64 = st.iter().map(|s| kinetic_energy(&profile, s)).sum();
        let speeds: Vec<String> = st
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}:{:.0}", i, s.v.len()))
            .collect();
        println!("t={t:.3} KE={:.3} J  {}", ke / 1e9, speeds.join(" "));
    }
}

/// Run the preset rack with explicit aim/speed and dump facts + rest (debug).
pub fn probe(aim_off: f64, speed: f64, a_tip: f64, b_tip: f64, elev: f64) {
    let profile = Profile::spec_default();
    let arr = seed1_arrangement();
    let cue = v3(-800.0, 0.0, R);
    let aim0 = (0.0f64 - cue.y).atan2(FOOT_SPOT_X - cue.x).to_degrees();
    let mut sim = Sim::new(
        profile.clone(),
        Table::new(),
        &crate::shots::break_setup(profile.clone(), &arr, cue, aim0 + aim_off, speed, a_tip, b_tip, elev).positions,
    );
    sim.annotate_frozen();
    let res = crate::strike::Strike::new(aim0 + aim_off, speed, a_tip, b_tip, elev)
        .resolve(profile.pivot_mm);
    sim.strike(res.v, res.w);
    let o = sim.run_to_rest();
    let f = derive(&sim);
    println!(
        "probe aim={:.2} speed={:.0} spin=({:.2},{:.2}) elev={:.1} -> t_rest={:.3} events={} distinct={} contacts={} pocketed={:?} off={:?} cue_pocketed={} runaway={:?}",
        aim0 + aim_off, speed, a_tip, b_tip, elev, o.t_rest, o.events,
        f.distinct_object_balls_to_rails, f.rail_contacts_total, f.pocketed, f.off_table,
        f.cue_ball_pocketed, o.runaway
    );
    for fact in sim.facts.iter() {
        if matches!(fact.kind, crate::sim::FactKind::Pocketed { .. }) {
            let st = sim.state_at(fact.t);
            match fact.kind {
                crate::sim::FactKind::Pocketed { ball, pocket } => {
                    println!(
                        "   pocketed ball {} in pocket {} at t={:.3} pos ({:.1},{:.1})",
                        ball, pocket, fact.t, st[ball as usize].p.x, st[ball as usize].p.y
                    );
                }
                _ => {}
            }
        }
    }
}

/// Final state dump for the break (debug).
pub fn dump_rest() {
    let setup = preset("break", Profile::spec_default());
    let (sim, sum) = run_and_summarize(&setup, false);
    println!("t_rest={:.4} events={} runaway={:?}", sum.t_rest, sum.events, sum.runaway);
    for b in sim.balls.iter() {
        println!(
            "ball {:>2}: p=({:>10.2},{:>10.2},{:>8.2}) v={:>8.2} w={:>8.2} mode={:?} pocketed={} off={}",
            b.id,
            b.p.x,
            b.p.y,
            b.p.z,
            b.v.len(),
            b.w.len(),
            b.mode,
            b.pocketed,
            b.off_table
        );
    }
}

/// Debug: a rolling ball into a cushion, printing the state around the hit.
pub fn dbg_cushion(v0: f64) {
    let profile = Profile::spec_default();
    let cue = v3(0.0, 0.0, R);
    let mut sim = Sim::new(profile.clone(), Table::new(), &[(0, cue)]);
    let res = crate::strike::Strike::new(0.0, v0, 0.0, 0.4, 0.0).resolve(profile.pivot_mm);
    sim.strike(res.v, res.w);
    sim.run_to_rest();
    let t_rest = sim.outcome.t_rest;
    let mut rails: Vec<f64> = sim
        .facts
        .iter()
        .filter(|f| matches!(f.kind, crate::sim::FactKind::RailContact { .. }))
        .map(|f| f.t)
        .collect();
    rails.push(t_rest);
    println!("cushion probe v0={v0:.0} mm/s t_rest={t_rest:.3}");
    let mut show: Vec<f64> = Vec::new();
    for t in &rails {
        show.push((*t - 0.0005).max(0.0));
        show.push(*t);
        show.push(*t + 0.0005);
        show.push(*t + 0.05);
        show.push(*t + 0.2);
    }
    for t in show {
        if t > t_rest {
            continue;
        }
        let st = &sim.state_at(t)[0];
        println!(
            "  t={t:7.4} p=({:8.1},{:6.1},{:6.2}) v=({:8.1},{:6.1},{:7.1}) w=({:7.1},{:7.1},{:6.1}) {:?}",
            st.p.x, st.p.y, st.p.z, st.v.x, st.v.y, st.v.z, st.w.x, st.w.y, st.w.z, st.mode
        );
    }
    println!("  rest x={:.1} mm", sim.balls[0].p.x);
}

/// Debug: a single ball with pure backspin, free space, no walls.
pub fn dbg_spin(spin_y: f64) {
    let profile = Profile::spec_default();
    let mut sim = Sim::new(profile.clone(), Table::new(), &[(0, v3(0.0, 0.0, R))]);
    sim.no_walls = true;
    sim.record_timeline = true;
    sim.strike(v3(0.0, 0.0, 0.0), v3(0.0, spin_y, 0.0));
    // a zero-velocity strike is legal here: the ball is launched by its spin
    let o = sim.run_to_rest();
    println!(
        "pure spin {} rad/s -> rest at {:.1} mm after {:.3} s, {} events (analytic: v_roll = R*w/... slide {:.0} mm + roll {:.0} mm)",
        spin_y,
        sim.balls[0].p.x,
        o.t_rest,
        o.events,
        0.5 * profile.a_slide() * (spin_y.abs() * R / (3.5 * profile.a_slide())).powi(2),
        (5.0 * spin_y.abs() * R / 7.0).powi(2) / (2.0 * profile.a_roll())
    );
}

/// Debug: a straight full hit with a given post-strike speed and spin.
pub fn dbg_draw(speed: f64, spin_y: f64, drag: f64) {
    let profile = Profile::spec_default();
    let cue = v3(-drag / 2.0, 0.0, R);
    let obj = v3(cue.x + drag, 0.0, R);
    let mut sim = Sim::new(profile.clone(), Table::new(), &[(0, cue), (1, obj)]);
    sim.no_walls = true;
    sim.record_timeline = true;
    sim.strike(v3(speed, 0.0, 0.0), v3(0.0, spin_y, 0.0));
    let o = sim.run_to_rest();
    let mut contact = None;
    for f in &sim.facts {
        if let crate::sim::FactKind::BallBall { .. } = f.kind {
            contact = Some(f.t);
            break;
        }
    }
    let t = contact.expect("no ball-ball contact");
    let a = sim.state_at((t - 1e-5).max(0.0));
    let b = sim.state_at(t + 1e-5);
    println!(
        "draw: pre-contact CB v={:.0} w={:.2} ; post CB v={:.0} w_y={:.2} OB v={:.0}",
        a[0].v.len(),
        a[0].w.y,
        b[0].v.x,
        b[0].w.y,
        b[1].v.len()
    );
    println!(
        "      contact x={:.1} rest x={:.1} -> draw {:.1} mm, t_rest {:.3}, events {}", 
        a[0].p.x,
        sim.balls[0].p.x,
        a[0].p.x - sim.balls[0].p.x,
        o.t_rest,
        o.events
    );
}

/// Debug walk: print the state at every event group for the break.
pub fn debug_break(groups: u64) {
    let setup = preset("break", Profile::spec_default());
    let profile = setup.profile.clone();
    let mut sim = Sim::new(profile.clone(), setup.table.clone(), &setup.positions);
    sim.annotate_frozen();
    let res = setup.strike.resolve(profile.pivot_mm);
    sim.strike(res.v, res.w);
    println!("initial candidates:");
    let mut cs = sim.candidates_debug();
    cs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (t, k) in cs.iter().take(12) {
        println!("   t={t:.6} {k}");
    }
    let mut last_events = 0u64;
    for _ in 0..20000 {
        if sim.outcome.groups >= groups {
            break;
        }
        let before = sim.outcome.groups;
        sim.step_once();
        if sim.outcome.groups != before {
            let mut mingap = f64::INFINITY;
            let mut who = (0usize, 0usize);
            for i in 0..sim.balls.len() {
                for j in (i + 1)..sim.balls.len() {
                    if !sim.balls[i].active() || !sim.balls[j].active() {
                        continue;
                    }
                    let g = (sim.balls[j].p - sim.balls[i].p).len() - 2.0 * R;
                    if g < mingap {
                        mingap = g;
                        who = (i, j);
                    }
                }
            }
            if sim.outcome.events != last_events {
                println!(
                    "group {} t={:.6} events={} min_gap={:.3}mm ({} {}) cue=({:.1},{:.1},{:.1}) v={:.0} b1=({:.1},{:.1}) v1={:.0}",
                    sim.outcome.groups,
                    sim.t,
                    sim.outcome.events,
                    mingap,
                    who.0,
                    who.1,
                    sim.balls[0].p.x,
                    sim.balls[0].p.y,
                    sim.balls[0].p.z,
                    sim.balls[0].v.len(),
                    sim.balls[1].p.x,
                    sim.balls[1].p.y,
                    sim.balls[1].v.len(),
                );
                for f in sim.facts.iter().rev().take(2) {
                    println!("    fact t={:.6} {:?}", f.t, f.kind);
                }
                last_events = sim.outcome.events;
            }
            if !sim.outcome.runaway.is_none() {
                break;
            }
        }
        if sim.outcome.runaway.is_some() {
            println!("runaway: {:?}", sim.outcome.runaway);
            break;
        }
    }
}

/// Rest-position sensitivity: how far the table moves when the declaration
/// moves by a plausible input error. This is what pins #7 §6's rest-position
/// tolerance -- the tolerance is achievable only if the input precision
/// supports it.
pub fn pin_tolerance(out: &Path) {
    println!("--- rest-position tolerance pinning (perturbation -> rest deviation)");
    let cases: &[(&str, f64, f64, f64)] = &[
        ("break", 0.0, 6200.0, 0.0),
        ("draw", 0.0, 3000.0, 0.0),
        ("follow", 0.0, 3000.0, 0.0),
        ("cut", 0.0, 1500.0, 0.0),
    ];
    let perms: &[(&str, f64, f64, f64, f64)] = &[
        // label, d_aim_deg, d_speed_mm_s, d_a_tip, d_b_tip
        ("aim +0.05 deg", 0.05, 0.0, 0.0, 0.0),
        ("aim +0.20 deg", 0.20, 0.0, 0.0, 0.0),
        ("speed +0.5%", 0.0, 0.005, 0.0, 0.0),
        ("speed +2%", 0.0, 0.02, 0.0, 0.0),
        ("spin a +0.02 R", 0.0, 0.0, 0.02, 0.0),
        ("spin b +0.02 R", 0.0, 0.0, 0.0, 0.02),
    ];
    let mut lines: Vec<String> = Vec::new();
    for (name, _, speed, _) in cases {
        let base = preset(name, Profile::spec_default());
        let (sim0, sum0) = run_and_summarize(&base, false);
        for (label, da, ds, dsa, dsb) in perms {
            let mut p2 = base.strike;
            let aim0 = p2.aim.y.atan2(p2.aim.x).to_degrees() + *da;
            p2 = crate::strike::Strike::new(aim0, speed * (1.0 + ds), *dsa, *dsb, 0.0);
            let mut setup2 = preset(name, Profile::spec_default());
            setup2.strike = p2;
            let (sim1, _) = run_and_summarize(&setup2, false);
            let mut devs: Vec<f64> = Vec::new();
            for (a, b) in sim0.balls.iter().zip(sim1.balls.iter()) {
                if a.pocketed != b.pocketed || a.off_table != b.off_table {
                    continue;
                }
                devs.push((b.p - a.p).len());
            }
            devs.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let med = if devs.is_empty() { f64::NAN } else { devs[devs.len() / 2] };
            let p90 = if devs.is_empty() {
                f64::NAN
            } else {
                devs[(((devs.len() as f64) * 0.9) as usize).min(devs.len() - 1)]
            };
            let max = devs.last().copied().unwrap_or(f64::NAN);
            let line = format!(
                "{name:>7} {label:>15}: median {med:7.2} mm, p90 {p90:7.2} mm, max {max:8.2} mm (t_rest {:.2} -> {:.2})",
                sum0.t_rest, sim1.outcome.t_rest
            );
            println!("{line}");
            lines.push(line);
        }
    }
    // precision ladder on the two-ball cut: what input precision does a
    // 25 mm median rest tolerance require?
    for &da in &[0.0005f64, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05] {
        let base = preset("cut", Profile::spec_default());
        let (sim0, _) = run_and_summarize(&base, false);
        let mut setup2 = preset("cut", Profile::spec_default());
        let aim0 = setup2.strike.aim.y.atan2(setup2.strike.aim.x).to_degrees() + da;
        setup2.strike = crate::strike::Strike::new(aim0, setup2.strike.speed, 0.0, 0.25, 0.0);
        let (sim1, _) = run_and_summarize(&setup2, false);
        let mut devs: Vec<f64> = Vec::new();
        for (a, b) in sim0.balls.iter().zip(sim1.balls.iter()) {
            if a.pocketed == b.pocketed {
                devs.push((b.p - a.p).len());
            }
        }
        devs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let med = devs[devs.len() / 2];
        let line = format!("cut aim +{da:.4} deg -> median rest deviation {med:7.3} mm");
        println!("{line}");
        lines.push(line);
    }
    for &ds in &[1e-5f64, 1e-4, 5e-4, 1e-3, 5e-3, 1e-2] {
        let base = preset("cut", Profile::spec_default());
        let (sim0, _) = run_and_summarize(&base, false);
        let mut setup2 = preset("cut", Profile::spec_default());
        let aim0 = setup2.strike.aim.y.atan2(setup2.strike.aim.x).to_degrees();
        setup2.strike = crate::strike::Strike::new(aim0, setup2.strike.speed * (1.0 + ds), 0.0, 0.25, 0.0);
        let (sim1, _) = run_and_summarize(&setup2, false);
        let mut devs: Vec<f64> = Vec::new();
        for (a, b) in sim0.balls.iter().zip(sim1.balls.iter()) {
            if a.pocketed == b.pocketed {
                devs.push((b.p - a.p).len());
            }
        }
        devs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let med = devs[devs.len() / 2];
        let line = format!("cut speed +{:.4}% -> median rest deviation {med:7.3} mm", ds * 100.0);
        println!("{line}");
        lines.push(line);
    }
    std::fs::write(out.join("tolerance-pinning.txt"), lines.join("\n")).unwrap();
}

pub fn full(out: &Path) {
    break_report(out);
    pin_numbers(out);
}

/// Render the PNG frames for one preset (or all).
pub fn render_all(name: &str, out: &Path) {
    let presets: Vec<&str> = if name == "all" {
        vec!["break", "draw", "follow", "cut", "bank"]
    } else {
        vec![]
    };
    let list: Vec<String> = if presets.is_empty() {
        vec![name.to_string()]
    } else {
        presets.iter().map(|s| s.to_string()).collect()
    };
    let shots_dir = out.join("shots");
    std::fs::create_dir_all(&shots_dir).unwrap();
    for p in list {
        render_preset(&p, &shots_dir);
    }
}

pub fn render_preset(name: &str, dir: &Path) {
    let setup = preset(name, Profile::spec_default());
    let (sim, sum) = run_and_summarize(&setup, true);
    let facts = derive(&sim);
    let n_frames = 8usize;
    let (w, h) = (640usize, 400usize);
    let view = View::new(w, h, 34.0, 18.0);
    // sample states
    // frames are coarse, but the trajectory overlay is sampled finely so the
    // drawn path is the real polyline, not a 8-segment approximation
    let frames: Vec<f64> = (0..=n_frames)
        .map(|i| sum.t_rest * i as f64 / n_frames as f64)
        .collect();
    let mut samples: Vec<(f64, Vec<BallState>)> = Vec::new();
    let dt_path = (sum.t_rest / 240.0).max(0.001);
    let mut t = 0.0;
    while t <= sum.t_rest + 1e-9 {
        samples.push((t, sim.state_at(t)));
        t += dt_path;
    }
    let samples: Vec<(f64, Vec<BallState>)> = samples;
    for (fi, &ft) in frames.iter().enumerate() {
        let states = sim.state_at(ft);
        let t = &ft;
        let mut c = Canvas::new(w, h, Rgb(0));
        draw_table(&mut c, &sim.table, &view);
        // trails: mid-shot frames show only the cue ball's path (all 16
        // worldlines overlaid reads as clutter); the last frame shows every
        // ball's path, which is the trajectory artefact.
        let last = fi == frames.len() - 1;
        for bi in 0..states.len() {
            if !last && bi != 0 {
                continue;
            }
            let mut pts: Vec<V3> = Vec::new();
            for (ts, st) in samples.iter() {
                if *ts > *t {
                    break;
                }
                if st[bi].pocketed || st[bi].off_table {
                    break;
                }
                pts.push(st[bi].p);
            }
            if pts.len() > 1 {
                let col = if bi == 0 { CUE_PATH } else { PATH };
                draw_path(&mut c, &view, &pts, col, 1.0);
            }
        }
        // final-frame highlight of balls that met a rail
        let rails_balls: Vec<u8> = facts.per_ball_rails.iter().map(|(b, _)| *b).collect();
        for (bi, st) in states.iter().enumerate() {
            if st.pocketed || st.off_table {
                // mark where it left the table
                if fi == frames.len() - 1 {
                    let (x, y) = view.px(st.p);
                    c.disc(x, y, 5.0, POCKETED);
                }
                continue;
            }
            let hl = if fi == frames.len() - 1 && rails_balls.contains(&(bi as u8)) {
                Some(HILITE)
            } else {
                None
            };
            draw_ball(&mut c, &view, bi as u8, st.p, hl);
        }
        // labels
        let title = format!(
            "{} FRAME {}/{} T={:.2}S",
            name.to_uppercase(),
            fi,
            frames.len() - 1,
            t
        );
        c.text(&title, 14, 12, 2, WHITE);
        let sub = format!(
            "CUE {:?}  RAILS {}  DISTINCT {}  POCKETED {}  OFF {}",
            if facts.cue_ball_pocketed { "POCKETED" } else { "ON TABLE" },
            facts.rail_contacts_total,
            facts.distinct_object_balls_to_rails,
            facts.pocketed.len(),
            facts.off_table.len()
        );
        c.text(&sub, 14, 30, 1, DIMMED);
        let fname = format!("{:02}-{}.png", fi, name);
        c.write_png(dir.join(&fname).to_str().unwrap()).unwrap();
    }
    // final summary frame with ball ids for the rest state
    println!(
        "rendered {name}: {n_frames} frames -> {}/  (rail contacts {}, distinct {})",
        dir.display(),
        facts.rail_contacts_total,
        facts.distinct_object_balls_to_rails
    );
}

pub fn total_energy_of(sim: &Sim, t: f64, profile: &Profile) -> f64 {
    sim.state_at(t)
        .iter()
        .map(|s| kinetic_energy(profile, s))
        .sum()
}

/// Unit check of the closed-form pair solver against a hand-computed contact.
pub fn selftest() {
    use crate::sim::solve_pair_pub;
    let dp = v3(-1435.0, 0.0, 0.0);
    let dv = v3(6200.0, 0.0, 0.0);
    let da = v3(-1961.33, 0.0, 0.0);
    println!("solve_pair -> {:?} (hand-computed contact t = 0.23068)", solve_pair_pub(dp, dv, da));
    let t = solve_pair_pub(dp, dv, da).unwrap();
    let p = dp + dv * t + da * (0.5 * t * t);
    println!("  |dp(t)| = {:.4} mm (want {:.4})", p.len(), 2.0 * R);
}
