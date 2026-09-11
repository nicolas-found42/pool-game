//! The 14 requirement rows of docs/spec/physics-break.json: drive the sim to
//! produce each row's target fact pattern, pinning the null parameters.
//!
//! "The parameters are the pin": aim, speed, spin and elevation are the free
//! parameters; the fact pattern is the requirement.

use crate::consts::*;
use crate::facts::{classify, ShotFacts, Tree};
use crate::json::Json;
use crate::shots::{aim_to, read_arrangement, run_shot, Arrangement, ShotSetup};
use crate::strike::Strike;
use crate::table::Table;
use crate::vec::{v3, V3};
use std::io::Write;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub aim_offset_deg: f64,
    pub aim_deg: f64,
    pub speed: f64,
    pub a_tip: f64,
    pub b_tip: f64,
    pub elevation_deg: f64,
}

impl Params {
    pub fn json(&self) -> String {
        format!(
            "{{\"aim_deg\":{:.3},\"aim_offset_deg\":{:.3},\"cue_speed_mm_s\":{:.1},\"spin_a_tip\":{:.3},\"spin_b_tip\":{:.3},\"elevation_deg\":{:.2}}}",
            self.aim_deg, self.aim_offset_deg, self.speed, self.a_tip, self.b_tip, self.elevation_deg
        )
    }
}

#[derive(Clone, Debug)]
pub struct Observed {
    pub distinct_object_balls_to_rails: usize,
    pub rail_contacts_total: usize,
    pub pocketed: Vec<u8>,
    pub off_table: Vec<u8>,
    pub legal: bool,
    pub cue_ball_pocketed: bool,
    pub tree: &'static str,
    pub rule: &'static str,
    pub cue_ball_contacts: usize,
    pub frozen_suppressed: usize,
    pub t_rest: f64,
    pub max_hop_mm: f64,
    pub off_table_rule_28_count: usize,
    pub frozen_at_start: bool,
    pub frozen_left_and_returned: bool,
}

impl Observed {
    pub fn json(&self) -> String {
        format!(
            "{{\"distinct_object_balls_to_rails\":{},\"rail_contacts_total\":{},\"pocketed\":[{}],\"off_table\":[{}],\"legal\":{},\"cue_ball_pocketed\":{},\"tree\":\"{}\",\"rule\":\"{}\",\"object_ball_contacts\":{},\"frozen_contacts_suppressed\":{},\"t_rest_s\":{:.3},\"max_hop_mm\":{:.2}}}",
            self.distinct_object_balls_to_rails,
            self.rail_contacts_total,
            self.pocketed.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","),
            self.off_table.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","),
            self.legal,
            self.cue_ball_pocketed,
            self.tree,
            self.rule,
            self.cue_ball_contacts,
            self.frozen_suppressed,
            self.t_rest,
            self.max_hop_mm
        )
    }
}

fn tree_name(t: Tree) -> &'static str {
    match t {
        Tree::LegalClean => "legal_clean",
        Tree::LegalPocketed => "legal_pocketed",
        Tree::IllegalBreak => "illegal_break",
        Tree::BreakFoul => "break_foul",
        Tree::EightOnBreak => "eight_on_break",
    }
}

pub fn observe(f: &ShotFacts, t_rest: f64, hop: f64) -> Observed {
    let frozen_left = f
        .frozen_left_and_returned
        .len()
        > 0;
    let cue_off = f.cue_ball_pocketed;
    let c = classify(f, cue_off);
    Observed {
        distinct_object_balls_to_rails: f.distinct_object_balls_to_rails,
        rail_contacts_total: f.rail_contacts_total,
        pocketed: f.pocketed.clone(),
        off_table: f.off_table.clone(),
        legal: c.legal_break,
        cue_ball_pocketed: cue_off,
        tree: tree_name(c.tree),
        rule: c.rule,
        cue_ball_contacts: f.cue_ball_contacts,
        frozen_suppressed: f.frozen_contacts_suppressed,
        t_rest,
        max_hop_mm: hop,
        off_table_rule_28_count: f.distinct_physical(),
        frozen_at_start: !f.frozen_at_start.is_empty(),
        frozen_left_and_returned: frozen_left,
    }
}

