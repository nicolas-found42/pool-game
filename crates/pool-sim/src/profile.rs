//! Interaction coefficients: the profile record and the speed-dependent ball–ball friction table
//! (`physics.md` §5).

use serde::{Deserialize, Serialize};

use crate::constants::GRAVITY_MM_S2;

/// The default profile's cushion normal-channel coefficient (`physics.md` §3.3: the measured
/// retention `e_c` = 0.70 corresponds to `e_n` ≈ 0.78).
pub const DEFAULT_E_N: f64 = 0.78;
/// The default profile's cushion friction (`physics.md` §3.3: weakly identified, bounded below).
pub const DEFAULT_MU_C: f64 = 0.20;
/// The default profile's vertical recovery (`physics.md` §3.4/§5: carried, not fitted).
pub const DEFAULT_E_SLATE: f64 = 0.60;
/// The default profile's cloth sliding friction (`physics.md` §5: not identifiable from the curves).
pub const DEFAULT_MU_S: f64 = 0.20;
/// The default profile's rolling resistance (`physics.md` §5).
pub const DEFAULT_MU_R: f64 = 0.010;
/// The default profile's spin decay (rad/s²).
pub const DEFAULT_SPIN_DECAY_RAD_S2: f64 = 10.0;
/// The default profile's ball–ball restitution (`physics.md` §5).
pub const DEFAULT_E_B: f64 = 0.95;
/// The default profile's cue pivot length (mm), the squirt scalar (`physics.md` §3.6).
pub const DEFAULT_PIVOT_MM: f64 = 350.0;

/// The speed-dependent ball–ball friction `μb(v)`, shipped as a piecewise-linear table over a 25 mm/s
/// step (`physics.md` §3.2).
///
/// The table is built once from the fitted closed form with the pinned `libm::exp`
/// (`architecture.md` §3's escape hatch); the simulation core only ever evaluates the table with
/// `+ − × ÷`.
#[derive(Debug, Clone, PartialEq)]
pub struct MuB {
    /// Fit coefficient `a`.
    pub a: f64,
    /// Fit coefficient `b`.
    pub b: f64,
    /// Fit coefficient `c` (per m/s).
    pub c: f64,
    /// `(speed mm/s, μ)` pairs, ascending, 25 mm/s apart, `0..=12000`.
    table: Vec<(f64, f64)>,
}

/// `MuB`'s record shape: the fitted coefficients, never the derived table.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MuBRecord {
    a: f64,
    b: f64,
    c: f64,
}

impl Serialize for MuB {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MuBRecord {
            a: self.a,
            b: self.b,
            c: self.c,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MuB {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = MuBRecord::deserialize(deserializer)?;
        Ok(Self::new(record.a, record.b, record.c))
    }
}

/// The table's step (mm/s).
pub const MU_B_STEP_MM_S: f64 = 25.0;
/// The table's upper bound (mm/s).
pub const MU_B_MAX_MM_S: f64 = 12_000.0;

impl MuB {
    /// Tabulate `μb(v) = a + b·e^(−c·v)` on `0..=12000` mm/s with a 25 mm/s step.
    #[must_use]
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        let mut table = Vec::new();
        let mut v = 0.0_f64;
        while v <= MU_B_MAX_MM_S {
            table.push((v, a + b * libm::exp(-c * (v / 1000.0))));
            v += MU_B_STEP_MM_S;
        }
        Self { a, b, c, table }
    }

    /// The fitted closed form; used to build and to validate the table, never in the core.
    #[must_use]
    pub fn exact(&self, v_mm_s: f64) -> f64 {
        self.a + self.b * libm::exp(-self.c * (v_mm_s / 1000.0))
    }

    /// The table lookup the core uses: piecewise linear, clamped at both ends.
    #[must_use]
    pub fn eval(&self, v_mm_s: f64) -> f64 {
        if v_mm_s <= 0.0 {
            return self.table[0].1;
        }
        let last = self.table.len() - 1;
        let idx = (v_mm_s / MU_B_STEP_MM_S) as usize;
        if idx >= last {
            return self.table[last].1;
        }
        let (v0, m0) = self.table[idx];
        let (v1, m1) = self.table[idx + 1];
        m0 + (m1 - m0) * ((v_mm_s - v0) / (v1 - v0))
    }

    /// The table's worst relative error against the closed form, over a 1 mm/s sweep.
    #[must_use]
    pub fn max_relative_error(&self) -> f64 {
        let mut worst: f64 = 0.0;
        let mut v = 0.0_f64;
        while v <= MU_B_MAX_MM_S {
            let exact = self.exact(v);
            let rel = ((self.eval(v) - exact) / exact).abs();
            if rel > worst {
                worst = rel;
            }
            v += 1.0;
        }
        worst
    }
}

