//! Table frame, geometry constants, and geometric predicates.
//!
//! Constants are the spec frame/geometry set only (physics resolution #7 §2/§5). There is no
//! fitted physics anywhere in this prototype: no spin, no throw, no squirt, no cushion spin
//! transfer, no sliding phase.

use std::ops::{Add, Div, Mul, Neg, Sub};

pub const L: f64 = 2540.0; // playing surface length, x
pub const W: f64 = 1270.0; // playing surface width, y
pub const HL: f64 = L / 2.0;
pub const HW: f64 = W / 2.0;
pub const R: f64 = 28.575; // ball radius, mm
pub const BALL_D: f64 = 2.0 * R;
pub const CORNER_MOUTH: f64 = 115.9;
pub const SIDE_MOUTH: f64 = 128.6;
pub const E_BB: f64 = 0.95; // ball–ball restitution (profile default)
pub const E_CUSHION: f64 = 0.75; // cushion effective e_n (profile default)
pub const MU_R: f64 = 0.010; // cloth rolling coefficient (profile default)
pub const G: f64 = 9806.65; // mm/s^2
pub const DECEL: f64 = MU_R * G; // mm/s^2, rolling-only deceleration
pub const SLEEP_V: f64 = 1.0; // mm/s sleep threshold
/// Ball-centre limits (cushion contact lines).
pub const XL: f64 = HL - R;
pub const YL: f64 = HW - R;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

impl V2 {
    pub const ZERO: V2 = V2 { x: 0.0, y: 0.0 };
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: V2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn dist(self, o: V2) -> f64 {
        (self - o).len()
    }
    pub fn unit(self) -> Option<V2> {
        let n = self.len();
        if n < 1e-12 {
            None
        } else {
            Some(self / n)
        }
    }
    /// Rotate by `a` radians.
    pub fn rotate(self, a: f64) -> V2 {
        let (s, c) = (a.sin(), a.cos());
        V2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, s: f64) -> V2 {
        V2::new(self.x * s, self.y * s)
    }
}
impl Div<f64> for V2 {
    type Output = V2;
    fn div(self, s: f64) -> V2 {
        V2::new(self.x / s, self.y / s)
    }
}
impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2::new(-self.x, -self.y)
    }
}

/// The four rails, named by the cushion they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    Left,
    Right,
    Bottom,
    Top,
}

pub const RAILS: [Rail; 4] = [Rail::Left, Rail::Right, Rail::Bottom, Rail::Top];

impl Rail {
    /// Mirror a point across this rail line.
    pub fn reflect(self, p: V2) -> V2 {
        match self {
            Rail::Left => V2::new(-2.0 * XL - p.x, p.y),
            Rail::Right => V2::new(2.0 * XL - p.x, p.y),
            Rail::Bottom => V2::new(p.x, -2.0 * YL - p.y),
            Rail::Top => V2::new(p.x, 2.0 * YL - p.y),
        }
    }

    pub fn is_x(self) -> bool {
        matches!(self, Rail::Left | Rail::Right)
    }

    /// The rail line's constant coordinate (`x = ±XL`, `y = ±YL`).
    fn coord(self) -> f64 {
        match self {
            Rail::Left => -XL,
            Rail::Right => XL,
            Rail::Bottom => -YL,
            Rail::Top => YL,
        }
    }

    /// `c` in `M(p) = c - p`, the mirror of the mirrored coordinate.
    fn mirror_c(self) -> f64 {
        match self {
            Rail::Left => -2.0 * XL,
            Rail::Right => 2.0 * XL,
            Rail::Bottom => -2.0 * YL,
            Rail::Top => 2.0 * YL,
        }
    }

    /// Is a ball-centre point sitting on this rail within the rail's physical extent?
    pub fn on_segment(self, p: V2) -> bool {
        match self {
            Rail::Left | Rail::Right => p.y.abs() <= YL + 1e-9,
            Rail::Bottom | Rail::Top => p.x.abs() <= XL + 1e-9,
        }
    }
}

/// Axis-wise affine `p -> s*p + t` per coordinate: the accumulated unfolding of the table as the
/// chain is walked (each rail mirror is affine, and reflections about axis-aligned lines stay
/// independent per axis).
#[derive(Clone, Copy)]
struct Unfold {
    sx: f64,
    tx: f64,
    sy: f64,
    ty: f64,
}

impl Unfold {
    fn identity() -> Self {
        Self {
            sx: 1.0,
            tx: 0.0,
            sy: 1.0,
            ty: 0.0,
        }
    }

    fn apply(&self, p: V2) -> V2 {
        V2::new(self.sx * p.x + self.tx, self.sy * p.y + self.ty)
    }

    /// Extend by one more mirror: `g' = g ∘ M_rail`.
    fn then(&self, r: Rail) -> Self {
        match r {
            Rail::Left | Rail::Right => Self {
                sx: -self.sx,
                tx: self.sx * r.mirror_c() + self.tx,
                ..*self
            },
            Rail::Bottom | Rail::Top => Self {
                sy: -self.sy,
                ty: self.sy * r.mirror_c() + self.ty,
                ..*self
            },
        }
    }

    /// Parameter `t` along the ray `o + t*d` (unit `d`) at which it crosses the rail's *unfolded*
    /// line `x = coord` (or `y = coord`).
    fn crossing(&self, r: Rail, o: V2, d: V2) -> Option<f64> {
        let (line_pos, o_c, d_c) = if r.is_x() {
            (self.sx * r.coord() + self.tx, o.x, d.x)
        } else {
            (self.sy * r.coord() + self.ty, o.y, d.y)
        };
        if d_c.abs() < 1e-12 {
            return None;
        }
        let t = (line_pos - o_c) / d_c;
        if t < 0.0 {
            None
        } else {
            Some(t)
        }
    }
}