/// Compare an observation with a row's `expected` block.
pub fn matches(expected: &Json, obs: &Observed) -> (bool, usize) {
    let mut total = 0usize;
    let mut hits = 0usize;
    let mut check = |cond: bool| {
        total += 1;
        if cond {
            hits += 1;
        }
        cond
    };
    let mut ok = true;
    if expected.get("distinct_object_balls_to_rails").is_some() {
        ok &= check(
            obs.distinct_object_balls_to_rails as f64
                == expected.f("distinct_object_balls_to_rails"),
        );
    }
    if expected.get("rail_contacts_total").is_some() {
        ok &= check(obs.rail_contacts_total as f64 == expected.f("rail_contacts_total"));
    }
    if expected.get("pocketed").is_some() {
        let want: Vec<u8> = expected
            .arr("pocketed")
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as u8))
            .collect();
        ok &= check(want == obs.pocketed);
    }
    if expected.get("off_table").is_some() {
        let want: Vec<u8> = expected
            .arr("off_table")
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as u8))
            .collect();
        ok &= check(want == obs.off_table);
    }
    if expected.get("legal").is_some() {
        ok &= check(obs.legal == expected.b("legal"));
    }
    if expected.get("cue_ball_pocketed").is_some() {
        ok &= check(obs.cue_ball_pocketed == expected.b("cue_ball_pocketed"));
    }
    if expected.get("tree").is_some() {
        ok &= check(obs.tree == expected.s("tree"));
    }
    if expected.get("rule").is_some() {
        ok &= check(obs.rule == expected.s("rule"));
    }
    if expected.get("object_ball_contacts").is_some() {
        ok &= check(obs.cue_ball_contacts as f64 == expected.f("object_ball_contacts"));
    }
    if expected.get("frozen_to_rail_at_shot_start").is_some() {
        ok &= check(obs.frozen_at_start == expected.b("frozen_to_rail_at_shot_start"));
    }
    if expected.get("frozen_ball_left_and_returned").is_some() {
        ok &= check(
            obs.frozen_left_and_returned == expected.b("frozen_ball_left_and_returned"),
        );
    }
    (ok, 100 * hits / total.max(1))
}

pub struct RowReport {
    pub id: String,
    pub name: String,
    pub produced: bool,
    pub params: Option<Params>,
    pub observed: Observed,
    pub near_miss: Option<(Params, Observed, usize)>,
    pub samples: usize,
    pub note: String,
}

/// Build the pre-state for a row: rack + cue ball, plus the pb-13 frozen ball.
fn setup_for(row: &Json, p: Params) -> (ShotSetup, Option<String>) {
    let arr: Arrangement = read_arrangement(row.get("arrangement").unwrap());
    let cue = row.get("shot").unwrap().get("cue_ball").unwrap();
    let cue_pos = v3(cue.f("x_mm"), cue.f("y_mm"), R);
    let mut positions: Vec<(u8, V3)> = vec![(0, cue_pos)];
    let frozen_row = row
        .get("target_facts")
        .and_then(|t| t.get("expected"))
        .map(|e| e.get("frozen_to_rail_at_shot_start").is_some())
        .unwrap_or(false);
    let mut note = None;
    let mut rack = crate::shots::arrangement_positions(&arr);
    if frozen_row {
        // Pin: the ball in the back-row corner slot 5.0 is moved out of the
        // rack and placed frozen to the right long cushion. The row does not
        // pin which ball or which rail; this is the prototype's pin.
        let mut moved = None;
        for (id, slot) in arr.iter() {
            if slot == "5.0" {
                moved = Some(id.parse::<u8>().unwrap());
            }
        }
        if let Some(id) = moved {
            rack.retain(|(b, _)| *b != id);
            positions.push((id, v3(700.0, HALF_WID - R, R)));
            note = Some(format!(
                "frozen-ball pin: ball {id} moved from rack slot 5.0 to (700, {:.1}) frozen to the right long cushion",
                HALF_WID - R
            ));
        }
    }
    positions.extend(rack);
    let aim0 = aim_to(cue_pos, v3(FOOT_SPOT_X, 0.0, R));
    let aim = aim0 + p.aim_offset_deg;
    let setup = ShotSetup {
        profile: Profile::spec_default(),
        table: Table::new(),
        positions,
        strike: Strike::new(aim, p.speed, p.a_tip, p.b_tip, p.elevation_deg),
        frozen_note: note.clone(),
    };
    (setup, note)
}

fn evaluate(row: &Json, p: Params) -> (Observed, usize, bool) {
    let (setup, _) = setup_for(row, p);
    let expected = row
        .get("target_facts")
        .and_then(|t| t.get("expected"))
        .cloned()
        .unwrap_or(Json::Null);
    let res = run_shot(&setup, false);
    let obs = observe(&res.facts, res.outcome.t_rest, res.outcome.max_hop_mm);
    let (ok, score) = matches(&expected, &obs);
    (obs, score, ok)
}

