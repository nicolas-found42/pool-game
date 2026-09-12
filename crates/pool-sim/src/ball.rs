//! Ball state and motion modes (`physics.md` §1/§3).

use serde::{Deserialize, Serialize};

/// The motion mode a ball is in between events (`physics.md` §3.1, §3.7). The mode is part of the
/// hashed state (`architecture.md` §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionMode {
    /// Ballistic: gravity only, no cloth contact (`physics.md` §3.7).
    Airborne,
    /// The contact point slips; Coulomb sliding deceleration (`physics.md` §3.1).
    Sliding,
    /// Rolling without slipping; linear rolling resistance (`physics.md` §3.1).
    Rolling,
    /// Translating as good as not, but still spinning about the vertical: spin decay only (§3.1).
    Spinning,
    /// Below the sleep thresholds; the state snaps here (`physics.md` §5).
    Stationary,
}

/// One ball's state: position, velocity, angular velocity, mode, and the two terminal flags.
///
/// Units: mm, mm/s, rad/s. The cue ball is index 0; object balls are 1..=15 in ball-number order
/// (`architecture.md` §11's canonical order).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BallState {
    /// Centre position (mm) in the table frame: x long, y across, z up.
    pub pos_mm: [f64; 3],
    /// Linear velocity (mm/s).
    pub vel_mm_s: [f64; 3],
    /// Angular velocity (rad/s).
    pub spin_rad_s: [f64; 3],
    /// The mode in force between the surrounding events.
    pub mode: MotionMode,
    /// True once the ball has crossed into a pocket mouth (`physics.md` §3.5).
    pub pocketed: bool,
    /// True once the ball has left the playing surface (`physics.md` §3.4's ceiling rule).
    pub off_table: bool,
}

impl BallState {
    /// A ball at rest at `pos_mm`, on the cloth.
    #[must_use]
    pub const fn at_rest(pos_mm: [f64; 3]) -> Self {
        Self {
            pos_mm,
            vel_mm_s: [0.0; 3],
            spin_rad_s: [0.0; 3],
            mode: MotionMode::Stationary,
            pocketed: false,
            off_table: false,
        }
    }

    /// Whether the ball is in play: on the surface and not pocketed.
    #[must_use]
    pub const fn in_play(&self) -> bool {
        !self.pocketed && !self.off_table
    }

    /// Speed (mm/s), the magnitude of the linear velocity.
    #[must_use]
    pub fn speed_mm_s(&self) -> f64 {
        let [vx, vy, vz] = self.vel_mm_s;
        (vx * vx + vy * vy + vz * vz).sqrt()
    }

    /// Angular speed (rad/s).
    #[must_use]
    pub fn spin_rate_rad_s(&self) -> f64 {
        let [wx, wy, wz] = self.spin_rad_s;
        (wx * wx + wy * wy + wz * wz).sqrt()
    }

    /// Total mechanical energy (J): `½mv² + ½Iω² + mg(z − R)`.
    ///
    /// The sim's units are mm and g, so the sum is in `g·mm²/s²` and the `1e9` converts it to joules.
    #[must_use]
    pub fn total_energy_j(&self) -> f64 {
        let [vx, vy, vz] = self.vel_mm_s;
        let [wx, wy, wz] = self.spin_rad_s;
        let linear = 0.5 * crate::constants::BALL_MASS_G * (vx * vx + vy * vy + vz * vz);
        let angular = 0.5 * crate::constants::BALL_INERTIA_G_MM2 * (wx * wx + wy * wy + wz * wz);
        let height = crate::constants::BALL_MASS_G
            * crate::constants::GRAVITY_MM_S2
            * (self.pos_mm[2] - crate::constants::BALL_RADIUS_MM);
        (linear + angular + height) / 1e9
    }
}
