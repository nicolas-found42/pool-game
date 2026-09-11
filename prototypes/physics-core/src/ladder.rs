//! The fitting ladder of #7 §6, stages 1-4, plus the differential cushion
//! check. Each stage isolates one parameter group and reports residuals
//! against the digitized curves in `data/curves/`.

use crate::consts::*;
use crate::json::{self, Json};
use crate::shots::{two_ball_setup, ShotSetup};
use crate::sim::{contact_point_velocity, Sim};
use crate::strike::Strike;
use crate::table::Table;
use crate::vec::{v3, V3};
use std::io::Write;
use std::path::Path;

fn load(curves: &str, name: &str) -> Json {
    let p = format!("{curves}/{name}");
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p}: {e}"));
    json::parse(&s).expect("json")
}

/// A generic straight full-hit draw/follow shot: cue ball travelling +x with
/// the given post-strike speed and spin, object ball `drag` mm ahead.
fn draw_shot(profile: &Profile, speed: f64, spin_y: f64, spin_z: f64, drag: f64) -> (f64, f64) {
    let cue = v3(-drag / 2.0, 0.0, R);
    let obj = v3(cue.x + drag, 0.0, R);
    let setup = two_ball_setup(profile.clone(), cue, obj, 1, 0.0, speed, 0.0, 0.0, 0.0);
    let mut sim = Sim::new(profile.clone(), Table::new(), &setup.positions);
    sim.record_timeline = true;
    sim.no_walls = true; // TP B-8's relation is free space
    // launch directly with the pre-impact state from the curve file
    sim.strike(v3(speed, 0.0, 0.0), v3(0.0, spin_y, spin_z));
    // remember where the cue ball meets the object ball
    let mut contact_x = f64::NAN;
    let mut last_cue_x = cue.x;
    sim.run_to_rest();
    for f in &sim.facts {
        if let crate::sim::FactKind::BallBall { a, .. } = f.kind {
            if a == 0 || a == 1 {
                let st = sim.state_at(f.t);
                contact_x = st[0].p.x;
                break;
            }
        }
    }
    let st = sim.state_at(sim.outcome.t_rest);
    if contact_x.is_nan() {
        contact_x = last_cue_x;
    }
    last_cue_x = st[0].p.x;
    // draw distance: how far the cue ball came back from the contact point
    (contact_x - last_cue_x, st[0].p.x - cue.x)
}

/// A ball released with pure backspin and no translation: exact comparison
/// against the file's closed form.
fn pure_spin_draw(profile: &Profile, spin_y: f64) -> (f64, f64) {
    let profile = profile.clone();
    let mut sim = Sim::new(profile.clone(), Table::new(), &[(0, v3(0.0, 0.0, R))]);
    sim.no_walls = true;
    sim.strike(v3(0.0, 0.0, 0.0), v3(0.0, spin_y, 0.0));
    sim.run_to_rest();
    (sim.balls[0].p.x.abs(), sim.outcome.t_rest)
}

/// A rolling cue ball into a stationary object ball, head-on; returns
/// (cue-ball travel, object-ball travel) in mm.
fn rolling_full_hit(profile: &Profile, speed: f64) -> (f64, f64) {
    let cue = v3(-600.0, 0.0, R);
    let obj = v3(0.0, 0.0, R);
    let setup = two_ball_setup(profile.clone(), cue, obj, 1, 0.0, speed, 0.0, 0.4, 0.0);
    let mut sim = Sim::new(profile.clone(), Table::new(), &setup.positions);
    sim.record_timeline = true;
    sim.no_walls = true; // TP B-5's table is free space
    let res = setup.strike.resolve(profile.pivot_mm);
    sim.strike(res.v, res.w);
    sim.run_to_rest();
    // TP B-5's ratio compares the object ball's travel with the cue ball's
    // travel *after* the impact.
    let mut t_contact = None;
    for f in &sim.facts {
        if let crate::sim::FactKind::BallBall { .. } = f.kind {
            t_contact = Some(f.t);
            break;
        }
    }
    let cx = match t_contact {
        Some(t) => sim.state_at(t)[0].p.x,
        None => cue.x,
    };
    ((sim.balls[0].p.x - cx).abs(), (sim.balls[1].p.x - obj.x).abs())
}