/// Search for the parameters that produce a row's pattern.
///
/// Three passes: a coarse (aim, speed) grid; a refinement around the best
/// near-misses; then a spin/elevation pass. Deterministic and reproducible --
/// the grid is fixed, no RNG anywhere.
pub fn search_row(row: &Json, budget: usize) -> RowReport {
    let id = row.s("id");
    let name = row.s("name");
    let cue = row.get("shot").unwrap().get("cue_ball").unwrap();
    let cue_pos = v3(cue.f("x_mm"), cue.f("y_mm"), R);
    let aim0 = aim_to(cue_pos, v3(FOOT_SPOT_X, 0.0, R));
    let wants_off_table = row
        .get("target_facts")
        .and_then(|t| t.get("expected"))
        .map(|e| e.arr("off_table").iter().count() > 0)
        .unwrap_or(false);

    let mut samples = 0usize;
    let mut best: Vec<(Params, Observed, usize)> = Vec::new();
    let mut found: Option<(Params, Observed, String)> = None;

    let mut evaluate = |p: Params, best: &mut Vec<(Params, Observed, usize)>,
                        samples: &mut usize,
                        found: &mut Option<(Params, Observed, String)>, note: &str| {
        if found.is_some() || *samples >= budget {
            return;
        }
        let (obs, score, ok) = evaluate(row, p);
        *samples += 1;
        if ok {
            *found = Some((p, obs, note.to_string()));
            return;
        }
        best.push((p, obs, score));
        best.sort_by(|a, b| b.2.cmp(&a.2));
        best.truncate(4);
    };

    // pass 1: coarse aim x speed, no spin
    let speeds = [
        800.0, 1000.0, 1200.0, 1400.0, 1600.0, 1850.0, 2100.0, 2400.0, 2700.0, 3100.0, 3600.0,
        4200.0, 5000.0, 6000.0, 7000.0,
    ];
    // rows whose target needs a specific ball pocketed need a wider aim box:
    // the mouth is a narrow window, so the interesting aims are off-axis
    let wants_pocket = row
        .get("target_facts")
        .and_then(|t| t.get("expected"))
        .map(|e| !e.arr("pocketed").is_empty())
        .unwrap_or(false);
    let offsets: Vec<f64> = if wants_pocket {
        (-24..=24).map(|i| i as f64 * 0.5).collect()
    } else {
        vec![-3.0, -2.5, -2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0]
    };
    for off in offsets.iter().copied() {
        for sp in speeds {
            let p = Params {
                aim_offset_deg: off,
                aim_deg: aim0 + off,
                speed: sp,
                a_tip: 0.0,
                b_tip: 0.0,
                elevation_deg: 0.0,
            };
            evaluate(p, &mut best, &mut samples, &mut found, "coarse grid, no spin");
            if found.is_some() || samples >= budget {
                break;
            }
        }
        if found.is_some() || samples >= budget {
            break;
        }
    }

    // pass 2: refine aim and speed around each near-miss
    if found.is_none() {
        let seeds: Vec<Params> = best.iter().map(|(p, _, _)| *p).collect();
        'outer: for seed in seeds {
            for i in -4..=4 {
                for j in -5..=5 {
                    let p = Params {
                        aim_offset_deg: seed.aim_offset_deg + 0.1 * i as f64,
                        aim_deg: aim0 + seed.aim_offset_deg + 0.1 * i as f64,
                        speed: (seed.speed + 50.0 * j as f64).max(200.0),
                        a_tip: 0.0,
                        b_tip: 0.0,
                        elevation_deg: 0.0,
                    };
                    evaluate(p, &mut best, &mut samples, &mut found, "refined (aim, speed)");
                    if found.is_some() || samples >= budget {
                        break 'outer;
                    }
                }
            }
        }
    }

    // pass 3: spin and elevation
    if found.is_none() {
        let seeds: Vec<Params> = best.iter().map(|(p, _, _)| *p).collect();
        'outer3: for seed in seeds {
            for &a_tip in &[-0.25f64, -0.1, 0.0, 0.1, 0.25] {
                for &b_tip in &[-0.4f64, -0.2, 0.0, 0.2, 0.4] {
                    let elevs: &[f64] = if wants_off_table {
                        &[0.0, 8.0, 14.0]
                    } else {
                        &[0.0]
                    };
                    for &el in elevs {
                        let p = Params {
                            aim_offset_deg: seed.aim_offset_deg,
                            aim_deg: aim0 + seed.aim_offset_deg,
                            speed: seed.speed,
                            a_tip,
                            b_tip,
                            elevation_deg: el,
                        };
                        evaluate(p, &mut best, &mut samples, &mut found, "spin/elevation");
                        if found.is_some() || samples >= budget {
                            break 'outer3;
                        }
                    }
                }
            }
        }
    }

    match found {
        Some((p, obs, note)) => RowReport {
            id,
            name,
            produced: true,
            params: Some(p),
            observed: obs,
            near_miss: None,
            samples,
            note,
        },
        None => {
            let (np, no, _) = best
                .first()
                .cloned()
                .unwrap_or((Params { aim_offset_deg: 0.0, aim_deg: aim0, speed: 0.0, a_tip: 0.0, b_tip: 0.0, elevation_deg: 0.0 }, Observed {
                    distinct_object_balls_to_rails: 0,
                    rail_contacts_total: 0,
                    pocketed: vec![],
                    off_table: vec![],
                    legal: false,
                    cue_ball_pocketed: false,
                    tree: "none",
                    rule: "none",
                    cue_ball_contacts: 0,
                    frozen_suppressed: 0,
                    t_rest: 0.0,
                    max_hop_mm: 0.0,
                    off_table_rule_28_count: 0,
                    frozen_at_start: false,
                    frozen_left_and_returned: false,
                }, 0));
            RowReport {
                id,
                name,
                produced: false,
                params: None,
                observed: no.clone(),
                near_miss: Some((np, no, 0)),
                samples,
                note: "no parameter set in the searched box produced the pattern".into(),
            }
        }
    }
}

