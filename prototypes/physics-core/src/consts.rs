//! Fixed constants (#7 §5 geometry/equipment table) and the profile-settable
//! interaction coefficients. Units: mm, g, s. Gravity therefore in mm/s^2.

/// Ball radius (mm), 2.25 in.
pub const R: f64 = 28.575;
/// Ball mass (g).
pub const M_BALL: f64 = 170.0;
/// Ball moment of inertia, 2/5 m R^2 (g mm^2).
pub const I_BALL: f64 = 0.4 * M_BALL * R * R;
/// Standard gravity, mm/s^2 (9.80665 m/s^2).
pub const G: f64 = 9806.65;

/// Playing surface: 2540 x 1270 mm (9 ft).
pub const TABLE_LEN: f64 = 2540.0;
pub const TABLE_WID: f64 = 1270.0;
pub const HALF_LEN: f64 = TABLE_LEN / 2.0; // 1270
pub const HALF_WID: f64 = TABLE_WID / 2.0; // 635
/// Head string at x = -635; foot spot at x = +635 (#7 §2).
pub const HEAD_STRING_X: f64 = -635.0;
pub const FOOT_SPOT_X: f64 = 635.0;

/// Cushion nose height, 63.5% of ball diameter (mm above the cloth).
pub const NOSE_HEIGHT: f64 = 0.635 * 2.0 * R;
/// Corner pocket mouth, mid of WPA 4.5-4.625 in (mm).
pub const MOUTH_CORNER: f64 = 115.9;
/// Side pocket mouth, mid of WPA 5-5.125 in (mm).
pub const MOUTH_SIDE: f64 = 128.6;
/// Pocket cut angles (degrees, WPA).
pub const CUT_CORNER_DEG: f64 = 142.0;
pub const CUT_SIDE_DEG: f64 = 104.0;

/// Simultaneity window epsilon (s) -- spec constant (#7 §5).
pub const EPS_GROUP: f64 = 1e-9;

/// Sleep thresholds: the two numbers this ticket pins by measurement (#7 §6).
/// Defaults as proposed in #7 §5 before measurement: 1 mm/s and 0.01 rad/s.
pub const SLEEP_V_MM_S: f64 = 1.0;
pub const SLEEP_W_RAD_S: f64 = 0.01;

/// A ball clears the cushion instead of hitting it once its *underside* is
/// above the cushion's top, i.e. centre height > R + nose height. The spec has
/// no rail-top geometry yet; this prototype branch is what makes "driven off
/// the table" reachable (see RESULTS.md).
pub const CUSHION_CONTACT_CEILING: f64 = NOSE_HEIGHT;

/// Profile-settable interaction coefficients (#7 §5). A profile is a named
/// record: values + provenance, so every fit is reproducible.
#[derive(Clone, Debug)]
pub struct Profile {
    pub mu_s: f64,        // cloth sliding friction, default 0.20
    pub mu_r: f64,        // cloth rolling resistance, default 0.010
    pub spin_decay: f64,  // turntable spin decay, rad/s^2, default 10
    pub e_b: f64,         // ball-ball restitution, default 0.95
    pub mu_b: MuB,        // ball-ball friction, speed dependent
    pub e_cushion: f64,   // cushion restitution along the contact normal
    pub mu_cushion: f64,  // cushion friction (tangential impulse limit)
    pub e_slate: f64,     // cloth/slate restitution for airborne landings
    pub pivot_mm: f64,    // cue: natural pivot length, the squirt scalar
}

/// Speed-dependent ball-ball friction: mu(v) = a + b*exp(-c*v). The prototype
/// evaluates the exponential when building a table and then fits a
/// piecewise-linear curve, so the *core* only ever does `+ - * /` on it
/// (#11 §3 bans transcendentals from the core).
#[derive(Clone, Debug)]
pub struct MuB {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    table: Vec<(f64, f64)>, // (speed mm/s, mu), ascending, piecewise-linear
}

impl MuB {
    pub fn new(a: f64, b: f64, c: f64) -> Self {
        // Tabulate on [0, 12 m/s] with a 25 mm/s step: max interpolation error
        // is reported by `table_error()`.
        let mut table = Vec::with_capacity(512);
        let mut v = 0.0f64;
        while v <= 12000.0 {
            table.push((v, a + b * (-c * (v / 1000.0)).exp()));
            v += 25.0;
        }
        MuB { a, b, c, table }
    }
    /// Exact closed form (only used to build/validate the table).
    pub fn exact(&self, v_mm_s: f64) -> f64 {
        self.a + self.b * (-self.c * (v_mm_s / 1000.0)).exp()
    }
    /// Core evaluation: piecewise linear, no transcendental.
    pub fn eval(&self, v_mm_s: f64) -> f64 {
        if v_mm_s <= 0.0 {
            return self.table[0].1;
        }
        let last = self.table.len() - 1;
        let step = 25.0;
        let idx = (v_mm_s / step) as usize;
        if idx >= last {
            return self.table[last].1;
        }
        let (v0, m0) = self.table[idx];
        let (v1, m1) = self.table[idx + 1];
        let f = (v_mm_s - v0) / (v1 - v0);
        m0 + (m1 - m0) * f
    }
    pub fn table_error(&self) -> f64 {
        let mut worst: f64 = 0.0;
        let mut v = 0.0f64;
        while v <= 12000.0 {
            let e = (self.eval(v) - self.exact(v)).abs();
            if e > worst {
                worst = e;
            }
            v += 1.0;
        }
        worst
    }
}

impl Profile {
    /// The spec's default profile (#7 §5 nominal values).
    pub fn spec_default() -> Self {
        Profile {
            mu_s: 0.20,
            mu_r: 0.010,
            spin_decay: 10.0,
            e_b: 0.95,
            // TP A-28's friction fit (mu = a + b e^{-cv}), the throw source.
            mu_b: MuB::new(0.009_951, 0.108, 1.088),
            e_cushion: 0.75,
            mu_cushion: 0.2,
            e_slate: 0.60,
            pivot_mm: 350.0,
        }
    }
    /// Sliding deceleration (mm/s^2).
    pub fn a_slide(&self) -> f64 {
        self.mu_s * G
    }
    /// Rolling deceleration (mm/s^2).
    pub fn a_roll(&self) -> f64 {
        self.mu_r * G
    }
    /// The horizontal COR a cushion impulse produces from `e_cushion`, given
    /// the nose-height contact normal. Derivable in closed form:
    /// COR_eff = n_x^2 * (1 + e_n) - 1.
    pub fn cushion_effective_cor(&self) -> f64 {
        let n = nose_normal_magnitude();
        n * n * (1.0 + self.e_cushion) - 1.0
    }
}

/// |n_x| of the cushion contact normal: sqrt(R^2 - (h_nose - R)^2) / R.
/// The nose sits 63.5% of the ball diameter above the cloth, i.e. above the
/// ball centre, so the contact normal is tilted down by ~15.7 degrees.
pub fn nose_normal_magnitude() -> f64 {
    let dz = NOSE_HEIGHT - R;
    ((R * R - dz * dz).sqrt()) / R
}
/// z-component magnitude of the same normal.
pub fn nose_normal_z() -> f64 {
    (NOSE_HEIGHT - R) / R
}