pub struct StageReport {
    pub name: String,
    pub lines: Vec<String>,
    pub rows: Vec<String>,
}

impl StageReport {
    fn new(name: &str) -> StageReport {
        StageReport {
            name: name.to_string(),
            lines: Vec::new(),
            rows: Vec::new(),
        }
    }
    fn push(&mut self, s: String) {
        println!("{s}");
        self.lines.push(s);
    }
}

/// Stage 1: draw / follow / tangent-line persistence against draw-follow.json.
pub fn stage1(curves: &str, profile_in: Profile) -> StageReport {
    let mut r = StageReport::new("stage1-draw-follow");
    // Compare against TP B-8 with the file's own parameters: e_b = 1.0 and
    // mu_b = 0.06 are the derivation's assumptions, not the spec profile's.
    let mut profile = profile_in;
    profile.e_b = 1.0;
    profile.mu_b = MuB::new(0.06, 0.0, 1.0);
    let d = load(curves, "draw-follow.json");
    let pts = d.arr("draw_distance_points");
    if pts.is_empty() {
        r.push("stage1: no points found".into());
        return r;
    }
    let mut n = 0usize;
    let mut skipped = 0usize;
    let mut sum_abs = 0.0f64;
    let mut sum_abs_frac = 0.0f64;
    let mut worst = (0.0f64, String::new());
    let mut worst_frac = (0.0f64, String::new());
    // the curve file's own spin convention: negative = backspin (draw)
    for p in pts.iter() {
        let speed = p.f("cb_speed_m_s") * 1000.0;
        let spin = p.f("cb_spin_rad_s");
        let drag = p.f("drag_distance_m") * 1000.0;
        let want = p.f("draw_distance_m") * 1000.0;
        // domain: TP B-8's relation is a backspin-only free-space closed form.
        // Where the pre-impact spin has gone positive (natural roll or better)
        // the file's squared relation still returns a positive "draw".
        let spin_at_impact = p.f("cb_spin_at_impact_rad_s");
        if spin_at_impact >= 0.0 || want > 2500.0 {
            skipped += 1;
            continue;
        }
        let (got, _) = draw_shot(&profile, speed, spin, 0.0, drag);
        let err = got - want;
        let frac = if want.abs() > 1.0 { err / want } else { 0.0 };
        n += 1;
        sum_abs += err.abs();
        if want.abs() > 1.0 {
            sum_abs_frac += frac.abs();
        }
        if err.abs() > worst.0 {
            worst = (err.abs(), format!("speed={speed:.0} spin={spin:.1} drag={drag:.0} want={want:.1} got={got:.1}"));
        }
        if frac.abs() > worst_frac.0 {
            worst_frac = (frac.abs(), format!("speed={speed:.0} b/R={:.2} want={want:.1} got={got:.1}", p.f("b_over_R")));
        }
        r.rows.push(format!(
            "{{\"cue_speed_m_s\":{},\"b_over_R\":{},\"drag_m\":{},\"want_mm\":{:.1},\"got_mm\":{:.1},\"err_mm\":{:.1}}}",
            p.f("cue_speed_m_s"),
            p.f("b_over_R"),
            p.f("drag_distance_m"),
            want,
            got,
            err
        ));
    }
    r.push(format!(
        "stage1 rows skipped as outside the relation's domain (non-negative pre-impact spin, or free-space draw beyond 2.5 m): {skipped} of {}",
        pts.len()
    ));
    // (a) pure-spin branch: the relation is exact for a ball launched with
    // backspin and no translation, so this isolates the cloth model.
    for w in [-50.0f64, -100.0, -150.0, -200.0] {
        let (got, _) = pure_spin_draw(&profile, w);
        let want = 0.3568 * w * w; // 2R^2/(49 g)(1/mu_s + 1/mu_r) with the file's units
        r.push(format!(
            "stage1 pure-spin branch w={w:.0} rad/s: relation {want:.1} mm, prototype {got:.1} mm, rel err {:.3}%",
            100.0 * (got - want).abs() / want
        ));
    }
    // (c) TP B-5's rolling direct-hit table: the source's own printed ratio
    // (6.08) disagrees with its own constants (7.559) -- which does the
    // prototype reproduce, and what mu_b would reproduce the printed value?
    let mut ratios = Vec::new();
    for (mph, cb, ob) in [
        (1.5f64, 0.1697f64, 1.2826f64),
        (3.0, 0.6787, 5.1304),
        (5.0, 1.8853, 14.2512),
    ] {
        let v = mph * 0.44704 * 1000.0;
        let (cb_t, ob_t) = rolling_full_hit(&profile, v);
        ratios.push(ob_t / cb_t);
        r.push(format!(
            "stage1 TP B-5 rolling direct hit {mph:.1} mph: file cb {cb:.4} m / ob {ob:.4} m (ratio {:.3}), prototype cb {:.4} m / ob {:.4} m (ratio {:.3})",
            ob / cb,
            cb_t / 1000.0,
            ob_t / 1000.0,
            ob_t / cb_t
        ));
    }
    let mean_ratio: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    r.push(format!(
        "stage1 TP B-5 ratio: prototype mean {mean_ratio:.3}; the file's re-derived constant gives 7.559, TP B-5's printed ratio is 6.08",
    ));
    r.push(format!(
        "stage1 draw/follow (TP B-8 parameters e_b=1.0, mu_b=0.06, free space): {} points, mean |err| = {:.1} mm, mean |err|/want = {:.2}% (where want>1mm)",
        n,
        sum_abs / n as f64,
        100.0 * sum_abs_frac / n as f64
    ));
    r.push(format!("stage1 worst absolute: {:.1} mm  [{}]", worst.0, worst.1));
    r.push(format!("stage1 worst relative: {:.1}%  [{}]", 100.0 * worst_frac.0, worst_frac.1));
    // sensitivity to the cloth pair (the file's own stated sensitivity)
    for (mus, mur) in [(0.20, 0.010), (0.15, 0.005), (0.40, 0.015)] {
        let mut p2 = profile.clone();
        p2.mu_s = mus;
        p2.mu_r = mur;
        let mut acc = 0.0f64;
        let mut cnt = 0.0f64;
        for p in pts.iter().step_by(10) {
            let speed = p.f("cb_speed_m_s") * 1000.0;
            let spin = p.f("cb_spin_rad_s");
            let drag = p.f("drag_distance_m") * 1000.0;
            let want = p.f("draw_distance_m") * 1000.0;
            if want.abs() < 50.0 {
                continue;
            }
            let (got, _) = draw_shot(&p2, speed, spin, 0.0, drag);
            acc += (got - want).abs() / want.abs();
            cnt += 1.0;
        }
        r.push(format!(
            "stage1 cloth sensitivity: mu_s={mus} mu_r={mur} -> mean relative err {:.1}%",
            100.0 * acc / cnt.max(1.0)
        ));
    }
    r
}