/// Drive every row of the corpus and write the status table.
pub fn run_all(spec_path: &str, out: &Path, budget: usize, only: &[String]) {
    let src = std::fs::read_to_string(spec_path).expect("physics-break.json");
    let doc = crate::json::parse(&src).expect("parse");
    let entries = doc.arr("entries");
    let mut reports: Vec<RowReport> = Vec::new();
    for e in entries.iter() {
        let id = e.s("id");
        if !only.is_empty() && !only.iter().any(|o| *o == id) {
            continue;
        }
        let t0 = std::time::Instant::now();
        let r = search_row(e, budget);
        println!(
            "row {}: {} ({} samples, {:.1}s){}",
            r.id,
            if r.produced { "PRODUCED" } else { "not produced" },
            r.samples,
            t0.elapsed().as_secs_f64(),
            if r.produced {
                format!(" params={}", r.params.unwrap().json())
            } else {
                String::new()
            }
        );
        if !r.produced {
            println!("      near miss: {} -> {}", r.near_miss.as_ref().unwrap().0.json(), r.observed.json());
        } else {
            println!("      observed: {}", r.observed.json());
        }
        reports.push(r);
    }
    let produced = reports.iter().filter(|r| r.produced).count();
    println!("rows produced: {produced}/{}", reports.len());
    let mut f = std::fs::File::create(out.join("rows.json")).unwrap();
    writeln!(f, "[").unwrap();
    for (i, r) in reports.iter().enumerate() {
        writeln!(
            f,
            "  {{\"id\":\"{}\",\"name\":\"{}\",\"produced\":{},\"samples\":{},\"params\":{},\"observed\":{},\"near_miss\":{},\"note\":\"{}\"}}{}",
            r.id,
            r.name.replace('"', "'"),
            r.produced,
            r.samples,
            r.params.map(|p| p.json()).unwrap_or_else(|| "null".into()),
            r.observed.json(),
            r.near_miss
                .as_ref()
                .map(|(p, o, _)| format!("{{\"params\":{},\"observed\":{}}}", p.json(), o.json()))
                .unwrap_or_else(|| "null".into()),
            r.note,
            if i + 1 == reports.len() { "" } else { "," }
        )
        .unwrap();
    }
    writeln!(f, "]").unwrap();
    let mut t = std::fs::File::create(out.join("rows.md")).unwrap();
    writeln!(t, "| row | produced | pinned parameters | observed (distinct/contacts/pocketed/off/legal/tree) |").unwrap();
    writeln!(t, "|---|---|---|---|").unwrap();
    for r in &reports {
        let p = r
            .params
            .map(|p| {
                format!(
                    "aim {:.2}deg, {:.0} mm/s, spin ({:.2},{:.2}) R, elev {:.1}deg",
                    p.aim_deg, p.speed, p.a_tip, p.b_tip, p.elevation_deg
                )
            })
            .unwrap_or_else(|| "not found".into());
        writeln!(
            t,
            "| {} {} | {} | {} | {}/{}/{:?}/{:?}/{}/{} |",
            r.id,
            r.name,
            if r.produced { "yes" } else { "no" },
            p,
            r.observed.distinct_object_balls_to_rails,
            r.observed.rail_contacts_total,
            r.observed.pocketed,
            r.observed.off_table,
            r.observed.legal,
            r.observed.tree
        )
        .unwrap();
    }
    let _ = std::path::Path::new(".");
}
