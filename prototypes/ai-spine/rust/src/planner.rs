//! Scripted planner: analytic makeability + mobility-style leave + micro refinement.
//!
//! This is the fallback policy and the imitation teacher in miniature (#9 §4): the best-scoring
//! declaration found so far under a per-decision work-unit cap, where one work unit is one sim
//! evaluation. Nothing here is a learned component.

use crate::gen::{generate, Candidate, GenCfg, Kind};
use crate::geom::{clearance, pockets, Ob, V2, BALL_D};
use crate::rng::SplitMix64;
use crate::toy_sim;

#[derive(Clone, Copy, Debug)]
pub struct PlannerCfg {
    pub gen: GenCfg,
    pub shortlist_k: usize,
    /// Work-unit cap: sim evaluations per decision (the spec's `B`).
    pub work_cap: usize,
    pub micro: bool,
    pub noise_aim_sigma: f64,
    pub noise_speed_sigma: f64,
    pub max_events: usize,
}

impl Default for PlannerCfg {
    fn default() -> Self {
        Self {
            gen: GenCfg::default(),
            shortlist_k: 6,
            work_cap: 24,
            micro: true,
            noise_aim_sigma: 0.0,
            noise_speed_sigma: 0.0,
            max_events: 4000,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub idx: usize,
    pub kind: Kind,
    pub ball: usize,
    pub pocket: usize,
    pub seed: f64,
    pub score: f64,
    pub pot: bool,
    pub scratch: bool,
    pub leave: f64,
    pub aim: V2,
    pub speed: f64,
    pub contact: V2,
    pub rails: u8,
    pub inter: u8,
    pub cut_cos: f64,
    pub clearance: f64,
    pub makeability: f64,
}

pub struct Decision {
    pub chosen: Option<Candidate>,
    pub score: f64,
    pub sim_evals: usize,
    pub verified_pot: bool,
    pub scratch: bool,
    pub gen_s: f64,
    pub shortlist_s: f64,
    pub verify_s: f64,
    pub micro_s: f64,
    pub total_s: f64,
    pub candidates: usize,
    pub rows: Vec<Row>,
    /// The refined strike, when the micro improved on the seed.
    pub refined: Option<Row>,
}

/// The legality mask. In the shipped system the rules layer supplies it (#9 §2); here every
/// generated candidate is legal by construction — the first contact is a live object ball and the
/// construction was cleared for line of sight. The mask exists so the shape is real.
pub fn mask(cands: &[Candidate], objs: &[Ob]) -> Vec<bool> {
    cands
        .iter()
        .map(|c| {
            objs.iter().any(|o| o.id == c.ball && o.alive)
                && c.clearance >= 0.0
                && c.cut_cos > 0.0
        })
        .collect()
}

/// Best direct-pot makeability available to `ball` from `cue` — the mobility primitive.
fn best_direct_makeability(cue: V2, ball: Ob, objs: &[Ob]) -> f64 {
    let ps = pockets();
    let mut best = 0.0f64;
    for pk in ps.iter() {
        let Some(u) = (pk.p - ball.p).unit() else {
            continue;
        };
        let contact = ball.p - u * BALL_D;
        let Some(aim) = (contact - cue).unit() else {
            continue;
        };
        let cut = aim.dot(u);
        if cut <= 0.02 {
            continue;
        }
        let c1 = clearance(cue, contact, objs, &[ball.id]);
        if c1 < 0.0 {
            continue;
        }
        let c2 = clearance(ball.p, pk.p, objs, &[ball.id]);
        if c2 < 0.0 {
            continue;
        }
        let m = crate::gen::makeability(
            cut,
            cue.dist(contact),
            ball.p.dist(pk.p),
            0,
            0,
            c1.min(c2),
        );
        if m > best {
            best = m;
        }
    }
    best
}

/// Mobility-style leave score in [0, 1]: how good the resulting position is for the next shot.
pub fn leave_score(cue_after: V2, objs_after: &[Ob]) -> f64 {
    let mut scores: Vec<f64> = objs_after
        .iter()
        .filter(|o| o.alive)
        .map(|o| best_direct_makeability(cue_after, *o, objs_after))
        .collect();
    if scores.is_empty() {
        return 1.0; // cleared the table
    }
    scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let n = scores.len().min(3);
    let mean = scores[..n].iter().sum::<f64>() / n as f64;
    let any = if scores[0] > 0.0 { 1.0 } else { 0.35 };
    (mean * any).clamp(0.0, 1.0)
}

fn remaining_objs(objs: &[Ob], potted: &[usize]) -> Vec<Ob> {
    objs.iter()
        .map(|o| Ob {
            alive: o.alive && !potted.contains(&o.id),
            ..*o
        })
        .collect()
}

/// Sim-evaluate one strike. Counts as one work unit.
fn eval_strike(
    cue: V2,
    objs: &[Ob],
    aim: V2,
    speed: f64,
    ball: usize,
    kind: Kind,
    cfg: &PlannerCfg,
) -> (bool, bool, bool, f64) {
    let target = if kind.is_safety() { None } else { Some(ball) };
    let out = toy_sim::strike(cue, objs, aim, speed, target, cfg.max_events);
    let cue_after = out.rest[0].p;
    let after = remaining_objs(objs, &out.potted_objs);
    let leave = if out.scratch {
        0.0
    } else {
        leave_score(cue_after, &after)
    };
    (
        out.target_potted,
        out.scratch,
        out.first_contact.is_some(),
        leave,
    )
}

fn score_candidate(c: &Candidate, pot: Option<bool>, contacted: bool, leave: f64) -> f64 {
    if c.kind.is_safety() {
        // A safety that never reached a ball is a foul, not a safe: contact is a hard term.
        if !contacted {
            return 0.02;
        }
        0.34 + 0.55 * leave
    } else {
        match pot {
            Some(true) => 1.0 + 0.5 * leave,
            Some(false) => 0.08 * leave,
            None => 0.45 * c.makeability + 0.20 * leave,
        }
    }
}

pub struct Refined {
    pub cand: Candidate,
    pub pot: bool,
    pub scratch: bool,
    pub leave: f64,
    pub score: f64,
    pub work: usize,
}

/// The analytic micro in miniature: the seed strike plus a small aim × speed perturbation sweep
/// against the sim, first improving hit winning under the work-unit cap.
pub fn refine_one(
    cue: V2,
    objs: &[Ob],
    seed: &Candidate,
    cfg: &PlannerCfg,
    work_cap: usize,
) -> Refined {
    const AIM_STEPS: [f64; 5] = [-0.010, -0.005, 0.0, 0.005, 0.010];
    const SPEED_STEPS: [f64; 3] = [0.90, 1.0, 1.10];
    let mut work = 0usize;
    let mut best: Option<Refined> = None;
    'outer: for sa in AIM_STEPS.iter() {
        for ss in SPEED_STEPS.iter() {
            if work >= work_cap {
                break 'outer;
            }
            let mut c = seed.clone();
            if *sa != 0.0 {
                c.aim = seed.aim.rotate(*sa);
            }
            if *ss != 1.0 {
                c.speed = seed.speed * ss;
            }
            let (pot, scratch, contacted, leave) =
                eval_strike(cue, objs, c.aim, c.speed, c.ball, c.kind, cfg);
            work += 1;
            let score = score_candidate(&c, Some(pot), contacted, leave);
            if best.as_ref().map(|b| score > b.score).unwrap_or(true) {
                best = Some(Refined {
                    cand: c,
                    pot,
                    scratch,
                    leave,
                    score,
                    work,
                });
            }
        }
    }
    let mut out = best.unwrap_or_else(|| Refined {
        cand: seed.clone(),
        pot: false,
        scratch: false,
        leave: 0.0,
        score: 0.0,
        work,
    });
    out.work = work;
    out
}

/// One full decision: generate → shortlist → verify → refine, under the work-unit cap.
pub fn decide(cue: V2, objs: &[Ob], cfg: &PlannerCfg) -> Decision {
    let t_start = std::time::Instant::now();
    let gen = generate(cue, objs, &cfg.gen);
    let gen_s = gen.elapsed_s;
    let cands = gen.candidates;

    let t_sl = std::time::Instant::now();
    let m = mask(&cands, objs);
    let mut order: Vec<usize> = (0..cands.len()).filter(|i| m[*i]).collect();
    order.sort_by(|a, b| {
        cands[*b]
            .seed_score
            .partial_cmp(&cands[*a].seed_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let shortlist_s = t_sl.elapsed().as_secs_f64();

    let mut rows: Vec<Row> = Vec::new();
    let mut work = 0usize;
    let mut best: Option<(f64, usize, bool, bool, f64)> = None; // (score, idx, pot, scratch, leave)
    let t_v = std::time::Instant::now();
    for &i in order.iter().take(cfg.shortlist_k) {
        if work >= cfg.work_cap {
            break;
        }
        let c = &cands[i];
        let (pot, scratch, contacted, leave) =
            eval_strike(cue, objs, c.aim, c.speed, c.ball, c.kind, cfg);
        work += 1;
        let score = score_candidate(c, Some(pot), contacted, leave);
        rows.push(row_of(i, c, score, pot, scratch, leave));
        if best.map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, i, pot, scratch, leave));
        }
    }
    let verify_s = t_v.elapsed().as_secs_f64();

    let t_m = std::time::Instant::now();
    let mut refined: Option<Row> = None;
    let mut chosen = best.and_then(|(_, i, _, _, _)| cands.get(i).cloned());
    let mut verified = best;
    if cfg.micro {
        if let Some((score0, bi, pot0, scratch0, leave0)) = best {
            let remaining = cfg.work_cap.saturating_sub(work);
            let r = refine_one(cue, objs, &cands[bi], cfg, remaining);
            work += r.work;
            refined = Some(row_of(bi, &r.cand, r.score, r.pot, r.scratch, r.leave));
            if r.score >= score0 {
                chosen = Some(r.cand.clone());
                verified = Some((r.score, bi, r.pot, r.scratch, r.leave));
            } else {
                verified = Some((score0, bi, pot0, scratch0, leave0));
            }
        }
    }
    let micro_s = t_m.elapsed().as_secs_f64();
    // Anytime commitment: with no work left (or a zero cap) the analytic seed still yields a
    // declaration — the best-scoring candidate found so far is played.
    if chosen.is_none() {
        if let Some(&i) = order.first() {
            chosen = cands.get(i).cloned();
        }
    }
    let (score, _idx, pot, scratch, _leave) = verified.unwrap_or((0.0, 0, false, false, 0.0));

    Decision {
        chosen,
        score,
        sim_evals: work,
        verified_pot: pot,
        scratch,
        gen_s,
        shortlist_s,
        verify_s,
        micro_s,
        total_s: t_start.elapsed().as_secs_f64(),
        candidates: cands.len(),
        rows,
        refined,
    }
}

fn row_of(idx: usize, c: &Candidate, score: f64, pot: bool, scratch: bool, leave: f64) -> Row {
    Row {
        idx,
        kind: c.kind,
        ball: c.ball,
        pocket: c.pocket,
        seed: c.seed_score,
        score,
        pot,
        scratch,
        leave,
        aim: c.aim,
        speed: c.speed,
        contact: c.contact,
        rails: c.rails,
        inter: c.inter,
        cut_cos: c.cut_cos,
        clearance: c.clearance,
        makeability: c.makeability,
    }
}

/// Apply execution noise to a decision's strike: the difficulty model of #9 §7, aim and speed
/// components only (the toy has no spin or elevation to perturb).
pub fn perturb(c: &Candidate, cfg: &PlannerCfg, rng: &mut SplitMix64) -> (V2, f64) {
    let aim = c.aim.rotate(gauss(rng) * cfg.noise_aim_sigma);
    let speed = c.speed * (1.0 + gauss(rng) * cfg.noise_speed_sigma);
    (aim, speed.max(100.0))
}

fn gauss(rng: &mut SplitMix64) -> f64 {
    rng.gaussian()
}