/// Stage 2: throw vs cut angle and speed against throw.json.
pub fn stage2(curves: &str, profile: Profile) -> StageReport {
    let mut r = StageReport::new("stage2-throw");
    let d = load(curves, "throw.json");
    let cols: Vec<String> = d
        .get("points")
        .and_then(|p| p.get("columns"))
        .and_then(|c| c.as_arr())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let ci = |name: &str| cols.iter().position(|c| c == name).unwrap_or(0);
    let (icut, ispeed, iroll, ieng, ithrow) = (
        ci("cut_deg"),
        ci("speed_m_s"),
        ci("roll_percent"),
        ci("english_percent"),
        ci("throw_deg"),
    );
    let rows = d.get("points").and_then(|p| p.get("data")).and_then(|v| v.as_arr()).cloned().unwrap_or_default();
    let mut n = 0usize;
    let mut err_sum = 0.0f64;
    let mut err_max = 0.0f64;
    let mut err_roll0_sum = 0.0f64;
    let mut n_roll0 = 0usize;
    let mut err_rollsum = 0.0f64;
    let mut n_roll = 0usize;
    let mut by_cut: Vec<(f64, f64, f64)> = Vec::new();
    let mut sample_rows: Vec<String> = Vec::new();
    for row in rows.iter() {
        let v = row.as_arr().unwrap();
        let cut = v[icut].as_f64().unwrap();
        let speed_mm = v[ispeed].as_f64().unwrap() * 1000.0;
        let spe = speed_mm / 1000.0;
        let speed = speed_mm;
        let roll = v[iroll].as_f64().unwrap();
        let eng = v[ieng].as_f64().unwrap();
        let want = v[ithrow].as_f64().unwrap();
        if cut < 5.0 {
            continue; // near head-on the throw model is the 1/7 cap regime
        }
        let Some(got) = crate::ladder::throw_sim(profile.clone(), cut, speed, roll, eng) else {
            continue;
        };
        // compare magnitudes: the file's throw column is unsigned
        let e = got.abs() - want;
        if sample_rows.len() < 6 {
            sample_rows.push(format!(
                "  cut={cut:.0} v={spe:.2} m/s roll={roll:.2} eng={eng:+.2} -> file {want:.3} deg, prototype {:.3} (signed)",
                got.abs()
            ));
        }
        n += 1;
        err_sum += e.abs();
        if e.abs() > err_max {
            err_max = e.abs();
        }
        if roll == 0.0 {
            err_roll0_sum += e.abs();
            n_roll0 += 1;
        } else {
            err_rollsum += e.abs();
            n_roll += 1;
        }
        by_cut.push((cut, want, got));
    }
    for l in &sample_rows {
        r.push(l.clone());
    }
    r.push(format!(
        "stage2 throw vs TP A-28 contours: {} points, mean |err| = {:.3} deg, max = {:.3} deg",
        n,
        err_sum / n.max(1) as f64,
        err_max
    ));
    r.push(format!(
        "stage2 split: roll=0 mean {:.3} deg ({} pts); roll>0 mean {:.3} deg ({} pts)",
        err_roll0_sum / n_roll0.max(1) as f64,
        n_roll0,
        err_rollsum / n_roll.max(1) as f64,
        n_roll
    ));
    // measured TP B-3 calibration points
    let meas = d.arr("measurements_TP_B3");
    let mut fit_rows: Vec<(f64, f64, f64)> = Vec::new(); // speed, cut, measured throw
    for m in meas.iter() {
        let speed = m.f("speed_m_s") * 1000.0;
        let cut = m.f("cut_deg");
        let want = m.f("throw_deg_from_in_per_yd");
        let model_a28 = m.f("model_deg_TP_A28_friction");
        let Some(got) = throw_sim(profile.clone(), cut, speed, 0.0, 0.0) else {
            continue;
        };
        r.push(format!(
            "stage2 measured: cut={cut:.0} speed={:.2} m/s measured={want:.3} deg, file A-28 model={model_a28:.3}, prototype={got:.3}",
            speed / 1000.0
        ));
        fit_rows.push((speed, cut, want));
    }
    // fit mu_b = a + b e^{-c v} to the six measured points (pattern search)
    let (best, err_before) = fit_mu_b(&fit_rows, profile.mu_b.a, profile.mu_b.b, profile.mu_b.c, profile.clone());
    r.push(format!(
        "stage2 mu_b fit on the 6 TP B-3 measured points: a={:.5} b={:.4} c={:.3} -> mean |err| {:.3} deg (published A-28: {:.5}/{:.3}/{:.3} -> {:.3} deg)",
        best.0, best.1, best.2, best.3, profile.mu_b.a, profile.mu_b.b, profile.mu_b.c, err_before
    ));
    let mut sc = profile.clone();
    sc.mu_b = MuB::new(best.0, best.1, best.2);
    let (_, err_after) = fit_mu_b(&fit_rows, sc.mu_b.a, sc.mu_b.b, sc.mu_b.c, sc);
    r.push(format!(
        "stage2 refit residual check: mean |err| with fitted mu_b = {:.3} deg (vs {:.3} with TP A-28's fit)",
        err_after, err_before
    ));
    // gearing + max throw sanity
    let mut max_throw = 0.0f64;
    for cut in [10.0, 20.0, 30.0, 40.0, 45.0, 60.0] {
        for eng in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            if let Some(t) = throw_sim(profile.clone(), cut, 500.0, 0.0, eng) {
                if t.abs() > max_throw {
                    max_throw = t.abs();
                }
            }
        }
    }
    r.push(format!(
        "stage2 prototype max |throw| over the grid = {:.3} deg (published family: max throw ~5 deg, zero at gearing)",
        max_throw
    ));
    r
}