/// Unfolded straight-line construction for a rail chain: the image point and the bounce points
/// folded back into real table coordinates.
pub struct ChainPath {
    pub image: V2,
    pub bounces: Vec<V2>,
}

/// Build (and geometrically verify) a path from `origin` to `target` hitting `chain` in order.
///
/// The unfolding mirrors the target through the chain in reverse hit order, walks the resulting
/// straight line for the crossings, folds the crossing points back, and then verifies the
/// reflection law at every bounce with exact arithmetic. A chain that fails verification yields
/// `None` — the generator is not trusted to be right on its own.
pub fn chain_path(origin: V2, target: V2, chain: &[Rail]) -> Option<ChainPath> {
    if chain.is_empty() {
        return None;
    }
    // Accumulated unfolding: applying it to the target gives the image point the straight line
    // aims at, and to each rail line gives the line the straight line must cross.
    let mut acc = Unfold::identity();
    let mut image = target;
    for r in chain.iter().rev() {
        image = r.reflect(image);
    }
    let d = image - origin;
    let dlen = d.len();
    if dlen < 1e-9 {
        return None;
    }
    let dhat = d / dlen;

    // Walk the unfolded line, crossing each rail's carried line in order.
    let mut unfolded = Vec::with_capacity(chain.len());
    let mut t_prev = 0.0;
    for r in chain {
        let t = acc.crossing(*r, origin, dhat)?;
        if t <= t_prev + 1e-9 {
            return None;
        }
        t_prev = t;
        unfolded.push(origin + dhat * t);
        acc = acc.then(*r);
    }
    if t_prev >= dlen - 1e-9 {
        return None; // the target must lie beyond the last bounce
    }
    if acc.apply(target).dist(image) > 1e-6 {
        return None; // the incremental and reverse-order unfoldings must agree
    }

    // Fold the crossings back: bounce i lives after i-1 earlier mirrors.
    let mut bounces = Vec::with_capacity(unfolded.len());
    for (i, b) in unfolded.iter().enumerate() {
        let mut p = *b;
        for r in chain[..i].iter().rev() {
            p = r.reflect(p);
        }
        if !chain[i].on_segment(p) {
            return None;
        }
        bounces.push(p);
    }

    // Verify the reflection law numerically at every bounce, plus the final leg to the target.
    for i in 0..bounces.len() {
        let prev = if i == 0 { origin } else { bounces[i - 1] };
        let next = if i + 1 < bounces.len() {
            bounces[i + 1]
        } else {
            target
        };
        let din = bounces[i] - prev;
        let dout = next - bounces[i];
        if din.len() < 1e-9 || dout.len() < 1e-9 {
            return None;
        }
        let want = chain[i].reflect(bounces[i] + dout) - bounces[i]; // dout mirrored across the rail
        // Mirroring the outgoing direction must reproduce the incoming one (angles are preserved).
        let want_u = want.unit()?;
        let in_u = din.unit()?;
        if in_u.dot(want_u) < 0.999_999 {
            return None;
        }
    }

    Some(ChainPath { image, bounces })
}

/// The six pockets as capture discs. Simplification, stated: real pockets have jaw geometry; a
/// capture disc at the mouth centre with the WPA mouth width as its diameter stands in for it.
#[derive(Clone, Copy, Debug)]
pub struct Pocket {
    pub p: V2,
    pub radius: f64,
}

pub fn pockets() -> [Pocket; 6] {
    let rc = CORNER_MOUTH / 2.0;
    let rs = SIDE_MOUTH / 2.0;
    [
        Pocket {
            p: V2::new(HL, HW),
            radius: rc,
        },
        Pocket {
            p: V2::new(HL, -HW),
            radius: rc,
        },
        Pocket {
            p: V2::new(-HL, HW),
            radius: rc,
        },
        Pocket {
            p: V2::new(-HL, -HW),
            radius: rc,
        },
        Pocket {
            p: V2::new(0.0, HW),
            radius: rs,
        },
        Pocket {
            p: V2::new(0.0, -HW),
            radius: rs,
        },
    ]
}

/// A live object ball, in table coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Ob {
    pub id: usize,
    pub p: V2,
    pub alive: bool,
}

/// Minimum clearance margin of the segment `a`→`b` against every ball in `balls` except the ones
/// whose ids are in `exclude`: `min(distance(centre, segment)) - 2R`. Negative means blocked.
pub fn clearance(a: V2, b: V2, balls: &[Ob], exclude: &[usize]) -> f64 {
    let d = b - a;
    let dl = d.len();
    if dl < 1e-9 {
        return f64::INFINITY;
    }
    let u = d / dl;
    let mut best = f64::INFINITY;
    for ob in balls {
        if !ob.alive || exclude.contains(&ob.id) {
            continue;
        }
        let ap = ob.p - a;
        let t = ap.dot(u).clamp(0.0, dl);
        let closest = a + u * t;
        let m = ob.p.dist(closest) - BALL_D;
        if m < best {
            best = m;
        }
    }
    best
}

/// Clearance for a multi-segment path (the minimum over its legs).
pub fn path_clearance(points: &[V2], balls: &[Ob], exclude: &[usize]) -> f64 {
    let mut best = f64::INFINITY;
    for w in points.windows(2) {
        let m = clearance(w[0], w[1], balls, exclude);
        if m < best {
            best = m;
        }
    }
    best
}

/// Distance from a point to the nearest pocket mouth centre.
pub fn nearest_pocket_dist(p: V2, ps: &[Pocket]) -> f64 {
    ps.iter()
        .map(|q| q.p.dist(p))
        .fold(f64::INFINITY, f64::min)
}
