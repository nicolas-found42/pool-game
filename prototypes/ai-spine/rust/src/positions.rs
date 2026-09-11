//! Seeded positions for the measurements: drills, eval suites, and the class probes.

use crate::geom::{pockets, Ob, V2, BALL_D, HL, HW, R, XL, YL};
use crate::rng::SplitMix64;

pub struct Position {
    pub cue: V2,
    pub objs: Vec<Ob>,
    pub max_shots: usize,
    pub n_initial: usize,
    pub tag: &'static str,
}

impl Position {
    pub fn live(&self) -> usize {
        self.objs.iter().filter(|o| o.alive).count()
    }
}

fn far_from_pockets(p: V2, margin: f64) -> bool {
    pockets().iter().all(|q| q.p.dist(p) > q.radius + margin)
}

fn fits(p: V2, placed: &[V2], margin: f64) -> bool {
    p.x.abs() < XL - margin && p.y.abs() < YL - margin && placed.iter().all(|q| q.dist(p) > BALL_D + 2.0)
}

/// `n`-ball open table: cue in the head half, object balls spread over the foot two thirds.
pub fn drill(seed: u64, n: usize, max_shots: usize) -> Position {
    let mut rng = SplitMix64::new(seed);
    let mut placed: Vec<V2> = Vec::new();
    // Cue ball: anywhere in the head third, at least 120 mm off the cushions.
    let mut cue = V2::ZERO;
    for _ in 0..500 {
        let c = V2::new(
            rng.range(-XL + 120.0, -180.0),
            rng.range(-YL + 120.0, YL - 120.0),
        );
        if far_from_pockets(c, 40.0) {
            cue = c;
            break;
        }
    }
    placed.push(cue);
    let mut objs = Vec::with_capacity(n);
    for i in 0..n {
        let mut p = V2::new(rng.range(-200.0, XL - 140.0), rng.range(-YL + 60.0, YL - 60.0));
        for _ in 0..400 {
            let cand = V2::new(
                rng.range(-200.0, XL - 140.0),
                rng.range(-YL + 60.0, YL - 60.0),
            );
            if fits(cand, &placed, 60.0) && far_from_pockets(cand, 60.0) && cand.dist(cue) > 4.0 * R {
                p = cand;
                break;
            }
        }
        placed.push(p);
        objs.push(Ob {
            id: i + 1,
            p,
            alive: true,
        });
    }
    Position {
        cue,
        objs,
        max_shots,
        n_initial: n,
        tag: "drill",
    }
}

/// One object ball sitting near a pocket with a clear, low-cut line: the easy-pot suite.
pub fn easy_pot(seed: u64) -> Position {
    let mut rng = SplitMix64::new(seed);
    let ps = pockets();
    for attempt in 0..200 {
        let pk = ps[(rng.next_u64() % 6) as usize];
        // Object ball a short distance inside the pocket, along the pocket's inward direction.
        let inward = match (pk.p.x.abs() > 1.0, pk.p.y.abs() > 1.0) {
            (true, true) => V2::new(-pk.p.x.signum(), -pk.p.y.signum()),
            (true, false) => V2::new(-pk.p.x.signum(), 0.0),
            _ => V2::new(0.0, -pk.p.y.signum()),
        };
        let to_centre = (V2::ZERO - pk.p).unit().unwrap_or(V2::new(-1.0, 0.0));
        let dir = (inward + to_centre).unit().unwrap_or(inward);
        let ob_p = pk.p + dir * rng.range(260.0, 420.0);
        if ob_p.x.abs() > XL - 80.0 || ob_p.y.abs() > YL - 80.0 {
            continue;
        }
        let u = (pk.p - ob_p).unit().unwrap_or(V2::new(1.0, 0.0));
        let ghost = ob_p - u * BALL_D;
        // Place the cue ball at a modest cut angle from the ghost point.
        let theta = rng.range(0.12, 0.55) * if attempt % 2 == 0 { 1.0 } else { -1.0 };
        let d = rng.range(500.0, 1100.0);
        let cue = ghost - u.rotate(theta) * d;
        if cue.x.abs() > XL - 90.0 || cue.y.abs() > YL - 90.0 {
            continue;
        }
        if !far_from_pockets(cue, 40.0) {
            continue;
        }
        let objs = vec![Ob {
            id: 1,
            p: ob_p,
            alive: true,
        }];
        return Position {
            cue,
            objs,
            max_shots: 4,
            n_initial: 1,
            tag: "easy_pot",
        };
    }
    // Deterministic fallback: a straight shot into the bottom-left corner.
    let pk = V2::new(-HL, -HW);
    let ob_p = V2::new(-640.0, -320.0);
    let u = (pk - ob_p).unit().unwrap();
    let ghost = ob_p - u * BALL_D;
    Position {
        cue: ghost - u * 800.0,
        objs: vec![Ob {
            id: 1,
            p: ob_p,
            alive: true,
        }],
        max_shots: 4,
        n_initial: 1,
        tag: "easy_pot",
    }
}

/// Backwards-compatible helper used by dumps: objects from a plain list of points.
pub fn from_points(cue: V2, pts: &[V2], max_shots: usize) -> Position {
    Position {
        cue,
        objs: pts
            .iter()
            .enumerate()
            .map(|(i, p)| Ob {
                id: i + 1,
                p: *p,
                alive: true,
            })
            .collect(),
        max_shots,
        n_initial: pts.len(),
        tag: "fixture",
    }
}

pub const TABLE_HALF_W: f64 = HW;
