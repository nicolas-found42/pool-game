//! The cue input model (`physics.md` §3.6/§4): a strike is a post-impact cue-ball state.
//!
//! No stick is simulated, so the tip restitution is not a constant; squirt ships as one scalar (the
//! cue's pivot length) with its rotation evaluated algebraically. The miscue envelope is an
//! input-boundary rejection (`architecture.md` §4's error policy), never a simulated event.

use crate::constants::{BALL_RADIUS_MM, TIP_FRICTION_MU};
use crate::math::{V3, v3};

/// A shot declaration: aim, launch speed, spin offsets, elevation — the `input-log.schema.json`
/// declaration's physics half (`physics.md` §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrikeDecl {
    /// The horizontal aim direction; normalised at the boundary, so it need not be a unit vector.
    pub aim: [f64; 2],
    /// The post-impact cue-ball launch speed (mm/s).
    pub speed_mm_s: f64,
    /// Tip offsets as fractions of the miscue envelope — `1.0` is the limit — with `a > 0` the
    /// shooter's right and `b > 0` above centre (`physics.md` §4).
    pub spin: [f64; 2],
    /// The cue's elevation above horizontal (radians).
    pub elevation_rad: f64,
}

/// A rejected declaration. Input-boundary problems are `Result`s; internal invariants are asserts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrikeError {
    /// The tip offset is past the miscue envelope: `|(a, b)| > 1`.
    Miscue {
        /// The offset's norm, in envelope fractions.
        norm: f64,
        /// The envelope's limit: `1.0`.
        limit: f64,
    },
    /// The aim is the zero vector, or not finite.
    BadAim,
    /// The speed is not positive and finite.
    BadSpeed,
    /// A spin offset is not finite.
    BadSpin,
    /// The elevation is not finite, or is at least `π/2` off the horizontal.
    BadElevation,
}

/// The post-impact cue-ball state (`physics.md` §3.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrikeOutcome {
    /// The cue ball's launch velocity (mm/s).
    pub v_mm_s: V3,
    /// The cue ball's launch angular velocity (rad/s).
    pub w_rad_s: V3,
}

/// The miscue envelope's radius (mm): `ρ_max = R·μ/√(1+μ²)` (`physics.md` §3.6) — 14.70 mm at
/// μ = 0.6. A function rather than a `const` because `sqrt` is not const-callable.
#[must_use]
pub fn miscue_envelope_mm() -> f64 {
    BALL_RADIUS_MM * TIP_FRICTION_MU / (1.0 + TIP_FRICTION_MU * TIP_FRICTION_MU).sqrt()
}

impl StrikeDecl {
    /// Check the declaration against the miscue envelope and the input domains.
    ///
    /// # Errors
    ///
    /// Returns [`StrikeError::BadSpin`] for a non-finite spin offset, [`StrikeError::Miscue`] when the
    /// offsets leave the unit disc, [`StrikeError::BadAim`] for a non-finite or zero aim,
    /// [`StrikeError::BadSpeed`] for a non-finite or non-positive speed, and
    /// [`StrikeError::BadElevation`] for a non-finite elevation or one at least `π/2` off the
    /// horizontal.
    pub fn validate(&self) -> Result<(), StrikeError> {
        let [a, b] = self.spin;
        if !a.is_finite() || !b.is_finite() {
            return Err(StrikeError::BadSpin);
        }
        let norm = (a * a + b * b).sqrt();
        if norm > 1.0 {
            return Err(StrikeError::Miscue { norm, limit: 1.0 });
        }
        let [ax, ay] = self.aim;
        if !ax.is_finite() || !ay.is_finite() || (ax == 0.0 && ay == 0.0) {
            return Err(StrikeError::BadAim);
        }
        if !self.speed_mm_s.is_finite() || self.speed_mm_s <= 0.0 {
            return Err(StrikeError::BadSpeed);
        }
        if !self.elevation_rad.is_finite()
            || self.elevation_rad.abs() >= std::f64::consts::FRAC_PI_2
        {
            return Err(StrikeError::BadElevation);
        }
        Ok(())
    }