/// A profile: the named record of values, provenance, and condition metadata that makes every fit
/// reproducible (`physics.md` §5). The record lives at `config/profiles/<id>.json`
/// (`architecture.md` §12); the crate parses it, the binaries read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Profile id, as it appears in the input-log header and `config/profiles/<id>.json`.
    pub id: String,
    /// Where the values came from and what they assume — the record's own provenance.
    pub provenance: Provenance,
    /// Condition metadata: the table, cloth, and ball set the values describe.
    pub conditions: Conditions,
    /// Cloth sliding friction.
    pub mu_s: f64,
    /// Cloth rolling resistance.
    pub mu_r: f64,
    /// Spin decay (rad/s²).
    pub spin_decay_rad_s2: f64,
    /// Ball–ball restitution.
    pub e_b: f64,
    /// Ball–ball friction table.
    pub mu_b: MuB,
    /// Cushion normal-channel coefficient.
    pub e_n: f64,
    /// Cushion friction.
    pub mu_c: f64,
    /// Cloth/slate restitution for airborne landings.
    pub e_slate: f64,
    /// Cue pivot length (mm), the squirt scalar.
    pub pivot_mm: f64,
}

impl Profile {
    /// The shipped default profile (`physics.md` §5's default column).
    #[must_use]
    pub fn default_profile() -> Self {
        Self {
            id: "default".to_string(),
            provenance: Provenance::default_record(),
            conditions: Conditions::default_record(),
            mu_s: DEFAULT_MU_S,
            mu_r: DEFAULT_MU_R,
            spin_decay_rad_s2: DEFAULT_SPIN_DECAY_RAD_S2,
            e_b: DEFAULT_E_B,
            // TP A-28's friction fit, the shipped throw source (`physics.md` §3.2/§12).
            mu_b: MuB::new(0.009_951, 0.108, 1.088),
            e_n: DEFAULT_E_N,
            mu_c: DEFAULT_MU_C,
            e_slate: DEFAULT_E_SLATE,
            pivot_mm: DEFAULT_PIVOT_MM,
        }
    }

    /// Sliding deceleration (mm/s²).
    #[must_use]
    pub fn slide_decel_mm_s2(&self) -> f64 {
        self.mu_s * GRAVITY_MM_S2
    }

    /// Rolling deceleration (mm/s²).
    #[must_use]
    pub fn roll_decel_mm_s2(&self) -> f64 {
        self.mu_r * GRAVITY_MM_S2
    }
}

/// Where a profile's values came from (`physics.md` §5: every fit is reproducible, so the record says
/// what it was fitted to).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The fitting sources, by the spec's labels.
    pub sources: Vec<String>,
    /// The label the constants carry (`fitted`, `measured`, `carried`, `provisional`, `derived`).
    pub label: String,
    /// What the record does *not* claim.
    pub caveats: Vec<String>,
}

impl Provenance {
    /// The default profile's provenance (`physics.md` §5's labels).
    #[must_use]
    pub fn default_record() -> Self {
        Self {
            sources: vec![
                "Dr. Dave / TP A-28 (μb fit)".to_string(),
                "TP B-6 (cushion retention: e_c = 0.70 ⇒ e_n ≈ 0.78)".to_string(),
                "WPA equipment section (geometry)".to_string(),
                "prototype #8 (sleep thresholds, ε)".to_string(),
            ],
            label: "fitted (e_n) / carried (μb, e_slate) / not identifiable (μs, μr)".to_string(),
            caveats: vec![
                "μb's published 0.03–0.08 band is provisional and not portable".to_string(),
                "e_slate is carried, not fitted: pinned by the ball-drop test".to_string(),
                "μc is weakly identified: bounded below, never pinned in value".to_string(),
            ],
        }
    }
}

/// The conditions a profile describes (`physics.md` §5: cloth speed, humidity, ball set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    /// Cloth description.
    pub cloth: String,
    /// Relative humidity (%), where recorded.
    pub humidity_pct: Option<f64>,
    /// Ball set description.
    pub balls: String,
    /// Table description.
    pub table: String,
}

impl Conditions {
    /// The conditions the default values were measured under.
    #[must_use]
    pub fn default_record() -> Self {
        Self {
            cloth: "9 ft worsted cloth, nominal".to_string(),
            humidity_pct: None,
            balls: "standard 2.25 in phenolic set, 170 g nominal".to_string(),
            table: "9 ft, 2540 × 1270 mm playing surface".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec's bound is **absolute**: 1.0e-5 (`physics.md` §3.2, as amended at M0 after this table
    /// measured 9.84e-6 absolute against 8.4e-5 relative — the relative figure is large only where μb is
    /// smallest). Both bounds are asserted so a step or coefficient change cannot silently degrade either.
    #[test]
    fn the_mu_b_table_meets_the_specs_error_bound() {
        let table = MuB::new(0.009_951, 0.108, 1.088);
        let mut worst_abs: f64 = 0.0;
        let mut v = 0.0_f64;
        while v <= MU_B_MAX_MM_S {
            let abs = (table.eval(v) - table.exact(v)).abs();
            if abs > worst_abs {
                worst_abs = abs;
            }
            v += 1.0;
        }
        assert!(
            worst_abs <= 1.0e-5,
            "worst absolute error {worst_abs:e} exceeds the spec's 1.0e-5"
        );
        let worst_rel = table.max_relative_error();
        assert!(
            worst_rel <= 1.0e-4,
            "worst relative error {worst_rel:e} exceeds the documented 8.4e-5 band"
        );
    }

    #[test]
    fn the_table_is_monotone_decreasing_and_clamped() {
        let table = MuB::new(0.009_951, 0.108, 1.088);
        assert!(table.eval(0.0) > table.eval(1000.0));
        assert!(table.eval(-5.0) == table.eval(0.0));
        assert!(table.eval(1.0e9) == table.eval(MU_B_MAX_MM_S));
    }
}
