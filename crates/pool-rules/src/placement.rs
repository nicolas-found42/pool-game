//! Ball-in-hand placement (`rules.md` §6, `architecture.md` §9): the geometry predicates and the
//! 3.10/3.11 composition.
//!
//! The predicates are composed from the simulation's published constants and follow `Sim::place_cue`
//! exactly — finite, the centre on the playing surface, and at least one diameter of clearance to every
//! other ball, with contact (inside the shared slop) accepted; `tests/placement_spot.rs` asserts the
//! agreement. The machine never reads ball state directly; it hands the ball array in.

use pool_sim::constants::{
    BALL_DIAMETER_MM, BALL_RADIUS_MM, CONTACT_SLOP_MM, HALF_LEN_MM, HALF_WIDTH_MM, HEAD_STRING_X_MM,
};

use crate::facts::PreBall;
use crate::vocab::{PlacementDomain, Vec2};

/// Why a submitted placement was rejected (`rules.md` §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlacementFault {
    /// The coordinates are not finite.
    NotFinite,
    /// The ball's centre would not sit on the playing surface.
    OutsideSurface {
        /// The rejected position (mm).
        pos_mm: [f64; 2],
    },
    /// The position is not inside the placement domain (2.13's reading of the head string).
    OutsideDomain {
        /// The domain that was demanded.
        domain: PlacementDomain,
    },
    /// The cue ball would overlap a ball.
    Overlaps {
        /// The ball it would overlap.
        ball: u8,
        /// Their centre distance minus one diameter (mm): negative is an overlap.
        gap_mm: f64,
    },
}

impl From<pool_sim::PlacementError> for PlacementFault {
    fn from(error: pool_sim::PlacementError) -> Self {
        match error {
            pool_sim::PlacementError::NotFinite => Self::NotFinite,
            pool_sim::PlacementError::OutsideSurface { pos_mm } => Self::OutsideSurface { pos_mm },
            pool_sim::PlacementError::Overlaps { ball, gap_mm } => Self::Overlaps { ball, gap_mm },
        }
    }
}

/// Whether a ball's centre sits on the playing surface (`Sim::place_cue`'s containment).
#[must_use]
pub fn on_surface(pos: Vec2) -> bool {
    pos.x.is_finite()
        && pos.y.is_finite()
        && pos.x.abs() <= HALF_LEN_MM - BALL_RADIUS_MM
        && pos.y.abs() <= HALF_WIDTH_MM - BALL_RADIUS_MM
}

/// 2.13's reading: a ball *on* the head string is not above it (`rules.md` §6).
#[must_use]
pub fn above_head_string(pos: Vec2) -> bool {
    pos.x < HEAD_STRING_X_MM
}

/// Whether `pos` is inside `domain`.
#[must_use]
pub fn in_domain(domain: PlacementDomain, pos: Vec2) -> bool {
    match domain {
        PlacementDomain::AboveHeadString => above_head_string(pos),
        PlacementDomain::Anywhere => true,
    }
}

/// The first ball at `pos` would overlap, with the gap it would leave (negative is an overlap). The
/// cue ball is never one of them: it is the ball being placed.
///
/// Overlap is *beyond* the contact slop, exactly as [`pool_sim::Sim::place_cue`] reads it: an
/// exact-contact position re-measured by a different route can land a few ulps short of a diameter,
/// and contact is admissible (`rules.md` §4's spot, `tests/placement_spot.rs`'s agreement).
#[must_use]
pub fn overlap(pos: Vec2, balls: &[PreBall]) -> Option<(u8, f64)> {
    balls
        .iter()
        .filter(|ball| ball.id != 0)
        .map(|ball| {
            let dx = pos.x - ball.x_mm;
            let dy = pos.y - ball.y_mm;
            ((dx * dx + dy * dy).sqrt() - BALL_DIAMETER_MM, ball.id)
        })
        .find(|(gap, _)| *gap < -CONTACT_SLOP_MM)
        .map(|(gap, id)| (id, gap))
}

/// Validate a submitted placement (`rules.md` §6): in-domain, on the surface, clear of every ball.
///
/// 3.10 is enforced here, at the input boundary, so a placement the machine accepted can never be a
/// 3.10 foul.
pub fn validate(
    domain: PlacementDomain,
    pos: Vec2,
    balls: &[PreBall],
) -> Result<(), PlacementFault> {
    if !pos.x.is_finite() || !pos.y.is_finite() {
        return Err(PlacementFault::NotFinite);
    }
    if !in_domain(domain, pos) {
        return Err(PlacementFault::OutsideDomain { domain });
    }
    if !on_surface(pos) {
        return Err(PlacementFault::OutsideSurface {
            pos_mm: [pos.x, pos.y],
        });
    }
    if let Some((ball, gap_mm)) = overlap(pos, balls) {
        return Err(PlacementFault::Overlaps { ball, gap_mm });
    }
    Ok(())
}

/// Whether every one of `legal` is above the head string: 1.6 ¶2's precondition (`rules.md` §6).
#[must_use]
pub fn all_above_head_string(balls: &[PreBall], legal: &[u8]) -> bool {
    !legal.is_empty()
        && legal.iter().all(|ball| {
            balls
                .iter()
                .find(|state| state.id == *ball)
                .is_some_and(|state| {
                    above_head_string(Vec2 {
                        x: state.x_mm,
                        y: state.y_mm,
                    })
                })
        })
}

/// The legal ball nearest the head string: the one 1.6 ¶2 spots.
///
/// Ties go to the lower ball number: the rule breaks them "by his designation" and no entry kind in
/// the input log carries one (`architecture.md` §6's `spot_request` has no fields).
#[must_use]
pub fn nearest_to_head_string(balls: &[PreBall], legal: &[u8]) -> Option<u8> {
    balls
        .iter()
        .filter(|state| legal.contains(&state.id))
        .min_by(|a, b| b.x_mm.total_cmp(&a.x_mm).then_with(|| a.id.cmp(&b.id)))
        .map(|state| state.id)
}