/// Simulate one throw shot: cue ball along +x, object ball offset so the cut
/// angle is `cut_deg`, pre-impact speed and spins from the curve conventions.
pub fn throw_sim(profile: Profile, cut_deg: f64, speed: f64, roll_pct: f64, eng_pct: f64) -> Option<f64> {
    let cut = cut_deg.to_radians();
    // impact parameter for a cut angle phi: sin(phi) = b / (2R)
    let b = (2.0 * R) * cut.sin();
    let cue = v3(-300.0, 0.0, R);
    let obj = v3(0.0, b, R);
    let _ = obj;
    let setup = two_ball_setup(profile.clone(), cue, v3(0.0, b, R), 1, 0.0, speed, 0.0, 0.0, 0.0);
    let mut sim = Sim::new(profile, Table::new(), &setup.positions);
    // curve conventions: english -> w_z = (5/4)(v/R) * percent
    //                    roll    -> w_y = (5/4)(v/R) * percent (topspin positive)
    let wz = (1.25 * speed / R) * eng_pct;
    let wy = (1.25 * speed / R) * roll_pct;
    sim.strike(v3(speed, 0.0, 0.0), v3(0.0, wy, wz));
    sim.run_to_rest();
    // the first ball-ball contact, then the object ball's departure angle
    let mut t_contact = None;
    for f in &sim.facts {
        if let crate::sim::FactKind::BallBall { .. } = f.kind {
            t_contact = Some(f.t);
            break;
        }
    }
    let t = t_contact?;
    let a = sim.state_at(t);
    let bstate = sim.state_at(t + 0.0015);
    let v_obj = bstate[1].v;
    if v_obj.len() < 1.0 {
        return None;
    }
    // geometric line: from the contact positions, the OB should leave along
    // the line of centres; the throw is the deviation from that line
    let n = (bstate[1].p - a[0].p).norm();
    let v_hat = v_obj.norm();
    let dot = (n.dot(v_hat)).max(-1.0).min(1.0);
    let ang = dot.acos().to_degrees();
    // Published throw tables are all positive: the quantity is the magnitude
    // of the OB's deviation from the line of centres. The prototype's signed
    // deviation comes out negative here, i.e. the OB departs closer to the cue
    // ball's incoming line -- the standard cut-induced-throw direction.
    // Return the signed value; callers compare magnitudes.
    let cross = n.x * v_hat.y - n.y * v_hat.x;
    if cross > 0.0 {
        Some(-ang)
    } else {
        Some(ang)
    }
}

