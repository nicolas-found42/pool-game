//! The spot search (`rules.md` §4's four clauses of WPA 1.5, `architecture.md` §9).
//!
//! The *ordering* is rule vocabulary, so it lives here; the geometry it orders is answered by the
//! simulation's published frame (the foot spot, the long string, the ball radius) and by the ball array
//! the caller hands in. The search is total and deterministic: it walks the long string in the
//! direction 1.5's clauses demand, and the first admissible point it reaches is the spot.

use pool_sim::constants::{
    BALL_DIAMETER_MM, BALL_RADIUS_MM, FOOT_SPOT_X_MM, FROZEN_GAP_MM, HALF_LEN_MM,
};

use crate::facts::PreBall;

/// The minimum gap the search keeps between the spotted ball and the cue ball: `rules.md` §4's δ,
/// pinned to the simulation's contact tolerance (`physics.md` §5), so a spotted ball is never within
/// the frozen threshold of the cue.
pub const CUE_SEPARATION_MM: f64 = FROZEN_GAP_MM;

/// Where a ball was spotted (`rules.md` §4): the ball and the position 1.5's algorithm gave it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// The spotted ball.
    pub ball: u8,
    /// The position (mm, table frame): on the long string, so `y` is always 0.
    pub pos_mm: [f64; 2],
}

/// One obstacle's blocked stretch of the long string: the `x` values at which a ball centred at
/// `(x, 0)` would stand closer than `radius` to the obstacle. The edges are contact, not overlap, and
/// contact is admissible — 1.5's clause 2 asks for it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Blocked {
    left: f64,
    right: f64,
}

impl Blocked {
    /// The stretch a ball at `centre` blocks for a spot ball that must keep `radius` away.
    fn of(centre: [f64; 2], radius: f64) -> Option<Self> {
        let [cx, cy] = centre;
        let half = radius * radius - cy * cy;
        if half <= 0.0 {
            return None;
        }
        let half = half.sqrt();
        Some(Self {
            left: cx - half,
            right: cx + half,
        })
    }

    /// Whether `x` is strictly inside the stretch: the spot ball's centre would stand closer than
    /// `radius`, which is an overlap. The edges are contact and are admissible.
    fn overlaps(self, x: f64) -> bool {
        self.left < x && x < self.right
    }
}

/// The long string's blocked stretches, read from the ball array on the table.
struct Obstacles {
    balls: Vec<Blocked>,
    cue: Option<Blocked>,
}

impl Obstacles {
    /// The stretches `ball` may not be spotted into: every other ball by a diameter, and the cue ball
    /// by a diameter plus the separation.
    fn of(ball: u8, balls: &[PreBall]) -> Self {
        Self {
            balls: balls
                .iter()
                .filter(|state| state.id != ball && state.id != 0)
                .filter_map(|state| Blocked::of([state.x_mm, state.y_mm], BALL_DIAMETER_MM))
                .collect(),
            cue: balls.iter().find(|state| state.id == 0).and_then(|state| {
                Blocked::of(
                    [state.x_mm, state.y_mm],
                    BALL_DIAMETER_MM + CUE_SEPARATION_MM,
                )
            }),
        }
    }

    /// Whether a ball centred at `(x, 0)` may stand there: in contact with everything is fine, inside
    /// anything is not.
    fn admissible(&self, x: f64) -> bool {
        !self.balls.iter().any(|blocked| blocked.overlaps(x))
            && !self.cue.is_some_and(|blocked| blocked.overlaps(x))
    }

    /// The nearest admissible `x` above `x`, toward the foot rail, if `x` overlaps something: the far
    /// edge of the stretch it is in, which is the contact position 1.5's clause 2 prefers.
    fn advance(&self, x: f64) -> Option<f64> {
        self.edges(x, |blocked| blocked.right, |current, edge| edge > current)
    }

    /// The nearest admissible `x` below `x`, toward the head rail.
    fn retreat(&self, x: f64) -> Option<f64> {
        self.edges(x, |blocked| blocked.left, |current, edge| edge < current)
    }

    /// The extreme edge, over every stretch overlapping `x`, that `better` prefers.
    fn edges(
        &self,
        x: f64,
        edge: impl Fn(&Blocked) -> f64,
        better: impl Fn(f64, f64) -> bool,
    ) -> Option<f64> {
        let mut next: Option<f64> = None;
        let mut push = |blocked: &Blocked| {
            let candidate = edge(blocked);
            if next.is_none_or(|current| better(current, candidate)) {
                next = Some(candidate);
            }
        };
        for blocked in &self.balls {
            if blocked.overlaps(x) {
                push(blocked);
            }
        }
        if let Some(cue) = &self.cue
            && cue.overlaps(x)
        {
            push(cue);
        }
        next
    }
}

/// The 1.5 spot position for `ball` (`rules.md` §4): a total, deterministic algorithm over the long
/// string.
///
/// 1. the foot-spot position, if a ball can stand there without overlapping anything;
/// 2. otherwise the position in contact with the interfering ball, as close to the foot spot as
///    possible;
/// 3. never within [`CUE_SEPARATION_MM`] of the cue ball — where two and three collide, the
///    separation wins and the next admissible position is taken;
/// 4. and if the whole stretch from the foot spot towards the foot rail is blocked, the position above
///    the foot spot as close to it as possible.
///
/// `balls` is the ball array on the table; the ball being spotted is off it and is never an obstacle.
#[must_use]
pub fn spot_position(ball: u8, balls: &[PreBall]) -> [f64; 2] {
    let obstacles = Obstacles::of(ball, balls);

    // Clauses 1–3: from the foot spot toward the foot rail.
    let mut x = FOOT_SPOT_X_MM;
    while let Some(next) = obstacles.advance(x) {
        if next > HALF_LEN_MM - BALL_RADIUS_MM {
            break;
        }
        x = next;
    }
    if obstacles.admissible(x) {
        return [x, 0.0];
    }

    // Clause 4: the stretch below the foot spot is blocked; the nearest position above the foot spot.
    let mut x = FOOT_SPOT_X_MM;
    while let Some(next) = obstacles.retreat(x) {
        if next < -(HALF_LEN_MM - BALL_RADIUS_MM) {
            break;
        }
        x = next;
    }
    debug_assert!(
        obstacles.admissible(x),
        "no admissible spot position on the long string: ball {ball} has nowhere to go"
    );
    [x, 0.0]
}