    /// Resolve the declaration into the cue ball's post-impact state (`physics.md` §3.6/§4).
    ///
    /// The spin offsets are converted once, at this boundary: `offset_mm = ρ_max · value`. Squirt is
    /// the pivot-length rotation `sin α = a/√(a² + L²)`, `cos α = L/√(a² + L²)` — algebraic, no
    /// transcendental. The elevation's `sin`/`cos` are the pinned `libm` escape hatch of
    /// `architecture.md` §3: they are the declaration's conversion, not part of the evolution.
    ///
    /// # Errors
    ///
    /// Returns the [`StrikeError`] of [`StrikeDecl::validate`] when the declaration is out of domain.
    pub fn resolve(&self, pivot_mm: f64) -> Result<StrikeOutcome, StrikeError> {
        self.validate()?;
        let aim = v3(self.aim[0], self.aim[1], 0.0).norm();

        let envelope = miscue_envelope_mm();
        let offset_a_mm = self.spin[0] * envelope;
        let offset_b_mm = self.spin[1] * envelope;

        // Squirt: the ball leaves away from the side the tip struck.
        let radius = (offset_a_mm * offset_a_mm + pivot_mm * pivot_mm).sqrt();
        let sin_squirt = offset_a_mm / radius;
        let cos_squirt = pivot_mm / radius;
        let sign = if offset_a_mm >= 0.0 { -1.0 } else { 1.0 };
        let aim = v3(
            aim.x * cos_squirt - aim.y * (sign * sin_squirt),
            aim.x * (sign * sin_squirt) + aim.y * cos_squirt,
            0.0,
        );

        let (sin_elev, cos_elev) = (libm::sin(self.elevation_rad), libm::cos(self.elevation_rad));
        let v_mm_s = v3(aim.x * cos_elev, aim.y * cos_elev, sin_elev) * self.speed_mm_s;

        // Off-centre impulse relation: ω = (5/2)·v·d/R², no stick simulated.
        let spin_scale = 2.5 * self.speed_mm_s / (BALL_RADIUS_MM * BALL_RADIUS_MM);
        let vertical_axis = v3(0.0, 0.0, 1.0).cross(aim); // ẑ × ê: +y when the aim is +x (follow)
        let w_rad_s = v3(0.0, 0.0, 1.0) * (spin_scale * offset_a_mm)
            + vertical_axis * (spin_scale * offset_b_mm);

        Ok(StrikeOutcome { v_mm_s, w_rad_s })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(aim: [f64; 2], speed: f64, spin: [f64; 2], elevation: f64) -> StrikeDecl {
        StrikeDecl {
            aim,
            speed_mm_s: speed,
            spin,
            elevation_rad: elevation,
        }
    }

    #[test]
    fn a_centre_strike_launches_along_the_aim_with_no_spin() {
        let outcome = decl([1.0, 0.0], 2000.0, [0.0, 0.0], 0.0)
            .resolve(350.0)
            .expect("a centre, level strike is legal");
        assert_eq!(outcome.v_mm_s, v3(2000.0, 0.0, 0.0));
        assert_eq!(outcome.w_rad_s, V3::ZERO);
    }

    #[test]
    fn the_envelope_admits_its_limit_and_rejects_past_it() {
        let limit = decl([1.0, 0.0], 1000.0, [1.0, 0.0], 0.0).resolve(350.0);
        assert!(limit.is_ok(), "the envelope's own limit is a legal strike");
        // A quarter millimetre past the envelope's radius is past the limit.
        let past = 1.0 + 0.25 / miscue_envelope_mm();
        let rejected = decl([1.0, 0.0], 1000.0, [past, 0.0], 0.0).resolve(350.0);
        assert!(matches!(
            rejected,
            Err(StrikeError::Miscue { limit: 1.0, .. })
        ));
    }

    #[test]
    fn offsets_scale_by_the_envelope_not_by_the_radius() {
        let envelope = miscue_envelope_mm();
        assert!(
            (envelope - 14.70).abs() < 0.01,
            "the envelope is 14.70 mm at μ = 0.6, not {envelope}"
        );
        let outcome = decl([1.0, 0.0], 1000.0, [0.5, 0.0], 0.0)
            .resolve(350.0)
            .expect("a half-envelope offset is legal");
        // ω_z = (5/2)·v·d/R² with d = 0.5·14.70 mm.
        let expected = 2.5 * 1000.0 * (0.5 * envelope) / (BALL_RADIUS_MM * BALL_RADIUS_MM);
        assert!((outcome.w_rad_s.z - expected).abs() < 1e-12);
    }

    #[test]
    fn bad_inputs_are_rejected_at_the_boundary() {
        assert_eq!(
            decl([0.0, 0.0], 1000.0, [0.0, 0.0], 0.0).resolve(350.0),
            Err(StrikeError::BadAim)
        );
        assert_eq!(
            decl([1.0, 0.0], 0.0, [0.0, 0.0], 0.0).resolve(350.0),
            Err(StrikeError::BadSpeed)
        );
        assert_eq!(
            decl([1.0, 0.0], 1000.0, [0.0, 0.0], std::f64::consts::FRAC_PI_2).resolve(350.0),
            Err(StrikeError::BadElevation)
        );
        assert_eq!(
            decl([1.0, 0.0], 1000.0, [f64::NAN, 0.0], 0.0).resolve(350.0),
            Err(StrikeError::BadSpin)
        );
    }
}