/// Fit mu_b's (a, b, c) to measured throw points by pattern search.
fn fit_mu_b(
    pts: &[(f64, f64, f64)],
    a0: f64,
    b0: f64,
    c0: f64,
    base: Profile,
) -> ((f64, f64, f64, f64), f64) {
    let eval = |a: f64, b: f64, c: f64| -> f64 {
        let mut p = base.clone();
        p.mu_b = MuB::new(a, b, c);
        let mut acc = 0.0f64;
        let mut n = 0.0f64;
        for (speed, cut, want) in pts {
            if let Some(got) = throw_sim(p.clone(), *cut, *speed, 0.0, 0.0) {
                acc += (got - want).abs();
                n += 1.0;
            }
        }
        acc / n.max(1.0)
    };
    let mut best = (a0, b0, c0, eval(a0, b0, c0));
    let mut step = (0.01, 0.1, 0.5);
    for _ in 0..40 {
        let mut improved = false;
        for k in 0..3 {
            for s in [-1.0f64, 1.0] {
                let mut cand = best;
                match k {
                    0 => cand.0 = (best.0 + s * step.0).max(0.001),
                    1 => cand.1 = (best.1 + s * step.1).max(0.01),
                    _ => cand.2 = (best.2 + s * step.2).max(0.01),
                }
                cand.3 = eval(cand.0, cand.1, cand.2);
                if cand.3 < best.3 {
                    best = cand;
                    improved = true;
                }
            }
        }
        if !improved {
            step = (step.0 * 0.5, step.1 * 0.5, step.2 * 0.5);
            if step.0 < 1e-5 {
                break;
            }
        }
    }
    (best, eval(a0, b0, c0))
}

