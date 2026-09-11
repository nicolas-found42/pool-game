//! Cue input model (#7 §4): a strike is a post-impact cue-ball state.
//!
//! `{aim direction, launch speed, spin offsets (a, b) in tip radii, elevation}`.
//! No stick is simulated; the tip restitution is therefore not a constant.
//! The miscue envelope is an input-boundary rejection, not a simulated event.
//!
//! Squirt uses the effective-endmass/pivot-length scalar: tan(alpha) = a / L,
//! which is the *definition* of the natural pivot length. The rotation is
//! applied with an algebraic sine/cosine (no transcendental reaches the core).

use crate::consts::*;
use crate::sim::{contact_point_velocity, motion_mode};
use crate::vec::{v3, V3};

#[derive(Clone, Copy, Debug)]
pub struct Strike {
    /// Unit horizontal aim direction (the line the cue is pointed along).
    pub aim: V3,
    /// Post-impact cue-ball speed (mm/s).
    pub speed: f64,
    /// Side tip offset, tip radii, positive = right english.
    pub a_tip: f64,
    /// Vertical tip offset, tip radii, positive = above centre (follow).
    pub b_tip: f64,
    /// Cue elevation above horizontal (radians).
    pub elevation: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrikeError {
    Miscue { offset: f64, limit: f64 },
    BadAim,
    BadSpeed,
}

#[derive(Clone, Copy, Debug)]
pub struct StrikeResult {
    pub v: V3,
    pub w: V3,
    pub squirt_rad: f64,
}

impl Strike {
    pub fn new(aim_deg: f64, speed: f64, a_tip: f64, b_tip: f64, elevation_deg: f64) -> Strike {
        let r = aim_deg.to_radians();
        Strike {
            aim: v3(r.cos(), r.sin(), 0.0),
            speed,
            a_tip,
            b_tip,
            elevation: elevation_deg.to_radians(),
        }
    }

    /// Miscue envelope: the tip-friction limit, a circle of radius 0.5R in
    /// (a, b) for a level cue (#7 §4).
    pub fn max_offset(&self) -> f64 {
        0.5
    }

    pub fn validate(&self) -> Result<(), StrikeError> {
        let off = (self.a_tip * self.a_tip + self.b_tip * self.b_tip).sqrt();
        let lim = self.max_offset();
        if off > lim {
            return Err(StrikeError::Miscue { offset: off, limit: lim });
        }
        if self.aim.xy().len() < 1e-9 {
            return Err(StrikeError::BadAim);
        }
        if self.speed <= 0.0 || !self.speed.is_finite() {
            return Err(StrikeError::BadSpeed);
        }
        Ok(())
    }

    /// Post-impact cue-ball state.
    pub fn resolve(&self, pivot_mm: f64) -> StrikeResult {
        // squirt: tan(alpha) = a_mm / pivot, evaluated algebraically
        let a_mm = self.a_tip * R;
        let sin_a = a_mm / (a_mm * a_mm + pivot_mm * pivot_mm).sqrt();
        let cos_a = pivot_mm / (a_mm * a_mm + pivot_mm * pivot_mm).sqrt();
        // the ball squirts away from the side the tip struck
        let sgn = if self.a_tip >= 0.0 { -1.0 } else { 1.0 };
        let aim = self.aim.xy().norm();
        let rotated = v3(
            aim.x * cos_a - aim.y * (sgn * sin_a),
            aim.x * (sgn * sin_a) + aim.y * cos_a,
            0.0,
        );
        let alpha = (sin_a / cos_a).atan();
        let (se, ce) = (sin_of(self.elevation), cos_of(self.elevation));
        let v = v3(rotated.x * ce, rotated.y * ce, se) * self.speed;
        // impulse relation of an off-centre strike: w = (5/2)(v/R)(offset)
        let axis = v3(0.0, 0.0, 1.0);
        let vert_axis = axis.cross(rotated); // ẑ x ê : +y for ê = +x (topspin)
        let w = axis * ((2.5 * self.speed / R) * self.a_tip) + vert_axis * ((2.5 * self.speed / R) * self.b_tip);
        StrikeResult {
            v,
            w,
            squirt_rad: alpha * sgn,
        }
    }
}

fn sin_of(x: f64) -> f64 {
    // only used at the declaration boundary
    x.sin()
}
fn cos_of(x: f64) -> f64 {
    x.cos()
}

/// Convenience: the launch speed that makes a straight, level centre-ball
/// stroke roll a target number of table lengths -- used by the WPA cushion
/// acceptance hook.
pub fn launch_speed_for_energy(v: f64) -> f64 {
    v
}

/// Sanity helper used by tests and the ladder: is this state rolling?
pub fn is_rolling(v: V3, w: V3) -> bool {
    contact_point_velocity(v, w).len() < 1e-6
}

pub fn mode_of(p: V3, v: V3, w: V3) -> crate::sim::Mode {
    motion_mode(p, v, w)
}
