//! The analytic event-driven core (#7 §1 / §3).
//!
//! State advances event to event: every candidate event time (ball-ball,
//! ball-cushion, ball-jaw, ball-tip, pocket drop, landing, motion-mode
//! transition) is solved in closed form, the minimum is taken, the impulse or
//! transition is applied, and the loop repeats. Because contacts are *solved*
//! rather than sampled, the core cannot tunnel -- `diag` measures the residual
//! penetration anyway.
//!
//! Every ball carries a local analytic law, so `state_at(t)` is an exact
//! evaluation of the mode in force at `t`, never an interpolation.

use crate::consts::*;
use crate::table::{DropShape, Pocket, Rail, Table, Wall};
use crate::vec::{v3, V3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Sliding,
    Rolling,
    Spinning,
    Stationary,
    Airborne,
}

/// Local motion law of one ball, valid from `law_t`:
///   p(t) = p0 + v0 t + a t^2 / 2
///   v(t) = v0 + a t
///   w(t) = (w0.x + wa.x t, w0.y + wa.y t, sign(w0.z) max(0, |w0.z| - decay t))
#[derive(Clone, Copy, Debug)]
pub struct Law {
    pub p0: V3,
    pub v0: V3,
    pub a: V3,
    pub w0: V3,
    pub wa: V3,
    pub wz_decay: f64,
    pub mode: Mode,
}

impl Law {
    pub fn pos(&self, t: f64) -> V3 {
        self.p0 + self.v0 * t + self.a * (0.5 * t * t)
    }
    pub fn vel(&self, t: f64) -> V3 {
        self.v0 + self.a * t
    }
    pub fn spin(&self, t: f64) -> V3 {
        let xy = self.w0 + self.wa * t;
        let d = self.wz_decay * t;
        let wz = if self.w0.z >= 0.0 {
            (self.w0.z - d).max(0.0)
        } else {
            (self.w0.z + d).min(0.0)
        };
        v3(xy.x, xy.y, wz)
    }
}

#[derive(Clone, Debug)]
pub struct Ball {
    pub id: u8,
    pub p: V3,
    pub v: V3,
    pub w: V3,
    pub mode: Mode,
    pub law: Law,
    pub law_t: f64,
    pub pocketed: bool,
    pub off_table: bool,
    /// Rail(s) this ball was touching at shot start (rules 2.7 input).
    pub frozen_rails: Vec<Rail>,
    /// (wall index, has separated since the shot started) for frozen rails.
    pub frozen_walls: Vec<(usize, bool)>,
    /// Ball ids frozen to this ball at shot start.
    pub frozen_balls: Vec<u8>,
}