/// Stage 3: cushion effective COR, the WPA acceptance stroke, and the bank
/// sanity check.
pub fn stage3(curves: &str, profile: Profile) -> StageReport {
    let mut r = StageReport::new("stage3-cushion");
    let d = load(curves, "cushion-bank.json");
    // (a) effective COR of a single square cushion hit, measured and predicted
    let pred = profile.cushion_effective_cor();
    let mut measured = Vec::new();
    for v in [800.0f64, 1500.0, 3000.0] {
        let cue = v3(-400.0, 0.0, R);
        let arr: Vec<(u8, V3)> = vec![(0, cue)];
        let setup = ShotSetup {
            profile: profile.clone(),
            table: Table::new(),
            positions: arr,
            strike: Strike::new(0.0, v, 0.0, 0.4, 0.0),
            frozen_note: None,
        };
        let mut sim = Sim::new(profile.clone(), Table::new(), &setup.positions);
        let res = setup.strike.resolve(profile.pivot_mm);
        sim.strike(res.v, res.w);
        sim.run_to_rest();
        // speed before and after the first cushion contact
        let mut pre = None;
        for f in &sim.facts {
            if let crate::sim::FactKind::RailContact { .. } = f.kind {
                let a = sim.state_at((f.t - 1e-4).max(0.0));
                let b = sim.state_at(f.t + 1e-4);
                pre = Some((a[0].v.xy().len(), b[0].v.xy().len()));
                break;
            }
        }
        if let Some((before, after)) = pre {
            measured.push(after / before);
            r.push(format!(
                "stage3 single square hit v={:.0} mm/s: horizontal speed {:.0} -> {:.0} mm/s, COR {:.4} (closed form {:.4}, e_n={:.2} nose normal |n_x|={:.5})",
                v, before, after, after / before, pred, profile.e_cushion, nose_normal_magnitude()
            ));
        }
    }
    // (b) WPA acceptance stroke: rolling from the head spot, travel in table lengths
    let mut found = None;
    for v in [1500.0f64, 2000.0, 2500.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0, 10000.0] {
        let (tl, ended) = rolling_travel(profile.clone(), v);
        r.push(format!(
            "stage3 WPA stroke v={:.1} m/s -> {:.3} table lengths{}",
            v / 1000.0,
            tl,
            if ended { " (ENDED OFF TABLE / POCKETED)" } else { "" }
        ));
        if found.is_none() && tl >= 4.0 {
            found = Some((v, tl));
        }
    }
    match found {
        Some((v, tl)) => r.push(format!(
            "stage3 WPA acceptance: the stroke must be about {:.1} m/s to run {:.2} table lengths (gate 4.0-4.5)",
            v / 1000.0, tl
        )),
        None => r.push("stage3 WPA acceptance: NOT reached within 10 m/s".into()),
    }
    // curve anchors: the TP B-6 travel at the printed speeds
    let mut anchor_err = 0.0f64;
    let mut anchor_n = 0usize;
    for a in d.arr("printed_anchors_TP_B6") {
        let mph = a.f("speed_mph");
        let v = mph * 0.44704 * 1000.0;
        let got = rolling_travel_table_lengths(profile.clone(), v);
        let want = a.f("printed_table_lengths");
        let e = got - want;
        anchor_err += e.abs();
        anchor_n += 1;
        r.push(format!(
            "stage3 TP B-6 anchor {mph:.1} mph: printed {want:.3} lengths, prototype {got:.3} (delta {:+.3})",
            e
        ));
    }
    r.push(format!(
        "stage3 anchor mean |err| = {:.3} table lengths over {} anchors (source says +/-2%)",
        anchor_err / anchor_n.max(1) as f64,
        anchor_n
    ));
    // (b2) identify the two knobs the WPA acceptance test and TP B-6 agree on
    let mut scan = String::new();
    for &(mur, en) in &[
        (0.010f64, 0.75f64), (0.010, 0.80), (0.010, 0.85), (0.010, 0.90),
        (0.008, 0.75), (0.008, 0.80), (0.008, 0.85),
        (0.006, 0.75), (0.006, 0.80),
        (0.005, 0.85),
    ] {
        let mut p2 = profile.clone();
        p2.mu_r = mur;
        p2.e_cushion = en;
        let tl4 = rolling_travel_table_lengths(p2.clone(), 4000.0);
        let tl7 = rolling_travel_table_lengths(p2, 7000.0);
        let line = format!(
            "stage3 WPA scan: mu_r={mur:.3} e_n={en:.2} -> 4 m/s stroke {tl4:.2} lengths, 7 m/s stroke {tl7:.2} lengths"
        );
        r.push(line.clone());
        scan.push_str(&line);
        scan.push('\n');
    }
    // (b3) which e_n reproduces the measured rail speed retention (e_c = 0.70)?
    for &en in &[0.75f64, 0.80, 0.83, 0.85, 0.90] {
        let mut p2 = profile.clone();
        p2.e_cushion = en;
        let ratio = measure_cushion_ratio(&p2, 1500.0);
        r.push(format!(
            "stage3 retention fit: e_n={en:.2} -> simulated speed retention {ratio:.4} (TP B-6 measured e_c = 0.70)"
        ));
    }
    std::fs::write(std::env::temp_dir().join("scan.txt"), scan).ok();
    // (c) measured bank data: geometry is under-determined by the file
    let banks = d.arr("measured_banks_TP_B28");
    r.push(format!(
        "stage3 bank grid: {} measured rows exist, but the file records aim/through diamonds only -- the ball's start position and speed are not pinned, so a like-for-like simulation is under-determined (spec gap for the ladder's stage-3 gate)",
        banks.len()
    ));
    r
}

