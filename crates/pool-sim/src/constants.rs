//! The fixed geometry and equipment constants of `physics.md` §5, and the frame of §2.
//!
//! SI millimetres; origin at the centre of the playing surface; x along the long axis (head → foot
//! positive), y across, z up, right-handed. Masses in grams, so gravity is mm/s².

/// Playing surface length (mm), 9 ft.
pub const TABLE_LEN_MM: f64 = 2540.0;
/// Playing surface width (mm).
pub const TABLE_WIDTH_MM: f64 = 1270.0;
/// Half the playing surface length; also the head/foot rail line.
pub const HALF_LEN_MM: f64 = TABLE_LEN_MM / 2.0;
/// Half the playing surface width.
pub const HALF_WIDTH_MM: f64 = TABLE_WIDTH_MM / 2.0;

/// The head string, bounding the quarter nearest the head rail.
pub const HEAD_STRING_X_MM: f64 = -635.0;
/// The foot spot: the rack's apex position, on the long string.
pub const FOOT_SPOT_X_MM: f64 = 635.0;

/// Ball radius (mm), 2.25 in.
pub const BALL_RADIUS_MM: f64 = 28.575;
/// Ball diameter (mm).
pub const BALL_DIAMETER_MM: f64 = 2.0 * BALL_RADIUS_MM;
/// Ball mass (g).
pub const BALL_MASS_G: f64 = 170.0;
/// Ball moment of inertia (g·mm²), ⅖ m R².
pub const BALL_INERTIA_G_MM2: f64 = 0.4 * BALL_MASS_G * BALL_RADIUS_MM * BALL_RADIUS_MM;

/// Cushion nose height above the cloth (mm), 63.5 % of the ball diameter.
pub const CUSHION_NOSE_HEIGHT_MM: f64 = 0.635 * BALL_DIAMETER_MM;
/// The centre height at which a ball clears a cushion (mm): `R` + nose height.
pub const CUSHION_CLEARANCE_MM: f64 = BALL_RADIUS_MM + CUSHION_NOSE_HEIGHT_MM;
/// The cushion contact normal's downward component, `(nose − R)/R`: every cushion contact carries a
/// tilted normal, which is what makes the vertical channel entailed (`physics.md` §3.3).
pub const CUSHION_NORMAL_Z: f64 = (CUSHION_NOSE_HEIGHT_MM - BALL_RADIUS_MM) / BALL_RADIUS_MM;

/// Corner pocket mouth (mm), mid of WPA's 4.5–4.625 in.
pub const POCKET_MOUTH_CORNER_MM: f64 = 115.9;
/// Side pocket mouth (mm), mid of WPA's 5–5.125 in.
pub const POCKET_MOUTH_SIDE_MM: f64 = 128.6;
/// Corner pocket cut angle (degrees).
pub const POCKET_CUT_CORNER_DEG: f64 = 142.0;
/// Side pocket cut angle (degrees).
pub const POCKET_CUT_SIDE_DEG: f64 = 104.0;

/// Standard gravity (mm/s²).
pub const GRAVITY_MM_S2: f64 = 9806.65;

/// Tip–ball friction, which defines the miscue envelope (`physics.md` §3.6).
pub const TIP_FRICTION_MU: f64 = 0.6;

/// The simultaneity window ε (s): events within ε of the earliest form one group.
pub const SIMULTANEITY_EPS_S: f64 = 1e-9;

/// Sleep threshold, linear (mm/s).
pub const SLEEP_LINEAR_MM_S: f64 = 1.0;
/// Sleep threshold, angular (rad/s).
pub const SLEEP_ANGULAR_RAD_S: f64 = 0.01;

/// Frozen tolerance (mm): one constant for rail and ball frozen status.
pub const FROZEN_GAP_MM: f64 = 0.5;

/// The contact slop (mm): at this scale two surfaces are in contact. One number serves the solver's
/// contact tests, the position-only separation step, and the placement boundary, so all three agree on
/// where contact begins and ends (`physics.md` §1). It is a rounding guard, four orders below the
/// model's thinnest real gap (a frozen ball's 0.5 mm), never a physical gap: an exact-contact position
/// re-measured by a different route can land a few ulps inside it, and contact is what it is.
pub const CONTACT_SLOP_MM: f64 = 1e-6;

/// The drop predicate's boundary tolerance (mm): the crossing root must reach the mouth boundary to
/// within this much, or it is not a crossing (`physics.md` §3.5.2b).
pub const DROP_BOUNDARY_TOLERANCE_MM: f64 = 1.0;

/// The cushion contact normal's horizontal magnitude, `√(R² − (nose − R)²)/R` (`physics.md` §3.3):
/// the tilted normal's inward part. A function rather than a `const` because `sqrt` is not
/// const-callable; the IEEE square root is correctly rounded, so it is bit-identical everywhere.
#[must_use]
pub fn cushion_normal_horizontal() -> f64 {
    let rise = CUSHION_NOSE_HEIGHT_MM - BALL_RADIUS_MM;
    (BALL_RADIUS_MM * BALL_RADIUS_MM - rise * rise).sqrt() / BALL_RADIUS_MM
}

/// `√3` as the pinned literal, for the rack lattice's row step (`rules-break.md` §2).
pub const SQRT_3: f64 = 1.732_050_807_568_877_2;
/// The rack's row step along x (mm).
pub const RACK_ROW_STEP_MM: f64 = BALL_RADIUS_MM * SQRT_3;