impl Ball {
    pub fn new(id: u8, p: V3) -> Ball {
        Ball {
            id,
            p,
            v: V3::ZERO,
            w: V3::ZERO,
            mode: Mode::Stationary,
            law: Law {
                p0: p,
                v0: V3::ZERO,
                a: V3::ZERO,
                w0: V3::ZERO,
                wa: V3::ZERO,
                wz_decay: 0.0,
                mode: Mode::Stationary,
            },
            law_t: 0.0,
            pocketed: false,
            off_table: false,
            frozen_rails: Vec::new(),
            frozen_walls: Vec::new(),
            frozen_balls: Vec::new(),
        }
    }
    pub fn active(&self) -> bool {
        !self.pocketed && !self.off_table
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KickCause {
    CushionPop,
    Landing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FactKind {
    BallBall { a: u8, b: u8 },
    RailContact { ball: u8, rail: Rail, frozen_no_count: bool },
    JawContact { ball: u8, pocket: usize },
    Pocketed { ball: u8, pocket: usize },
    OffTable { ball: u8 },
    Kick { ball: u8, cause: KickCause },
    Rest { ball: u8 },
    SlideToRoll { ball: u8 },
    RollToStop { ball: u8 },
    SpinDown { ball: u8 },
    Freeze { ball: u8 },
    Depenetration { a: u8, b: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct Fact {
    pub seq: u64,
    pub t: f64,
    pub group: u32,
    pub kind: FactKind,
}

#[derive(Clone, Debug)]
pub struct BallState {
    pub p: V3,
    pub v: V3,
    pub w: V3,
    pub mode: Mode,
    pub pocketed: bool,
    pub off_table: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub t_rest: f64,
    pub max_penetration_mm: f64,
    pub min_pair_gap_mm: f64,
    pub max_hop_mm: f64,
    pub runaway: Option<String>,
    pub events: u64,
    pub groups: u64,
}

#[derive(Clone, Copy, Debug)]
enum Ev {
    Wall { ball: u8, wall: usize },
    Tip { ball: u8, tip: usize },
    Pair { i: u8, j: u8 },
    Land { ball: u8 },
    Drop { ball: u8, pocket: usize },
    SlideToRoll { ball: u8 },
    RollToStop { ball: u8 },
    SpinDown { ball: u8 },
}

impl Ev {
    /// Canonical resolution order inside a simultaneity group (#7 §1):
    /// contact type first, then ball ids.
    fn rank(&self) -> (u8, u8, u16) {
        match self {
            Ev::Pair { i, j } => (0, *i, *j as u16),
            Ev::Wall { ball, wall } => (1, *ball, *wall as u16),
            Ev::Tip { ball, tip } => (2, *ball, *tip as u16),
            Ev::Drop { ball, pocket } => (3, *ball, *pocket as u16),
            Ev::Land { ball } => (4, *ball, 0),
            Ev::SlideToRoll { ball } => (5, *ball, 0),
            Ev::RollToStop { ball } => (6, *ball, 0),
            Ev::SpinDown { ball } => (7, *ball, 0),
        }
    }
}

pub struct Sim {
    pub table: Table,
    pub profile: Profile,
    pub balls: Vec<Ball>,
    pub facts: Vec<Fact>,
    pub timeline: Vec<(f64, Vec<Law>)>,
    pub record_timeline: bool,
    pub t: f64,
    pub group: u32,
    pub seq: u64,
    pub outcome: Outcome,
    pub max_events: u64,
    pub max_time: f64,
    pub sleep_v: f64,
    pub sleep_w: f64,
    /// Ladder-only mode: no cushions, jaws or pockets, so a free-space
    /// closed form (TP B-8's draw relation) can be compared like for like.
    pub no_walls: bool,
}

impl Sim {
    pub fn new(profile: Profile, table: Table, positions: &[(u8, V3)]) -> Sim {
        for (id, p) in positions {
            assert!(
                p.z >= R - 1e-9,
                "ball {id} placed below the cloth (z = {}) in the mm frame",
                p.z
            );
        }
        let mut balls: Vec<Ball> = positions
            .iter()
            .map(|(id, p)| {
                let mut b = Ball::new(*id, *p);
                b.law = law_for(&profile, *p, V3::ZERO, V3::ZERO, Mode::Stationary);
                b
            })
            .collect();
        for b in balls.iter_mut() {
            b.law = law_for(&profile, b.p, V3::ZERO, V3::ZERO, Mode::Stationary);
        }
        let timeline = vec![(0.0, balls.iter().map(|b| b.law).collect())];
        Sim {
            table,
            profile,
            balls,
            facts: Vec::new(),
            timeline,
            record_timeline: true,
            t: 0.0,
            group: 0,
            seq: 0,
            outcome: Outcome::default(),
            max_events: 400_000,
            max_time: 240.0,
            sleep_v: SLEEP_V_MM_S,
            sleep_w: SLEEP_W_RAD_S,
            no_walls: false,
        }
    }

    /// Apply a post-impact cue-ball state (the strike declaration's output).
    pub fn strike(&mut self, v: V3, w: V3) {
        self.balls[0].v = v;
        self.balls[0].w = w;
        self.balls[0].mode = motion_mode(self.balls[0].p, v, w);
        self.install_all();
        if self.record_timeline {
            let laws: Vec<Law> = self.balls.iter().map(|b| b.law).collect();
            self.timeline.push((0.0, laws));
        }
    }

    /// Record frozen rails/pairs from the pre-shot state (rules 2.7).
    pub fn annotate_frozen(&mut self) {
        let n = self.balls.len();
        for i in 0..n {
            for (wi, w) in self.table.walls.iter().enumerate() {
                let s = (self.balls[i].p - w.p).dot(w.t);
                let d = (self.balls[i].p - w.p).dot(w.n);
                if w.rail.is_some() && s >= -1e-9 && s <= w.len + 1e-9 && (d - R).abs() <= 0.5 {
                    let r = w.rail.unwrap();
                    if !self.balls[i].frozen_rails.contains(&r) {
                        self.balls[i].frozen_rails.push(r);
                    }
                    if !self.balls[i].frozen_walls.iter().any(|(k, _)| *k == wi) {
                        self.balls[i].frozen_walls.push((wi, false));
                    }
                }
            }
            for j in (i + 1)..n {
                let d = (self.balls[j].p - self.balls[i].p).len();
                if (d - 2.0 * R).abs() <= 0.05 {
                    let bj = self.balls[j].id;
                    let bi = self.balls[i].id;
                    if !self.balls[i].frozen_balls.contains(&bj) {
                        self.balls[i].frozen_balls.push(bj);
                    }
                    if !self.balls[j].frozen_balls.contains(&bi) {
                        self.balls[j].frozen_balls.push(bi);
                    }
                }
            }
        }
    }

    fn install_all(&mut self) {
        let t = self.t;
        let profile = self.profile.clone();
        for b in self.balls.iter_mut() {
            b.law = law_for(&profile, b.p, b.v, b.w, b.mode);
            b.law_t = t;
        }
    }

    /// Exact state of every ball at absolute time `t`.
    pub fn state_at(&self, t: f64) -> Vec<BallState> {
        let idx = match self.timeline.binary_search_by(|e| e.0.partial_cmp(&t).unwrap()) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        let (t0, laws) = &self.timeline[idx];
        let tau = t - t0;
        laws.iter()
            .zip(self.balls.iter())
            .map(|(law, b)| BallState {
                p: law.pos(tau),
                v: law.vel(tau),
                w: law.spin(tau),
                mode: law.mode,
                pocketed: b.pocketed,
                off_table: b.off_table,
            })
            .collect()
    }

    fn push_fact(&mut self, kind: FactKind) {
        self.facts.push(Fact {
            seq: self.seq,
            t: self.t,
            group: self.group,
            kind,
        });
        self.seq += 1;
    }

    fn snapshot(&mut self) {
        if self.record_timeline {
            let laws: Vec<Law> = self.balls.iter().map(|b| b.law).collect();
            self.timeline.push((self.t, laws));
        }
    }

    /// Candidate list at the current time (debug).
    pub fn candidates_debug(&self) -> Vec<(f64, String)> {
        self.candidates()
            .into_iter()
            .map(|(t, e)| (t, format!("{e:?}")))
            .collect()
    }

    /// One event-group step (used by the debug walk).
    pub fn step_once(&mut self) {
        let cands = self.candidates();
        let mut best = f64::INFINITY;
        for (t, _) in &cands {
            if *t < best {
                best = *t;
            }
        }
        if !best.is_finite() {
            self.outcome.runaway = Some("no candidates".into());
            return;
        }
        let t_next = if best < self.t { self.t } else { best };
        let mut group: Vec<(f64, Ev)> = cands
            .into_iter()
            .filter(|(t, _)| *t <= best + EPS_GROUP)
            .collect();
        group.sort_by(|a, b| a.1.rank().cmp(&b.1.rank()));
        self.t = t_next;
        self.group += 1;
        self.outcome.groups += 1;
        for i in 0..self.balls.len() {
            if !self.balls[i].active() {
                continue;
            }
            let tau = self.t - self.balls[i].law_t;
            let law = self.balls[i].law;
            self.balls[i].p = law.pos(tau);
            self.balls[i].v = law.vel(tau);
            self.balls[i].w = law.spin(tau);
        }
        self.update_separation();
        for (_, ev) in group {
            self.outcome.events += 1;
            self.apply(ev);
        }
        self.install_all();
        self.snapshot();
        self.check_rest();
    }

    pub fn run_to_rest(&mut self) -> Outcome {
        let mut stall = 0u32;
        loop {
            if self.outcome.events > self.max_events {
                self.outcome.runaway = Some(format!("event cap {} hit", self.max_events));
                break;
            }
            if self.t > self.max_time {
                self.outcome.runaway = Some(format!("time cap {} s hit", self.max_time));
                break;
            }
            let facts_before = self.facts.len();
            let cands = self.candidates();
            let mut best = f64::INFINITY;
            for (t, _) in &cands {
                if *t < best {
                    best = *t;
                }
            }
            if !best.is_finite() {
                break; // nothing left to happen
            }
            let t_next = if best < self.t { self.t } else { best };
            let mut group: Vec<(f64, Ev)> = cands
                .into_iter()
                .filter(|(t, _)| *t <= best + EPS_GROUP)
                .collect();
            group.sort_by(|a, b| a.1.rank().cmp(&b.1.rank()));
            self.t = t_next;
            self.group += 1;
            self.outcome.groups += 1;
            for i in 0..self.balls.len() {
                if !self.balls[i].active() {
                    continue;
                }
                let tau = self.t - self.balls[i].law_t;
                let law = self.balls[i].law;
                self.balls[i].p = law.pos(tau);
                self.balls[i].v = law.vel(tau);
                self.balls[i].w = law.spin(tau);
            }
            self.update_separation();
            for (_, ev) in group {
                self.outcome.events += 1;
                self.apply(ev);
            }
            for i in 0..self.balls.len() {
                let h = self.balls[i].p.z - R;
                if h > self.outcome.max_hop_mm {
                    self.outcome.max_hop_mm = h;
                }
            }
            self.install_all();
            self.snapshot();
            self.check_rest();
            if self.facts.len() == facts_before && t_next <= self.t + 1e-15 {
                stall += 1;
                if stall > 200 {
                    self.outcome.runaway = Some("no-progress stall".to_string());
                    break;
                }
            } else {
                stall = 0;
            }
        }
        self.outcome.t_rest = self.t;
        let (pen, gap, hop) = self.measure();
        self.outcome.max_penetration_mm = pen;
        self.outcome.min_pair_gap_mm = gap;
        if hop > self.outcome.max_hop_mm {
            self.outcome.max_hop_mm = hop;
        }
        self.outcome.clone()
    }

    /// Mark a frozen-rail ball as separated once it is clear of the cushion.
    fn update_separation(&mut self) {
        for i in 0..self.balls.len() {
            let p = self.balls[i].p;
            let walls = self.table.walls.clone();
            for (wi, sep) in self.balls[i].frozen_walls.iter_mut() {
                if *sep {
                    continue;
                }
                let w = &walls[*wi];
                let d = (p - w.p).dot(w.n);
                if d > R + 0.5 {
                    *sep = true;
                }
            }
        }
    }

    fn check_rest(&mut self) {
        for i in 0..self.balls.len() {
            let b = &self.balls[i];
            if !b.active() || b.mode == Mode::Stationary || b.mode == Mode::Airborne {
                continue;
            }
            if b.v.len() <= self.sleep_v && b.w.abs_max() <= self.sleep_w {
                let id = b.id;
                let ball = &mut self.balls[i];
                ball.v = V3::ZERO;
                ball.w = V3::ZERO;
                ball.mode = Mode::Stationary;
                self.push_fact(FactKind::Rest { ball: id });
            }
        }
    }

    fn measure(&self) -> (f64, f64, f64) {
        let mut pen = 0.0f64;
        let mut gap = f64::INFINITY;
        let mut hop = 0.0f64;
        for b in &self.balls {
            if !b.active() {
                continue;
            }
            for w in &self.table.walls {
                let s = (b.p - w.p).dot(w.t).max(0.0).min(w.len);
                let closest = w.p + w.t * s;
                let d = (b.p - closest).len();
                let p = R - d;
                if p > pen {
                    pen = p;
                }
            }
            let h = b.p.z - R;
            if h > hop {
                hop = h;
            }
        }
        for i in 0..self.balls.len() {
            for j in (i + 1)..self.balls.len() {
                if !self.balls[i].active() || !self.balls[j].active() {
                    continue;
                }
                let g = (self.balls[j].p - self.balls[i].p).len() - 2.0 * R;
                if g < gap {
                    gap = g;
                }
            }
        }
        if !gap.is_finite() {
            gap = 0.0;
        }
        (pen, gap, hop)
    }

    // ------------------------------------------------------------------
    // candidate events
    // ------------------------------------------------------------------

    fn candidates(&self) -> Vec<(f64, Ev)> {
        let mut out: Vec<(f64, Ev)> = Vec::with_capacity(64);
        let n = self.balls.len();
        for i in 0..n {
            let b = &self.balls[i];
            if !b.active() {
                continue;
            }
            let tau = self.t - b.law_t;
            let p = b.law.pos(tau);
            let v = b.law.vel(tau);
            let w = b.law.spin(tau);
            match b.mode {
                Mode::Stationary => {}
                Mode::Spinning => {
                    let t = spin_down_time(w, &self.profile, self.sleep_w);
                    if t.is_finite() && t > 0.0 {
                        out.push((self.t + t, Ev::SpinDown { ball: i as u8 }));
                    }
                }
                Mode::Rolling => {
                    let sp = v.len();
                    if sp > 0.0 {
                        out.push((
                            self.t + sp / self.profile.a_roll(),
                            Ev::RollToStop { ball: i as u8 },
                        ));
                    }
                }
                Mode::Sliding => {
                    let u = contact_point_velocity(v, w);
                    let ul = u.len();
                    if ul > 0.0 {
                        out.push((
                            self.t + ul / (3.5 * self.profile.a_slide()),
                            Ev::SlideToRoll { ball: i as u8 },
                        ));
                    }
                }
                Mode::Airborne => {
                    if let Some(dt) = solve_z_landing(p.z, v.z) {
                        out.push((self.t + dt, Ev::Land { ball: i as u8 }));
                    }
                }
            }
            let high = b.mode == Mode::Airborne && p.z - R > CUSHION_CONTACT_CEILING || self.no_walls;
            if !high {
                for (wi, wall) in self.table.walls.iter().enumerate() {
                    if let Some(dt) = solve_wall(p, v, b.law.a, wall) {
                        out.push((self.t + dt, Ev::Wall { ball: i as u8, wall: wi }));
                    }
                }
                for (ti, tip) in self.table.tips.iter().enumerate() {
                    if let Some(dt) = solve_point(p, v, b.law.a, *tip) {
                        out.push((self.t + dt, Ev::Tip { ball: i as u8, tip: ti }));
                    }
                }
            }
            for (pi, pk) in self.table.pockets.iter().enumerate() {
                if high {
                    continue; // a ball above the rail flies over the pocket too
                }
                let _ = pk;
                if let Some(dt) = solve_drop(p, v, b.law.a, pk) {
                    out.push((self.t + dt, Ev::Drop { ball: i as u8, pocket: pi }));
                }
            }
        }
        for i in 0..n {
            for j in (i + 1)..n {
                if !self.balls[i].active() || !self.balls[j].active() {
                    continue;
                }
                let (bi, bj) = (&self.balls[i], &self.balls[j]);
                let ti = self.t - bi.law_t;
                let tj = self.t - bj.law_t;
                let dp = bi.law.pos(ti) - bj.law.pos(tj);
                let dv = bi.law.vel(ti) - bj.law.vel(tj);
                if dp.dot(dv) >= 0.0 {
                    continue;
                }
                if std::env::var("PC_TRACE").is_ok() && self.t < 1e-9 && i == 0 && j == 1 {
                    eprintln!(
                        "TRACE pair({i},{j}) dp={dp} dv={dv} da={} modes={:?}/{:?} law_t=({},{})",
                        bi.law.a - bj.law.a,
                        bi.law.mode,
                        bj.law.mode,
                        bi.law_t,
                        bj.law_t
                    );
                }
                if let Some(dt) = solve_pair(dp, dv, bi.law.a - bj.law.a) {
                    out.push((
                        self.t + dt,
                        Ev::Pair {
                            i: i as u8,
                            j: j as u8,
                        },
                    ));
                }
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // event application
    // ------------------------------------------------------------------

    fn apply(&mut self, ev: Ev) {
        match ev {
            Ev::Wall { ball, wall } => {
                let w = self.table.walls[wall].clone();
                // #7 §3: the nose height (63.5% of ball diameter) is in the
                // contact normal, so a cushion face's normal is tilted down by
                // asin((h_nose - R)/R) and its horizontal part shrinks to
                // sqrt(R^2 - (h_nose-R)^2)/R = 0.96286.
                let nx = nose_normal_magnitude();
                let nz = nose_normal_z();
                let n = v3(w.n.x * nx, w.n.y * nx, -nz);
                if w.rail.is_some() {
                    self.impulse_surface(ball as usize, n, Some(w.rail.unwrap()), None, Some(wall));
                } else {
                    self.impulse_surface(ball as usize, n, None, w.pocket, Some(wall));
                }
            }
            Ev::Tip { ball, tip } => {
                let tp = self.table.tips[tip];
                let p = self.balls[ball as usize].p;
                let n = (p - tp).norm();
                if n.len() == 0.0 {
                    return;
                }
                let pocket = self.nearest_pocket(tp);
                self.impulse_surface(ball as usize, n, None, Some(pocket), None);
            }
            Ev::Pair { i, j } => self.hit_pair(i as usize, j as usize),
            Ev::Land { ball } => self.land(ball as usize),
            Ev::Drop { ball, pocket } => self.drop(ball as usize, pocket),
            Ev::SlideToRoll { ball } => self.slide_to_roll(ball as usize),
            Ev::RollToStop { ball } => self.roll_to_stop(ball as usize),
            Ev::SpinDown { ball } => self.spin_down(ball as usize),
        }
    }

    fn slide_to_roll(&mut self, i: usize) {
        let sleep_v = self.sleep_v;
        let sleep_w = self.sleep_w;
        let (v, wz, id) = {
            let b = &self.balls[i];
            (b.v, b.w.z, b.id)
        };
        let sp = v.len();
        let ball = &mut self.balls[i];
        if sp <= sleep_v {
            ball.v = V3::ZERO;
            ball.w.x = 0.0;
            ball.w.y = 0.0;
            if wz.abs() > sleep_w {
                ball.mode = Mode::Spinning;
            } else {
                ball.w = V3::ZERO;
                ball.mode = Mode::Stationary;
            }
            self.push_fact(FactKind::RollToStop { ball: id });
            return;
        }
        ball.w.x = -v.y / R;
        ball.w.y = v.x / R;
        ball.mode = Mode::Rolling;
        self.push_fact(FactKind::SlideToRoll { ball: id });
    }

    fn roll_to_stop(&mut self, i: usize) {
        let sleep_w = self.sleep_w;
        let (wz, id) = {
            let b = &self.balls[i];
            (b.w.z, b.id)
        };
        let ball = &mut self.balls[i];
        ball.v = V3::ZERO;
        ball.w.x = 0.0;
        ball.w.y = 0.0;
        if wz.abs() > sleep_w {
            ball.mode = Mode::Spinning;
        } else {
            ball.w = V3::ZERO;
            ball.mode = Mode::Stationary;
        }
        self.push_fact(FactKind::RollToStop { ball: id });
    }

    fn spin_down(&mut self, i: usize) {
        let id = self.balls[i].id;
        self.balls[i].w = V3::ZERO;
        self.balls[i].mode = Mode::Stationary;
        self.push_fact(FactKind::SpinDown { ball: id });
    }

    fn land(&mut self, i: usize) {
        let id = self.balls[i].id;
        let e_slate = self.profile.e_slate;
        let mut kicked = false;
        {
            let b = &mut self.balls[i];
            if b.v.z < 0.0 {
                b.v.z = -e_slate * b.v.z;
            }
            b.p.z = R;
            if b.v.z > 1e-6 {
                kicked = true;
            } else {
                b.v.z = 0.0;
            }
            b.mode = motion_mode(b.p, b.v, b.w);
        }
        if kicked {
            self.push_fact(FactKind::Kick {
                ball: id,
                cause: KickCause::Landing,
            });
        }
        let p = self.balls[i].p;
        if p.x.abs() > HALF_LEN + 1e-6 || p.y.abs() > HALF_WID + 1e-6 {
            let b = &mut self.balls[i];
            b.off_table = true;
            b.v = V3::ZERO;
            b.w = V3::ZERO;
            b.mode = Mode::Stationary;
            self.push_fact(FactKind::OffTable { ball: id });
        }
    }

    fn drop(&mut self, i: usize, pocket: usize) {
        let id = self.balls[i].id;
        {
            let b = &mut self.balls[i];
            b.pocketed = true;
            b.v = V3::ZERO;
            b.w = V3::ZERO;
            b.mode = Mode::Stationary;
        }
        self.push_fact(FactKind::Pocketed {
            ball: id,
            pocket,
        });
    }

    fn nearest_pocket(&self, p: V3) -> usize {
        let mut best = 0;
        let mut bd = f64::INFINITY;
        for pk in &self.table.pockets {
            let d = (p - pk.mouth_center).len();
            if d < bd {
                bd = d;
                best = pk.id;
            }
        }
        best
    }

    /// Frictional-inelastic impulse with the nose-height contact normal
    /// (#7 §3). `n` is the inward unit normal, `r_c = -R n`.
    fn impulse_surface(
        &mut self,
        i: usize,
        n: V3,
        rail: Option<Rail>,
        pocket: Option<usize>,
        wall_idx: Option<usize>,
    ) {
        let (e_n, mu_c, e_slate) = (
            self.profile.e_cushion,
            self.profile.mu_cushion,
            self.profile.e_slate,
        );
        let near_cloth = self.balls[i].p.z - R <= 1.0;
        let id = self.balls[i].id;
        let mut popped = false;
        {
            let b = &mut self.balls[i];
            let v_n = b.v.dot(n);
            if v_n >= 0.0 {
                return; // separating (group re-check)
            }
            let rc = n * (-R);
            let jn = -(1.0 + e_n) * v_n * M_BALL;
            let u = b.v + b.w.cross(rc);
            let u_t = u - n * u.dot(n);
            let utl = u_t.len();
            let jt = if utl > 1e-9 {
                let stick = utl * M_BALL / 7.0;
                let cap = mu_c * jn;
                let mag = if stick < cap { stick } else { cap };
                u_t.norm() * (-mag)
            } else {
                V3::ZERO
            };
            let j = n * jn + jt;
            b.v = b.v + j / M_BALL;
            b.w = b.w + rc.cross(j) / I_BALL;
            if near_cloth && b.v.z < 0.0 {
                // the tilted normal drives the ball into the cloth; the cloth
                // recovers with the slate restitution (#7 §5 e_slate)
                b.v.z = -e_slate * b.v.z;
                if b.v.z > 1e-6 {
                    popped = true;
                } else {
                    b.v.z = 0.0;
                }
            }
            b.mode = motion_mode(b.p, b.v, b.w);
            if b.mode == Mode::Stationary {
                b.v = V3::ZERO;
                b.w = V3::ZERO;
            }
        }
        if let Some(r) = rail {
            // rules 2.7: a ball frozen to this rail at shot start is not driven
            // to it unless it has left first
            let frozen_no_count = match wall_idx {
                Some(wi) => self.balls[i]
                    .frozen_walls
                    .iter()
                    .any(|(k, sep)| *k == wi && !*sep),
                None => false,
            };
            self.push_fact(FactKind::RailContact {
                ball: id,
                rail: r,
                frozen_no_count,
            });
        } else if let Some(p) = pocket {
            self.push_fact(FactKind::JawContact { ball: id, pocket: p });
        }
        if popped {
            self.push_fact(FactKind::Kick {
                ball: id,
                cause: KickCause::CushionPop,
            });
        }
    }

    fn hit_pair(&mut self, i: usize, j: usize) {
        let (pi, pj) = (self.balls[i].p, self.balls[j].p);
        let n = (pj - pi).norm(); // i -> j
        if n.len() == 0.0 {
            return;
        }
        let v_rel = self.balls[i].v - self.balls[j].v;
        let v_n = v_rel.dot(n);
        if v_n <= 0.0 {
            return; // separating (group re-check)
        }
        if v_n <= 0.0 {
            // overlapping but separating: depenetrate along the line of centres
            let d = (pj - pi).len();
            if d < 2.0 * R - 1e-6 {
                let push = (2.0 * R - d) * 0.5;
                self.balls[i].p = self.balls[i].p - n * push;
                self.balls[j].p = self.balls[j].p + n * push;
                self.push_fact(FactKind::Depenetration {
                    a: self.balls[i].id,
                    b: self.balls[j].id,
                });
            }
            return;
        }
        let e = self.profile.e_b;
        let jn = (1.0 + e) * v_n * M_BALL / 2.0;
        let ri = n * R; // i's centre -> contact point
        let rj = n * (-R); // j's centre -> contact point
        let ui = self.balls[i].v + self.balls[i].w.cross(ri);
        let uj = self.balls[j].v + self.balls[j].w.cross(rj);
        let u = ui - uj; // i's contact point relative to j's
        let u_t = u - n * u.dot(n);
        let utl = u_t.len();
        let jt_vec = if utl > 1e-9 {
            let stick = utl * M_BALL / 7.0;
            let mu = self.profile.mu_b.eval(utl);
            let cap = mu * jn;
            let mag = if stick < cap { stick } else { cap };
            u_t.norm() * (-mag) // friction impulse on i
        } else {
            V3::ZERO
        };
        let imp_i = n * (-jn) + jt_vec; // normal + friction impulse on i
        let imp_j = imp_i * -1.0; // third law
        {
            let bi = &mut self.balls[i];
            bi.v = bi.v + imp_i / M_BALL;
            bi.w = bi.w + ri.cross(imp_i) / I_BALL;
            bi.mode = motion_mode(bi.p, bi.v, bi.w);
        }
        {
            let bj = &mut self.balls[j];
            bj.v = bj.v + imp_j / M_BALL;
            bj.w = bj.w + rj.cross(imp_j) / I_BALL;
            bj.mode = motion_mode(bj.p, bj.v, bj.w);
        }
        let (id_i, id_j) = (self.balls[i].id, self.balls[j].id);
        let d = (self.balls[j].p - self.balls[i].p).len();
        if d < 2.0 * R - 1e-6 {
            let push = (2.0 * R - d) * 0.5;
            self.balls[i].p = self.balls[i].p - n * push;
            self.balls[j].p = self.balls[j].p + n * push;
            self.push_fact(FactKind::Depenetration { a: id_i, b: id_j });
        }
        self.push_fact(FactKind::BallBall {
            a: id_i,
            b: id_j,
        });
    }
}

// ----------------------------------------------------------------------
// mode and law construction
// ----------------------------------------------------------------------

pub fn contact_point_velocity(v: V3, w: V3) -> V3 {
    // contact point is at (0,0,-R) from the centre
    v3(v.x - R * w.y, v.y + R * w.x, 0.0)
}

pub fn is_airborne(p: V3, v: V3) -> bool {
    p.z - R > 1e-9 || v.z.abs() > 1e-7
}

/// Canonical mode determination from a state (used after every impulse).
pub fn motion_mode(p: V3, v: V3, w: V3) -> Mode {
    if is_airborne(p, v) {
        return Mode::Airborne;
    }
    let u = contact_point_velocity(v, w);
    if u.len() > 1e-6 {
        Mode::Sliding
    } else if v.len() > SLEEP_V_MM_S {
        Mode::Rolling
    } else if w.abs_max() > SLEEP_W_RAD_S {
        Mode::Spinning
    } else {
        Mode::Stationary
    }
}

pub fn law_for(profile: &Profile, p: V3, v: V3, w: V3, mode: Mode) -> Law {
    match mode {
        Mode::Stationary => Law {
            p0: p,
            v0: V3::ZERO,
            a: V3::ZERO,
            w0: V3::ZERO,
            wa: V3::ZERO,
            wz_decay: 0.0,
            mode,
        },
        Mode::Spinning => Law {
            p0: p,
            v0: V3::ZERO,
            a: V3::ZERO,
            w0: v3(0.0, 0.0, w.z),
            wa: V3::ZERO,
            wz_decay: profile.spin_decay,
            mode,
        },
        Mode::Rolling => {
            let a = v.norm() * (-profile.a_roll());
            Law {
                p0: p,
                v0: v,
                a,
                w0: v3(-v.y / R, v.x / R, w.z),
                wa: v3(-a.y / R, a.x / R, 0.0),
                wz_decay: profile.spin_decay,
                mode,
            }
        }
        Mode::Sliding => {
            let u = contact_point_velocity(v, w);
            let a = u.norm() * (-profile.a_slide());
            let wa = v3(a.y, -a.x, 0.0) * (5.0 / (2.0 * R));
            Law {
                p0: p,
                v0: v,
                a,
                w0: w,
                wa,
                wz_decay: profile.spin_decay,
                mode,
            }
        }
        Mode::Airborne => Law {
            p0: p,
            v0: v,
            a: v3(0.0, 0.0, -G),
            w0: w,
            wa: V3::ZERO,
            wz_decay: 0.0,
            mode,
        },
    }
}

// ----------------------------------------------------------------------
// closed-form solvers
// ----------------------------------------------------------------------

fn smallest_positive_root(a: f64, b: f64, c: f64) -> Option<f64> {
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return None;
        }
        let t = -c / b;
        return if t > 0.0 { Some(t) } else { None };
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t1 = (-b - s) / (2.0 * a);
    let t2 = (-b + s) / (2.0 * a);
    let (lo, hi) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
    if lo > 0.0 {
        Some(lo)
    } else if hi > 0.0 {
        Some(hi)
    } else {
        None
    }
}

fn solve_wall(p: V3, v: V3, acc: V3, w: &Wall) -> Option<f64> {
    let d0 = (p - w.p).dot(w.n) - R;
    let d1 = v.dot(w.n);
    if d1 >= 0.0 {
        // not approaching the face: a ball sliding along a cushion at exactly
        // R would otherwise re-trigger at t = 0 for ever
        return None;
    }
    let d2 = 0.5 * acc.dot(w.n);
    let t = smallest_positive_root(d2, d1, d0)?;
    if t > 1e6 {
        return None;
    }
    let pc = p + v * t + acc * (0.5 * t * t);
    let s = (pc - w.p).dot(w.t);
    if s < -1e-9 || s > w.len + 1e-9 {
        return None;
    }
    Some(t)
}

fn solve_point(p: V3, v: V3, acc: V3, tp: V3) -> Option<f64> {
    let dp = p - tp;
    if dp.dot(v) >= 0.0 {
        return None; // not approaching the jaw edge
    }
    let a = v.dot(v);
    let b = 2.0 * dp.dot(v);
    let c = dp.dot(dp) - R * R;
    let mut t = smallest_positive_root(a, b, c)?;
    for _ in 0..3 {
        let pos = dp + v * t + acc * (0.5 * t * t);
        let vel = v + acc * t;
        let f = pos.dot(pos) - R * R;
        let fp = 2.0 * pos.dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        t -= f / fp;
        if t <= 0.0 {
            t = 1e-9;
        }
    }
    if t > 0.0 {
        Some(t)
    } else {
        None
    }
}

pub fn solve_pair_pub(dp: V3, dv: V3, da: V3) -> Option<f64> {
    solve_pair(dp, dv, da)
}

fn solve_pair(dp: V3, dv: V3, da: V3) -> Option<f64> {
    let d2 = dp.dot(dp);
    if d2 < 4.0 * R * R - 1e-9 {
        // already overlapping: resolve at the current instant
        return Some(0.0);
    }
    let a = dv.dot(dv);
    let b = 2.0 * dp.dot(dv);
    let c = dp.dot(dp) - 4.0 * R * R;
    let mut t = smallest_positive_root(a, b, c)?;
    for _ in 0..3 {
        let pos = dp + dv * t + da * (0.5 * t * t);
        let vel = dv + da * t;
        let f = pos.dot(pos) - 4.0 * R * R;
        let fp = 2.0 * pos.dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        t -= f / fp;
        if t <= 0.0 {
            t = 1e-9;
        }
    }
    if t > 0.0 {
        Some(t)
    } else {
        None
    }
}

fn solve_z_landing(z: f64, vz: f64) -> Option<f64> {
    let d0 = z - R;
    if d0 <= 0.0 && vz <= 0.0 {
        // already at or below the cloth and not rising: land now
        return Some(0.0);
    }
    smallest_positive_root(-0.5 * G, vz, d0)
}

fn solve_drop(p: V3, v: V3, acc: V3, pk: &Pocket) -> Option<f64> {
    let f0 = pk.shape.f(p);
    if f0 >= 0.0 {
        return Some(0.0); // already over the mouth
    }
    let mut t = match &pk.shape {
        DropShape::ChordPlane { m, outward, .. } => {
            let d = (p - *m).dot(*outward);
            let denom = v.dot(*outward);
            if denom <= 0.0 {
                return None; // not heading into the pocket
            }
            -d / denom
        }
        DropShape::Disc { c, r } => {
            let dp = p - *c;
            let a = v.dot(v);
            let b = 2.0 * dp.dot(v);
            let cc = dp.dot(dp) - r * r;
            smallest_positive_root(a, b, cc)?
        }
    };
    for _ in 0..4 {
        let pos = p + v * t + acc * (0.5 * t * t);
        let vel = v + acc * t;
        let f = pk.shape.f(pos);
        let grad = match &pk.shape {
            DropShape::ChordPlane { outward, .. } => *outward,
            DropShape::Disc { c, .. } => -(pos - *c).norm(),
        };
        let fp = grad.dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        let step = f / fp;
        if !step.is_finite() {
            return None;
        }
        t -= step;
        if t <= 0.0 {
            t = 1e-9;
        }
    }
    if t <= 0.0 || t > 1e4 {
        return None;
    }
    let pos = p + v * t + acc * (0.5 * t * t);
    // the root must actually be on the boundary, and inside the mouth
    if pk.shape.f(pos).abs() > 1.0 {
        return None;
    }
    if !pk.shape.lateral_ok(pos) {
        return None;
    }
    Some(t)
}

fn spin_down_time(w: V3, profile: &Profile, sleep_w: f64) -> f64 {
    if profile.spin_decay <= 0.0 {
        return f64::INFINITY;
    }
    let rem = w.z.abs() - sleep_w;
    if rem <= 0.0 {
        0.0
    } else {
        rem / profile.spin_decay
    }
}