/// Horizontal speed retention of a single square cushion hit.
fn measure_cushion_ratio(profile: &Profile, v: f64) -> f64 {
    let cue = v3(-400.0, 0.0, R);
    let setup = ShotSetup {
        profile: profile.clone(),
        table: Table::new(),
        positions: vec![(0, cue)],
        strike: Strike::new(0.0, v, 0.0, 0.4, 0.0),
        frozen_note: None,
    };
    let pivot = setup.profile.pivot_mm;
    let mut sim = Sim::new(profile.clone(), Table::new(), &setup.positions);
    sim.record_timeline = true;
    let res = setup.strike.resolve(pivot);
    sim.strike(res.v, res.w);
    sim.run_to_rest();
    for f in &sim.facts {
        if let crate::sim::FactKind::RailContact { .. } = f.kind {
            let a = sim.state_at((f.t - 1e-4).max(0.0));
            let b = sim.state_at(f.t + 1e-4);
            return b[0].v.xy().len() / a[0].v.xy().len();
        }
    }
    f64::NAN
}

/// Rolling ball from the head spot, square hits on the short cushions, total
/// path length in 9 ft table lengths.
fn rolling_travel_table_lengths(profile: Profile, speed: f64) -> f64 {
    rolling_travel(profile, speed).0
}

/// (table lengths travelled, ended pocketed/off-table)
fn rolling_travel(profile: Profile, speed: f64) -> (f64, bool) {
    let cue = v3(HEAD_STRING_X, 0.0, R);
    let pivot = profile.pivot_mm;
    let setup = ShotSetup {
        profile: profile.clone(),
        table: Table::new(),
        positions: vec![(0, cue)],
        strike: Strike::new(0.0, speed, 0.0, 0.4, 0.0),
        frozen_note: None,
    };
    let mut sim = Sim::new(profile, Table::new(), &setup.positions);
    let res = setup.strike.resolve(pivot);
    sim.strike(res.v, res.w);
    let start = cue.x;
    sim.run_to_rest();
    let t_rest = sim.outcome.t_rest;
    // integrate the path length by sampling the exact facade
    let mut len = 0.0;
    let mut prev = sim.state_at(0.0)[0].p;
    let steps = 4000;
    for i in 1..=steps {
        let t = t_rest * i as f64 / steps as f64;
        let st = &sim.state_at(t)[0];
        if st.off_table || st.pocketed {
            break;
        }
        len += (st.p - prev).len();
        prev = st.p;
    }
    let _ = start;
    let ended = sim.balls[0].pocketed || sim.balls[0].off_table;
    (len / TABLE_LEN, ended)
}

