//! Candidate generator: recursive mirrored-table / ghost-ball constructions.
//!
//! Direct pots, ≤ 3-rail banks (object ball to a rail then a pocket) and kicks (cue ball to a rail
//! then the object ball), ≤ 1-intermediate-ball combinations, plus safety intent templates.
//! Line-of-sight pruning is analytic; every rail construction is unfolded and re-verified
//! geometrically before it is emitted. Dedup is by predicted contact point, with the approach
//! direction as the tiebreak inside a contact cell.

use std::collections::HashMap;

use crate::geom::{
    chain_path, clearance, path_clearance, pockets, Ob, Rail, V2, BALL_D, RAILS, XL, YL,
};
use crate::toy_sim::speed_for;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Direct,
    Bank,
    Kick,
    Combo,
    SafetyEscape,
    SafetyRollUp,
    SafetyTwoWay,
}

impl Kind {
    pub fn is_safety(self) -> bool {
        matches!(
            self,
            Kind::SafetyEscape | Kind::SafetyRollUp | Kind::SafetyTwoWay
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Direct => "direct",
            Kind::Bank => "bank",
            Kind::Kick => "kick",
            Kind::Combo => "combo",
            Kind::SafetyEscape => "safety_escape",
            Kind::SafetyRollUp => "safety_rollup",
            Kind::SafetyTwoWay => "safety_twoway",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub kind: Kind,
    /// Target object ball id (the called ball), or the first-contact ball for safeties.
    pub ball: usize,
    pub pocket: usize,
    pub aim: V2,
    pub speed: f64,
    /// Predicted contact point: the cue ball's centre at the first contact.
    pub contact: V2,
    pub cut_cos: f64,
    pub rails: u8,
    pub inter: u8,
    pub clearance: f64,
    pub makeability: f64,
    pub leave: f64,
    pub path_len: f64,
    /// Cue-ball travel to the predicted contact point.
    pub d_cue_contact: f64,
    /// Object ball's travel to the pocket (includes its rail legs); 0 for safeties.
    pub d_obj_pocket: f64,
    pub chain: [u8; 3],
    pub chain_len: u8,
    /// Analytic score used to order the shortlist before any sim work.
    pub seed_score: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct GenCfg {
    pub max_rails: u8,
    pub banks: bool,
    pub kicks: bool,
    pub combos: bool,
    pub safeties: bool,
    pub max_cands: usize,
    pub min_cut_cos: f64,
}

impl Default for GenCfg {
    fn default() -> Self {
        Self {
            max_rails: 3,
            banks: true,
            kicks: true,
            combos: true,
            safeties: true,
            max_cands: 32,
            min_cut_cos: 0.10,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GenStats {
    pub constructed: usize,
    pub rej_geometry: usize,
    pub rej_los: usize,
    pub kept: usize,
    pub after_dedup: usize,
    pub by_kind: [usize; 7],
    /// Stage counters for the rail constructions (diagnostics for the prototype).
    pub bank_chain_ok: usize,
    pub bank_cut_ok: usize,
    pub kick_chain_ok: usize,
    pub kick_cut_ok: usize,
    /// Kept candidates per rail depth (index 1..3), pre-dedup.
    pub bank_by_rails: [usize; 4],
    pub kick_by_rails: [usize; 4],
}

fn kind_idx(k: Kind) -> usize {
    match k {
        Kind::Direct => 0,
        Kind::Bank => 1,
        Kind::Kick => 2,
        Kind::Combo => 3,
        Kind::SafetyEscape => 4,
        Kind::SafetyRollUp => 5,
        Kind::SafetyTwoWay => 6,
    }
}

/// All rail chains up to `max_rails` deep, no two adjacent rails equal.
fn chains(max_rails: u8) -> Vec<Vec<Rail>> {
    let mut out = Vec::new();
    for d in 1..=max_rails {
        let mut cur: Vec<Vec<Rail>> = RAILS.iter().map(|r| vec![*r]).collect();
        for _ in 1..d {
            let mut next = Vec::new();
            for c in &cur {
                for r in RAILS.iter() {
                    if Some(*r) != c.last().copied() {
                        let mut c2 = c.clone();
                        c2.push(*r);
                        next.push(c2);
                    }
                }
            }
            cur = next;
        }
        out.extend(cur);
    }
    out
}

fn rail_idx(r: Rail) -> u8 {
    match r {
        Rail::Left => 0,
        Rail::Right => 1,
        Rail::Bottom => 2,
        Rail::Top => 3,
    }
}

/// Analytic makeability heuristic in [0, 1] — an estimate only; the sim is the truth.
pub fn makeability(
    cut_cos: f64,
    d_cue: f64,
    d_path: f64,
    rails: u8,
    inter: u8,
    clearance_margin: f64,
) -> f64 {
    let base = cut_cos.clamp(0.0, 1.0).powf(1.5);
    let dist = (-(d_cue + d_path) / 3500.0).exp();
    let clear = (clearance_margin / 40.0).clamp(0.15, 1.0);
    let rail_pen = 0.55f64.powi(i32::from(rails));
    let inter_pen = 0.45f64.powi(i32::from(inter));
    (base * dist * clear * rail_pen * inter_pen).clamp(0.0, 1.0)
}

/// Cue-ball launch speed for a pot: carry the object ball to the pocket with speed to spare,
/// accounting for the cut, the rail count, the combination loss, and the cloth deceleration on
/// the cue ball's own travel.
fn pot_speed(d_cue: f64, d_path: f64, cut_cos: f64, rails: u8, inter: u8) -> f64 {
    let obj_launch = speed_for(d_path, 700.0);
    let transfer = (0.975 * cut_cos.clamp(0.05, 1.0)).max(0.05);
    let rail_keep = 0.75f64.powi(i32::from(rails));
    let inter_keep = 0.9f64.powi(i32::from(inter));
    let need = obj_launch / (transfer * rail_keep * inter_keep);
    speed_for(d_cue, need).clamp(500.0, 9000.0)
}

struct Builder<'a> {
    cue: V2,
    objs: &'a [Ob],
    ps: [crate::geom::Pocket; 6],
    cfg: GenCfg,
    stats: GenStats,
    out: Vec<Candidate>,
}

impl<'a> Builder<'a> {
    fn push(
        &mut self,
        kind: Kind,
        ball: usize,
        pocket: usize,
        aim: V2,
        speed: f64,
        contact: V2,
        cut_cos: f64,
        rails: u8,
        inter: u8,
        clr: f64,
        path_len: f64,
        d_obj_pocket: f64,
        chain: &[Rail],
        seed_override: Option<f64>,
    ) {
        let mk = makeability(cut_cos, self.cue.dist(contact), path_len, rails, inter, clr);
        let mut ch = [0u8; 3];
        for (i, r) in chain.iter().take(3).enumerate() {
            ch[i] = rail_idx(*r);
        }
        self.out.push(Candidate {
            kind,
            ball,
            pocket,
            aim,
            speed,
            contact,
            cut_cos,
            rails,
            inter,
            clearance: clr,
            makeability: mk,
            leave: 0.0,
            path_len,
            d_cue_contact: self.cue.dist(contact),
            d_obj_pocket,
            chain: ch,
            chain_len: chain.len().min(3) as u8,
            seed_score: seed_override.unwrap_or(mk),
        });
        self.stats.kept += 1;
        self.stats.by_kind[kind_idx(kind)] += 1;
        let d = usize::from(rails).min(3);
        match kind {
            Kind::Bank => self.stats.bank_by_rails[d] += 1,
            Kind::Kick => self.stats.kick_by_rails[d] += 1,
            _ => {}
        }
    }

    fn direct(&mut self) {
        let n_objs = self.objs.len();
        for oi in 0..n_objs {
            let b = self.objs[oi];
            if !b.alive {
                continue;
            }
            let ps_local = self.ps;
            for (pi, pk) in ps_local.iter().enumerate() {
                self.stats.constructed += 1;
                let Some(u) = (pk.p - b.p).unit() else {
                    continue;
                };
                let contact = b.p - u * BALL_D;
                let Some(aim) = (contact - self.cue).unit() else {
                    continue;
                };
                let cut_cos = aim.dot(u);
                if cut_cos <= self.cfg.min_cut_cos {
                    self.stats.rej_geometry += 1;
                    continue;
                }
                let c1 = clearance(self.cue, contact, self.objs, &[b.id]);
                if c1 < 0.0 {
                    self.stats.rej_los += 1;
                    continue;
                }
                let c2 = clearance(b.p, pk.p, self.objs, &[b.id]);
                if c2 < 0.0 {
                    self.stats.rej_los += 1;
                    continue;
                }
                let clr = c1.min(c2);
                let d_path = b.p.dist(pk.p);
                let speed = pot_speed(self.cue.dist(contact), d_path, cut_cos, 0, 0);
                self.push(
                    Kind::Direct,
                    b.id,
                    pi,
                    aim,
                    speed,
                    contact,
                    cut_cos,
                    0,
                    0,
                    clr,
                    d_path,
                    d_path,
                    &[],
                    None,
                );
            }
        }
    }

    fn banks(&mut self, chains: &[Vec<Rail>]) {
        let n_objs = self.objs.len();
        for oi in 0..n_objs {
            let b = self.objs[oi];
            if !b.alive {
                continue;
            }
            let ps_local = self.ps;
            for (pi, pk) in ps_local.iter().enumerate() {
                for chain in chains {
                    self.stats.constructed += 1;
                    let Some(path) = chain_path(b.p, pk.p, chain) else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    self.stats.bank_chain_ok += 1;
                    let Some(d0) = (path.bounces[0] - b.p).unit() else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    let contact = b.p - d0 * BALL_D;
                    let Some(aim) = (contact - self.cue).unit() else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    let cut_cos = aim.dot(d0);
                    if cut_cos <= self.cfg.min_cut_cos {
                        self.stats.rej_geometry += 1;
                        continue;
                    }
                    self.stats.bank_cut_ok += 1;
                    let c1 = clearance(self.cue, contact, self.objs, &[b.id]);
                    if c1 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let mut pts = vec![b.p];
                    pts.extend(path.bounces.iter().copied());
                    pts.push(pk.p);
                    let c2 = path_clearance(&pts, self.objs, &[b.id]);
                    if c2 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let d_path = path_len(&pts);
                    let speed =
                        pot_speed(self.cue.dist(contact), d_path, cut_cos, chain.len() as u8, 0);
                    self.push(
                        Kind::Bank,
                        b.id,
                        pi,
                        aim,
                        speed,
                        contact,
                        cut_cos,
                        chain.len() as u8,
                        0,
                        c1.min(c2),
                        d_path,
                        d_path,
                        chain,
                        None,
                    );
                }
            }
        }
    }

    fn kicks(&mut self, chains: &[Vec<Rail>]) {
        let n_objs = self.objs.len();
        for oi in 0..n_objs {
            let b = self.objs[oi];
            if !b.alive {
                continue;
            }
            let ps_local = self.ps;
            for (pi, pk) in ps_local.iter().enumerate() {
                let Some(u) = (pk.p - b.p).unit() else {
                    continue;
                };
                let contact = b.p - u * BALL_D;
                for chain in chains {
                    self.stats.constructed += 1;
                    let Some(path) = chain_path(self.cue, contact, chain) else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    self.stats.kick_chain_ok += 1;
                    let last = *path.bounces.last().expect("non-empty chain");
                    let Some(d_last) = (contact - last).unit() else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    let cut_cos = d_last.dot(u);
                    if cut_cos <= self.cfg.min_cut_cos {
                        self.stats.rej_geometry += 1;
                        continue;
                    }
                    self.stats.kick_cut_ok += 1;
                    let mut pts = vec![self.cue];
                    pts.extend(path.bounces.iter().copied());
                    pts.push(contact);
                    let c1 = path_clearance(&pts, self.objs, &[b.id]);
                    if c1 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let c2 = clearance(b.p, pk.p, self.objs, &[b.id]);
                    if c2 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let d_path = path_len(&pts);
                    let speed =
                        pot_speed(d_path, b.p.dist(pk.p), cut_cos, chain.len() as u8, 0);
                    let Some(aim) = (path.bounces[0] - self.cue).unit() else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    self.push(
                        Kind::Kick,
                        b.id,
                        pi,
                        aim,
                        speed,
                        contact,
                        cut_cos,
                        chain.len() as u8,
                        0,
                        c1.min(c2),
                        d_path,
                        b.p.dist(pk.p),
                        chain,
                        None,
                    );
                }
            }
        }
    }

    fn combos(&mut self) {
        let n_objs = self.objs.len();
        for oi in 0..n_objs {
            let b = self.objs[oi];
            if !b.alive {
                continue;
            }
            let ps_local = self.ps;
            for (pi, pk) in ps_local.iter().enumerate() {
                let Some(u) = (pk.p - b.p).unit() else {
                    continue;
                };
                let impact = b.p - u * BALL_D;
                let c2 = clearance(b.p, pk.p, self.objs, &[b.id]);
                for mi in 0..n_objs {
                    let m = self.objs[mi];
                    if !m.alive || m.id == b.id {
                        continue;
                    }
                    self.stats.constructed += 1;
                    let Some(dm) = (impact - m.p).unit() else {
                        continue;
                    };
                    let cut2 = dm.dot(u);
                    if cut2 <= self.cfg.min_cut_cos {
                        self.stats.rej_geometry += 1;
                        continue;
                    }
                    let contact = m.p - dm * BALL_D;
                    let Some(aim) = (contact - self.cue).unit() else {
                        self.stats.rej_geometry += 1;
                        continue;
                    };
                    let cut1 = aim.dot(dm);
                    if cut1 <= self.cfg.min_cut_cos {
                        self.stats.rej_geometry += 1;
                        continue;
                    }
                    let c1 = clearance(self.cue, contact, self.objs, &[m.id]);
                    if c1 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let cm = clearance(m.p, impact, self.objs, &[m.id, b.id]);
                    if cm < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    if c2 < 0.0 {
                        self.stats.rej_los += 1;
                        continue;
                    }
                    let d_path = m.p.dist(impact) + b.p.dist(pk.p);
                    let speed = pot_speed(self.cue.dist(contact), d_path, cut1.min(cut2), 0, 1);
                    self.push(
                        Kind::Combo,
                        b.id,
                        pi,
                        aim,
                        speed,
                        contact,
                        cut1.min(cut2),
                        0,
                        1,
                        c1.min(cm).min(c2),
                        d_path,
                        b.p.dist(pk.p),
                        &[],
                        // Combinations are the least reliable family: start them below the
                        // equivalent direct construction regardless of geometry.
                        Some(
                            makeability(
                                cut1.min(cut2),
                                self.cue.dist(contact),
                                d_path,
                                0,
                                1,
                                c1.min(cm),
                            ) * 0.8,
                        ),
                    );
                }
            }
        }
    }

    fn safeties(&mut self) {
        // Nearest few balls only: a safety is about escape and leave, not target choice.
        let mut live: Vec<Ob> = self.objs.iter().copied().filter(|o| o.alive).collect();
        live.sort_by(|a, b| {
            self.cue
                .dist(a.p)
                .partial_cmp(&self.cue.dist(b.p))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        live.truncate(3);
        // Impact parameter k: 0 is a full hit, 1 grazes. The aim angle follows from the geometry
        // (sin a = 2R k / d) and the contact point is where the cue ball's centre reaches 2R from
        // the target's centre along the aim line — not the naive "aim at the ball" offset.
        let offsets = |cue: V2, o: Ob, k: f64| -> Option<(V2, V2, f64, f64)> {
            let d = cue.dist(o.p);
            if d <= BALL_D + 1.0 {
                return None;
            }
            let to_ball = (o.p - cue).unit()?;
            let a = (BALL_D * k / d).clamp(0.0, 1.0).asin();
            for sign in [1.0f64, -1.0] {
                let aim = to_ball.rotate(a * sign);
                let l = (d * d - (BALL_D * k).powi(2)).max(0.0).sqrt();
                let contact = cue + aim * l;
                if contact.x.abs() < XL && contact.y.abs() < YL {
                    return Some((aim, contact, a, l));
                }
            }
            None
        };
        for o in live.iter() {
            for k in [0.45f64, 0.75, 0.95] {
                self.stats.constructed += 1;
                let Some((aim, contact, _a, _l)) = offsets(self.cue, *o, k) else {
                    self.stats.rej_geometry += 1;
                    continue;
                };
                let c1 = clearance(self.cue, contact, self.objs, &[o.id]);
                if c1 < 0.0 {
                    self.stats.rej_los += 1;
                    continue;
                }
                let cut_cos = aim.dot((o.p - self.cue).unit().unwrap_or(aim));
                let speed = speed_for(self.cue.dist(contact) + 900.0, 450.0).clamp(500.0, 2600.0);
                self.push(
                    Kind::SafetyEscape,
                    o.id,
                    usize::MAX,
                    aim,
                    speed,
                    contact,
                    cut_cos,
                    0,
                    0,
                    c1,
                    0.0,
                    0.0,
                    &[],
                    Some(0.30),
                );
            }
        }
        // Roll-up: a half-ball hit at leave pace, sending the cue ball to a rail-side resting place.
        for o in live.iter().take(2) {
            for k in [0.20f64, 0.40] {
                self.stats.constructed += 1;
                let Some((aim, contact, _a, _l)) = offsets(self.cue, *o, k) else {
                    self.stats.rej_geometry += 1;
                    continue;
                };
                let c1 = clearance(self.cue, contact, self.objs, &[o.id]);
                if c1 < 0.0 {
                    self.stats.rej_los += 1;
                    continue;
                }
                let cut_cos = aim.dot((o.p - self.cue).unit().unwrap_or(aim));
                let speed = speed_for(self.cue.dist(contact) + 1600.0, 600.0).clamp(700.0, 3200.0);
                self.push(
                    Kind::SafetyRollUp,
                    o.id,
                    usize::MAX,
                    aim,
                    speed,
                    contact,
                    cut_cos,
                    0,
                    0,
                    c1,
                    0.0,
                    0.0,
                    &[],
                    Some(0.28),
                );
            }
        }
        // Two-way: the best-makeability pot already generated, played at leave pace.
        let best_pot = self
            .out
            .iter()
            .filter(|c| !c.kind.is_safety())
            .max_by(|a, b| a.makeability.partial_cmp(&b.makeability).unwrap())
            .cloned();
        if let Some(mut p) = best_pot {
            self.stats.constructed += 1;
            p.kind = Kind::SafetyTwoWay;
            p.speed *= 0.85;
            p.seed_score = 0.26;
            self.push(
                Kind::SafetyTwoWay,
                p.ball,
                p.pocket,
                p.aim,
                p.speed,
                p.contact,
                p.cut_cos,
                p.rails,
                p.inter,
                p.clearance,
                p.path_len,
                p.d_obj_pocket,
                &[],
                Some(0.26),
            );
        }
    }
}

fn path_len(pts: &[V2]) -> f64 {
    pts.windows(2).map(|w| w[0].dist(w[1])).sum()
}

pub struct GenOut {
    pub candidates: Vec<Candidate>,
    pub stats: GenStats,
    pub elapsed_s: f64,
}

/// Generate the typed candidate set for a position. `cue` is the cue ball's centre.
pub fn generate(cue: V2, objs: &[Ob], cfg: &GenCfg) -> GenOut {
    let t0 = std::time::Instant::now();
    let mut b = Builder {
        cue,
        objs,
        ps: pockets(),
        cfg: *cfg,
        stats: GenStats::default(),
        out: Vec::new(),
    };
    b.direct();
    if cfg.combos {
        b.combos();
    }
    let chains = if cfg.max_rails > 0 {
        chains(cfg.max_rails)
    } else {
        Vec::new()
    };
    if cfg.banks && cfg.max_rails > 0 {
        b.banks(&chains);
    }
    if cfg.kicks && cfg.max_rails > 0 {
        b.kicks(&chains);
    }
    if cfg.safeties {
        b.safeties();
    }

    // Dedup by predicted contact point (1 mm grid), with the approach direction as the tiebreak
    // inside a cell so that two constructions converging on the same contact at genuinely
    // different angles both survive. Keeps the better seed score.
    let mut best: HashMap<(i64, i64, i64, bool), usize> = HashMap::new();
    let mut keep: Vec<Candidate> = Vec::with_capacity(b.out.len());
    for c in b.out.drain(..) {
        let angle_bucket = (c.aim.y.atan2(c.aim.x) / 0.02).round() as i64;
        let key = (
            c.contact.x.round() as i64,
            c.contact.y.round() as i64,
            angle_bucket,
            c.kind.is_safety(),
        );
        match best.get(&key) {
            Some(&i) => {
                if c.seed_score > keep[i].seed_score {
                    keep[i] = c;
                }
            }
            None => {
                best.insert(key, keep.len());
                keep.push(c);
            }
        }
    }
    let after_dedup = keep.len();
    // Order by the analytic seed score: what the shortlist is cut from.
    keep.sort_by(|a, b| {
        b.seed_score
            .partial_cmp(&a.seed_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    keep.truncate(cfg.max_cands);

    let mut stats = b.stats;
    stats.after_dedup = after_dedup;
    GenOut {
        candidates: keep,
        stats,
        elapsed_s: t0.elapsed().as_secs_f64(),
    }
}

/// The ordered shortlist cut: sort + truncate, measured on its own because it sits on the
/// per-decision path.
pub fn shortlist(cands: &mut Vec<Candidate>, k: usize) {
    cands.sort_by(|a, b| {
        b.seed_score
            .partial_cmp(&a.seed_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cands.truncate(k);
}
