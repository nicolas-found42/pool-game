//! Execution noise (`architecture.md` §5, `ai.md` §7): the σ draws the `Session` applies to **policy
//! declarations only**. A human seat's declaration is never perturbed, and the log stores the intent,
//! never the perturbed strike — replay re-derives the same perturbation from the same stream.
//!
//! The model is `ai.md` §7's: one per-level factor scales all four axes (aim, cue speed, spin a/b,
//! elevation), aimed at the measured make-rate curve. The aim σ is the measured half; the other three
//! axes are the assumption the spec labels provisional, with speed at 2.5 × aim.

use pool_rng::SplitMix64;
use pool_rules::{ShotDeclaration, Spin, Vec2};

use crate::log::DifficultyLevel;

/// The aim σ (rad) of each level — `ai.md` §7's measured curve and the level assignment derived from
/// it: Pro 0.5 mrad, Advanced 1–2 mrad, Intermediate 4 mrad, Beginner 8–16 mrad.
///
/// The two intervals are resolved to their upper end (Advanced 2 mrad, Beginner 16 mrad): the spec
/// fixes the interval, not the point, and the upper end is the one that keeps the four levels ordered
/// and distinct. Reported to the spec's owner rather than hidden here.
#[must_use]
pub fn aim_sigma_rad(level: DifficultyLevel) -> f64 {
    match level {
        DifficultyLevel::Beginner => 16.0e-3,
        DifficultyLevel::Intermediate => 4.0e-3,
        DifficultyLevel::Advanced => 2.0e-3,
        DifficultyLevel::Pro => 0.5e-3,
    }
}

/// The speed axis's σ as a multiple of the aim σ, as a **relative** perturbation of the declared
/// speed (`ai-constants.md`: "speed at 2.5 × aim σ").
pub const SPEED_SIGMA_FACTOR: f64 = 2.5;

/// The execution-noise stream: one perturbation per policy declaration, in log order.
#[derive(Debug, Clone)]
pub struct Noise {
    stream: SplitMix64,
}

impl Noise {
    /// A stream seeded with the match's `noise_seed` (`architecture.md` §5).
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            stream: SplitMix64::new(seed),
        }
    }

    /// The declaration a policy seat actually executes: its intent perturbed by the level's σ.
    ///
    /// The axes are drawn in a fixed order — `(aim, speed)`, `(spin a, spin b)`, `(elevation,
    /// reserved)` — so the stream's consumption is part of the replay contract. The `reserved` value
    /// is unused today: the order is append-only, so an axis added later takes it without shifting
    /// the draws an already-recorded golden was made with.
    ///
    /// The spin pair is projected back onto the miscue envelope when the perturbation would leave it
    /// (`|(a, b)| ≤ 1` is the declaration's contract, `architecture.md` §6), so a policy declaration
    /// stays a playable strike; the elevation is not clamped, because its boundary is the strike's
    /// own (`|e| < π/2`) and no σ of `ai.md` §7 comes near it. The aim's rotation is the pinned `libm`
    /// escape hatch of `architecture.md` §3 — a declaration conversion, not part of the evolution.
    #[must_use]
    pub fn perturb(
        &mut self,
        level: DifficultyLevel,
        declaration: &ShotDeclaration,
    ) -> ShotDeclaration {
        let sigma = aim_sigma_rad(level);
        let (aim_draw, speed_draw) = self.stream.gaussian_pair();
        let (spin_a_draw, spin_b_draw) = self.stream.gaussian_pair();
        let (elevation_draw, _reserved) = self.stream.gaussian_pair();

        let angle = sigma * aim_draw;
        let (sin, cos) = (libm::sin(angle), libm::cos(angle));
        let aim = Vec2 {
            x: declaration.aim.x * cos - declaration.aim.y * sin,
            y: declaration.aim.x * sin + declaration.aim.y * cos,
        };

        let speed = declaration.speed * (1.0 + SPEED_SIGMA_FACTOR * sigma * speed_draw);

        let (a, b) = (
            declaration.spin.a + sigma * spin_a_draw,
            declaration.spin.b + sigma * spin_b_draw,
        );
        let magnitude = (a * a + b * b).sqrt();
        let scale = if magnitude > 1.0 {
            1.0 / magnitude
        } else {
            1.0
        };
        let spin = Spin {
            a: a * scale,
            b: b * scale,
        };

        let elevation = declaration.elevation + sigma * elevation_draw;
        ShotDeclaration {
            call: declaration.call.clone(),
            aim,
            speed,
            spin,
            elevation,
        }
    }
}