/// Stage 4: squirt and pivot length against squirt.json.
pub fn stage4(curves: &str, _profile: Profile) -> StageReport {
    let mut r = StageReport::new("stage4-squirt");
    let d = load(curves, "squirt.json");
    let table = d.arr("shaft_table");
    let mut implied: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut min_a = 1.0f64;
    let mut max_a = 0.0f64;
    for row in table.iter() {
        let angle = row.f("squirt_angle_deg").to_radians();
        let pivot_mm = row.f("pivot_point_in") * 25.4;
        // the pivot definition: tan(alpha) = a / L  ->  a = L tan(alpha)
        let a = pivot_mm * angle.tan();
        min_a = min_a.min(a);
        max_a = max_a.max(a);
        implied.push((row.s("shaft"), angle.to_degrees(), pivot_mm, a));
    }
    let mean_a: f64 = implied.iter().map(|(_, _, _, a)| *a).sum::<f64>() / implied.len() as f64;
    let sd = (implied
        .iter()
        .map(|(_, _, _, a)| (a - mean_a) * (a - mean_a))
        .sum::<f64>()
        / implied.len() as f64)
        .sqrt();
    r.push(format!(
        "stage4 Platinum table: {} shafts; if tan(alpha)=a/L holds, the implied reference tip offset is a = {:.2} mm ({:.2} R), sd {:.2} mm, range {:.2}-{:.2} mm",
        implied.len(),
        mean_a,
        mean_a / R,
        sd,
        min_a,
        max_a
    ));
    let ang_min = implied.iter().map(|(_, a, _, _)| *a).fold(f64::INFINITY, f64::min);
    let ang_max = implied.iter().map(|(_, a, _, _)| *a).fold(0.0f64, f64::max);
    r.push(format!(
        "stage4 squirt band: {:.2}-{:.2} deg (Shepard band 0.5-2.3, Platinum 1.3-2.3); pivot {:.1}-{:.1} in",
        ang_min, ang_max, 12.0 * 0.0 + 7.6, 14.1
    ));
    // prototype squirt angles for a mid pivot cue at that reference offset
    for pivot_in in [7.6f64, 10.0, 12.0, 14.1] {
        let pivot_mm = pivot_in * 25.4;
        for a_tip in [0.25, 0.5] {
            let s = Strike::new(0.0, 2000.0, a_tip, 0.0, 0.0);
            let res = s.resolve(pivot_mm);
            r.push(format!(
                "stage4 prototype: pivot {pivot_in:.1} in, tip offset {a_tip:.2} R -> squirt {:.3} deg, launch {:.1} deg off aim",
                res.squirt_rad.to_degrees(),
                res.v.y.atan2(res.v.x).to_degrees()
            ));
        }
    }
    r
}

pub fn run(stage: &str, curves: &str, out: &Path) {
    let profile = Profile::spec_default();
    let mut reports: Vec<StageReport> = Vec::new();
    if stage == "all" || stage == "1" {
        reports.push(stage1(curves, profile.clone()));
    }
    if stage == "all" || stage == "2" {
        reports.push(stage2(curves, profile.clone()));
    }
    if stage == "all" || stage == "3" {
        reports.push(stage3(curves, profile.clone()));
    }
    if stage == "all" || stage == "4" {
        reports.push(stage4(curves, profile.clone()));
    }
    for r in reports {
        let mut f = std::fs::File::create(out.join(format!("{}.txt", r.name))).unwrap();
        for l in &r.lines {
            writeln!(f, "{l}").unwrap();
        }
        let mut f = std::fs::File::create(out.join(format!("{}.csv", r.name))).unwrap();
        for l in &r.rows {
            writeln!(f, "{l}").unwrap();
        }
    }
    // the mu_b table error, an #11 §3 input
    println!(
        "mu_b piecewise table max error vs the exponential: {:.6}",
        profile.mu_b.table_error()
    );
    let _ = contact_point_velocity(V3::ZERO, V3::ZERO);
}
