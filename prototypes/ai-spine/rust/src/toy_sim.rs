//! Toy analytic table model for the prototype.
//!
//! Spec frame/geometry constants only, no fitted physics (#7 §2/§5): rolling-only motion with a
//! constant deceleration `MU_R*G`, ball–ball impulse with restitution, cushion reflection with an
//! effective normal restitution, pockets as capture discs. No spin, throw, squirt, sliding phase,
//! jaw geometry, or simultaneity handling — every one of those is out of scope for #16 and is
//! deferred to the real sim (#8). Event-driven, so per-shot cost has the same shape (events, not
//! timesteps) as the locked simulation.

use crate::geom::{pockets, Ob, V2, BALL_D, DECEL, E_BB, E_CUSHION, R, SLEEP_V, XL, YL};

/// Contact is accepted within this slack (mm) — conservative advancement converges toward it.
pub const CONTACT_TOL: f64 = 0.02;

#[derive(Clone, Copy, Debug)]
pub struct Ball {
    pub p: V2,
    pub v: V2,
    pub alive: bool,
}

impl Ball {
    pub fn at_rest(p: V2) -> Self {
        Self {
            p,
            v: V2::ZERO,
            alive: true,
        }
    }
    pub fn struck(p: V2, dir: V2, speed: f64) -> Self {
        Self {
            p,
            v: dir * speed,
            alive: true,
        }
    }
    pub fn speed(&self) -> f64 {
        self.v.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Stop(usize),
    BallBall(usize, usize),
    Cushion(usize),
    Pocket(usize),
}

pub struct Shot {
    pub balls: Vec<Ball>,
    pub potted: Vec<usize>,
    pub first_contact: Option<usize>,
    pub events: usize,
    pub wall_time: f64,
}

fn ball_ball_t(pi: V2, vi: V2, pj: V2, vj: V2) -> Option<f64> {
    let dp = pj - pi;
    let dv = vj - vi;
    let a = dv.dot(dv);
    if a <= 1e-12 {
        return None;
    }
    let b = 2.0 * dp.dot(dv);
    let c = dp.dot(dp) - BALL_D * BALL_D;
    if c < 0.0 {
        return Some(0.0);
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if t >= 0.0 {
        Some(t)
    } else {
        None
    }
}

fn single_ball_t(p: V2, v: V2, c: V2, radius: f64) -> Option<f64> {
    let dp = p - c;
    let a = v.dot(v);
    if a <= 1e-12 {
        return None;
    }
    let b = 2.0 * dp.dot(v);
    let cc = dp.dot(dp) - radius * radius;
    if cc <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - 4.0 * a * cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if t >= 0.0 {
        Some(t)
    } else {
        None
    }
}

/// One shot: the given balls (index 0 = cue ball) from their current state to rest.
pub fn shoot(balls: &[Ball], max_events: usize) -> Shot {
    let t0 = std::time::Instant::now();
    let mut balls = balls.to_vec();
    let ps = pockets();
    let mut potted: Vec<usize> = Vec::new();
    let mut first_contact = None;
    let mut n_events = 0usize;

    loop {
        // Earliest event across every candidate.
        let mut best_t = f64::INFINITY;
        let mut best: Option<Event> = None;

        for (i, b) in balls.iter().enumerate() {
            if !b.alive {
                continue;
            }
            let s = b.speed();
            if s > SLEEP_V {
                let t = s / DECEL;
                if t < best_t {
                    best_t = t;
                    best = Some(Event::Stop(i));
                }
                // Cushions (exact kinematics: the ball must still reach the rail).
                let t_stop = s / DECEL;
                for (axis_is_x, limit_pos, limit_neg, vc, pc) in [
                    (true, XL, -XL, b.v.x, b.p.x),
                    (false, YL, -YL, b.v.y, b.p.y),
                ] {
                    let _ = axis_is_x;
                    if vc > 0.0 {
                        let d = limit_pos - pc;
                        if d >= 0.0 {
                            let t = d / vc;
                            if t <= t_stop && t < best_t {
                                best_t = t;
                                best = Some(Event::Cushion(i));
                            }
                        }
                    } else if vc < 0.0 {
                        let d = pc - limit_neg;
                        if d >= 0.0 {
                            let t = d / vc.abs();
                            if t <= t_stop && t < best_t {
                                best_t = t;
                                best = Some(Event::Cushion(i));
                            }
                        }
                    }
                }
                // Pockets.
                for pk in ps.iter() {
                    if let Some(t) = single_ball_t(b.p, b.v, pk.p, pk.radius) {
                        if t <= t_stop && t < best_t {
                            best_t = t;
                            best = Some(Event::Pocket(i));
                        }
                    }
                }
            }
        }
        for i in 0..balls.len() {
            if !balls[i].alive {
                continue;
            }
            for j in (i + 1)..balls.len() {
                if !balls[j].alive {
                    continue;
                }
                if balls[i].speed() <= SLEEP_V && balls[j].speed() <= SLEEP_V {
                    continue;
                }
                if let Some(t) = ball_ball_t(balls[i].p, balls[i].v, balls[j].p, balls[j].v) {
                    if t < best_t {
                        best_t = t;
                        best = Some(Event::BallBall(i, j));
                    }
                }
            }
        }

        let Some(ev) = best else { break };
        if !best_t.is_finite() {
            break;
        }
        if n_events > max_events {
            break;
        }

        // Advance every ball by the event time (each capped at its own stop time).
        for b in balls.iter_mut() {
            if !b.alive {
                continue;
            }
            let s = b.speed();
            if s <= 0.0 {
                continue;
            }
            let dt = best_t.min(s / DECEL);
            let u = b.v / s;
            b.p = b.p + u * (s * dt - 0.5 * DECEL * dt * dt);
            let ns = s - DECEL * dt;
            b.v = if ns <= SLEEP_V { V2::ZERO } else { u * ns };
        }

        match ev {
            Event::Stop(_) => {
                n_events += 1;
            }
            Event::Cushion(i) => {
                n_events += 1;
                // Which rail was crossed: clamp the coordinate that is out of bounds.
                if balls[i].p.x.abs() > XL - 1e-9 {
                    balls[i].p.x = balls[i].p.x.clamp(-XL, XL);
                    balls[i].v.x = -E_CUSHION * balls[i].v.x;
                } else {
                    balls[i].p.y = balls[i].p.y.clamp(-YL, YL);
                    balls[i].v.y = -E_CUSHION * balls[i].v.y;
                }
            }
            Event::Pocket(i) => {
                n_events += 1;
                balls[i].alive = false;
                balls[i].v = V2::ZERO;
                if !potted.contains(&i) {
                    potted.push(i);
                }
            }
            Event::BallBall(i, j) => {
                // Collision detection extrapolates linearly while motion is decelerating, so the
                // estimate can fire before the balls actually touch. Advance conservatively:
                // apply the impulse only at contact, otherwise let the loop re-solve from here.
                let gap = balls[j].p.dist(balls[i].p);
                if gap > BALL_D + CONTACT_TOL {
                    // Not in contact yet: a refinement step, not an event.
                    continue;
                }
                n_events += 1;
                if first_contact.is_none() && (i == 0 || j == 0) {
                    first_contact = Some(if i == 0 { j } else { i });
                }
                let n = match (balls[j].p - balls[i].p).unit() {
                    Some(n) => n,
                    None => V2::new(1.0, 0.0),
                };
                // Separate a residual overlap so the pair cannot re-trigger at t = 0 forever.
                if gap < BALL_D {
                    let push = (BALL_D - gap) / 2.0 + 1e-9;
                    balls[i].p = balls[i].p - n * push;
                    balls[j].p = balls[j].p + n * push;
                }
                let vrel = balls[j].v - balls[i].v;
                let vn = vrel.dot(n);
                if vn < 0.0 {
                    // Equal masses: impulse per unit mass.
                    let jm = -(1.0 + E_BB) * vn / 2.0;
                    balls[i].v = balls[i].v - n * jm;
                    balls[j].v = balls[j].v + n * jm;
                }
            }
        }
    }

    // Anything still moving after the event cap is snapped to rest.
    for b in balls.iter_mut() {
        b.v = V2::ZERO;
    }

    Shot {
        balls,
        potted,
        first_contact,
        events: n_events,
        wall_time: t0.elapsed().as_secs_f64(),
    }
}

/// Convenience: run a strike from a position and report whether ball `target` (an object-ball
/// index into `obs`, where `obs[k]` is the ball's own state) was pocketed, plus whether the cue
/// ball scratched.
pub struct StrikeOutcome {
    pub target_potted: bool,
    pub scratch: bool,
    pub first_contact: Option<usize>,
    pub rest: Vec<Ball>,
    pub events: usize,
    /// Ids of the object balls that were pocketed.
    pub potted_objs: Vec<usize>,
}

pub fn strike(
    cue: V2,
    objs: &[Ob],
    aim: V2,
    speed: f64,
    target_id: Option<usize>,
    max_events: usize,
) -> StrikeOutcome {
    let mut balls = vec![Ball::struck(cue, aim, speed)];
    for ob in objs {
        if ob.alive {
            balls.push(Ball::at_rest(ob.p));
        }
    }
    let shot = shoot(&balls, max_events);
    // Map ball indices back to object-ball ids (index 0 is the cue ball).
    let mut potted_objs: Vec<usize> = Vec::new();
    for &idx in shot.potted.iter().filter(|i| **i > 0) {
        let mut n = 1usize;
        for ob in objs {
            if ob.alive {
                if n == idx {
                    potted_objs.push(ob.id);
                    break;
                }
                n += 1;
            }
        }
    }
    let target_potted = target_id
        .map(|t| potted_objs.contains(&t))
        .unwrap_or(false);
    StrikeOutcome {
        target_potted,
        scratch: shot.potted.contains(&0),
        first_contact: shot.first_contact.map(|k| {
            let mut idx = 0usize;
            for ob in objs {
                if ob.alive {
                    idx += 1;
                    if idx == k {
                        return ob.id;
                    }
                }
            }
            0
        }),
        rest: shot.balls,
        events: shot.events,
        potted_objs,
    }
}

/// Distance a rolling ball travels from `v0` to rest, for speed ladders.
pub fn roll_distance(v0: f64) -> f64 {
    v0 * v0 / (2.0 * DECEL)
}

/// Roughly the speed needed to roll `dist` mm and still arrive with `v_end`.
pub fn speed_for(dist: f64, v_end: f64) -> f64 {
    (v_end * v_end + 2.0 * DECEL * dist.max(0.0)).sqrt()
}

pub const CUSHION_MARGIN: f64 = R;

/// FNV-1a 64 over the canonical state bytes — the same idea as the spec's `state_hash` (#11 §11),
/// used here to show two identical decisions produce identical shots.
pub fn state_hash(balls: &[Ball]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let feed = |bytes: &[u8], h: &mut u64| {
        for b in bytes {
            *h ^= u64::from(*b);
            *h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for b in balls {
        feed(&b.p.x.to_bits().to_le_bytes(), &mut h);
        feed(&b.p.y.to_bits().to_le_bytes(), &mut h);
        feed(&b.v.x.to_bits().to_le_bytes(), &mut h);
        feed(&b.v.y.to_bits().to_le_bytes(), &mut h);
        feed(&[u8::from(b.alive)], &mut h);
    }
    h
}
