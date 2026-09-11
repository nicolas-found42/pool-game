//! Shot-declaration state, miscue-envelope geometry, and aim-guide math for the #10
//! cue-UX prototype.
//!
//! Frame and units follow the physics decision on #7 §2: SI millimetres, origin at the
//! centre of the playing surface, +x along the long axis (head → foot), +y across,
//! +z up, right-handed. A strike is `{aim direction, cue-ball launch speed, spin offsets
//! (a, b) in tip radii, elevation}` (#7 §4); the miscue envelope is enforced here as a
//! hard limit, not simulated.

use bevy::math::{Vec2, Vec3};

/// Ball radius, #7 §2.
pub const BALL_R: f32 = 28.575;
/// Tip–ball friction, #7 §5 (defines the miscue envelope).
pub const MU_TIP: f32 = 0.6;
/// Assumed cue-tip radius (a 12.7 mm tip). #7 §4 states offsets in *tip radii* but the
/// limit in ball radii; this constant only sets the displayed scale, the limit is geometric.
pub const TIP_RADIUS: f32 = 6.35;
pub const TABLE_HALF_X: f32 = 1270.0;
pub const TABLE_HALF_Y: f32 = 635.0;
pub const MAX_SPEED: f32 = 7000.0;
pub const MAX_DRAG: f32 = 350.0;
pub const ELEV_STEP: f32 = 5.0 * std::f32::consts::PI / 180.0;
pub const MAX_ELEV: f32 = 80.0 * std::f32::consts::PI / 180.0;
pub const STICK_LEN: f32 = 1450.0;
/// How far past the envelope the spin selector reaches, so the reject state is reachable.
pub const SPIN_SELECT_MAX: f32 = 1.25;
/// Tolerance on the envelope test. A declaration authored *exactly* at the limit is
/// legal (μ is itself a fitted constant); without a tolerance, an offset of R·μ/√(1+μ²)
/// computes as a hair outside and reads as rejected — the prototype's own boundary bug.
pub const ENVELOPE_EPS_MM: f32 = 1e-3;

pub const CUE_BALL: Vec2 = Vec2::new(-450.0, -120.0);
pub const OBJECT_BALLS: [Vec2; 4] = [
    Vec2::new(150.0, 200.0),
    Vec2::new(620.0, -330.0),
    Vec2::new(-60.0, 470.0),
    Vec2::new(1000.0, 320.0),
];
/// Unit aim used by every scripted screenshot state: a ~27 deg cut on object ball 1
/// (chosen so the aim line, ghost ball, object-ball line and tangent line all separate).
pub const PRESET_AIM: Vec2 = Vec2::new(0.8998, 0.4363);

/// Offset at which the required friction ratio reaches mu: with θ the angle between the
/// tip path and the contact normal, no miscue needs tan θ ≤ mu, so the largest in-plane
/// offset is ρ = R sin θ = R mu / √(1 + mu²) ≈ 0.514 R ≈ 14.7 mm - the "#7 §4 ≈ 0.5 R".
pub fn envelope_mm() -> f32 {
    BALL_R * MU_TIP / (1.0 + MU_TIP * MU_TIP).sqrt()
}

/// The same limit expressed in the ticket's unit (tip radii), for the read-out.
pub fn envelope_tip_radii() -> f32 {
    envelope_mm() / TIP_RADIUS
}

#[derive(Clone, Copy, Debug)]
pub struct Declaration {
    /// Unit horizontal aim direction, table frame.
    pub aim: Vec2,
    /// Cue-ball launch speed, mm/s.
    pub speed: f32,
    /// Spin offset across the ball face, tip radii (+ = right of the aim direction).
    pub a: f32,
    /// Spin offset up the ball face, tip radii (+ = above centre = follow side).
    pub b: f32,
    /// Cue elevation, radians.
    pub elevation: f32,
}

impl Default for Declaration {
    fn default() -> Self {
        Self {
            aim: PRESET_AIM,
            speed: 0.0,
            a: 0.0,
            b: 0.0,
            elevation: 0.0,
        }
    }
}

impl Declaration {
    pub fn offset(&self) -> Vec2 {
        Vec2::new(self.a, self.b) * TIP_RADIUS
    }

    pub fn offset_len(&self) -> f32 {
        self.offset().length()
    }

    /// Signed distance past the miscue envelope in mm; ≤ 0 is inside (legal).
    pub fn envelope_margin(&self) -> f32 {
        self.offset_len() - envelope_mm()
    }

    pub fn valid(&self) -> bool {
        self.envelope_margin() <= ENVELOPE_EPS_MM
    }

    pub fn elevation_deg(&self) -> f32 {
        self.elevation.to_degrees()
    }

    pub fn aim_deg(&self) -> f32 {
        self.aim.y.atan2(self.aim.x).to_degrees()
    }

    /// The input-log declaration body (#4 shape, `call` omitted - rules vocabulary is #6).
    pub fn json(&self) -> String {
        format!(
            "{{\"aim\":{{\"x\":{:+.3},\"y\":{:+.3}}},\"speed\":{:.0},\n         \"spin\":{{\"a\":{:+.2},\"b\":{:+.2}}},\"elevation\":{:.3}}}",
            self.aim.x, self.aim.y, self.speed, self.a, self.b, self.elevation
        )
    }

    /// Plain-language reading of what is being authored.
    pub fn intent(&self) -> String {
        let env = envelope_mm();
        let mut parts: Vec<&str> = Vec::new();
        if self.b * TIP_RADIUS > 0.25 * env {
            parts.push("follow / top");
        } else if self.b * TIP_RADIUS < -0.25 * env {
            parts.push("draw / back");
        } else {
            parts.push("centre-height");
        }
        if self.a * TIP_RADIUS > 0.25 * env {
            parts.push("right english");
        } else if self.a * TIP_RADIUS < -0.25 * env {
            parts.push("left english");
        }
        let elev = self.elevation_deg();
        if elev < 1.0 {
            parts.push("level cue");
        } else if elev < 35.0 {
            parts.push("slightly elevated");
        } else {
            parts.push("masse / jump elevation");
        }
        parts.join(" | ")
    }
}

// ---------------------------------------------------------------------------
// Cue frame: everything below is in the table frame (x long, y across, z up).

/// Unit cue axis: the direction the tip pushes the ball. Raising the butt tips the push
/// direction *down* into the cloth, so z is negative for positive elevation.
pub fn cue_axis(aim: Vec2, elevation: f32) -> Vec3 {
    let (s, c) = elevation.sin_cos();
    Vec3::new(aim.x * c, aim.y * c, -s)
}

/// In-plane "across the ball face" axis (+a): the shooter's right, horizontal by
/// construction. With `aim` along the long axis this is `aim x up`, so `a > 0` is right
/// english both on screen (in the widget) and in the frame.
pub fn cue_side(aim: Vec2) -> Vec3 {
    Vec3::new(aim.y, -aim.x, 0.0)
}

/// In-plane "up the ball face" axis (+b): the level-case up direction carried along with
/// the cue as it stands up (`e_b = e_a x u`). At zero elevation this is world up and
/// `b > 0` is contact above centre = follow; as the cue stands up the (a, b) plane tilts
/// with it, which is why the same (a, b) means a different contact point — and a
/// different spin — at different elevations.
pub fn cue_up(aim: Vec2, elevation: f32) -> Vec3 {
    cue_side(aim).cross(cue_axis(aim, elevation))
}

/// Tip contact point on the ball surface, relative to the ball centre.
pub fn contact_point(aim: Vec2, elevation: f32, offset: Vec2) -> Vec3 {
    let axis = cue_axis(aim, elevation);
    let o = cue_side(aim) * offset.x + cue_up(aim, elevation) * offset.y;
    let h = (BALL_R * BALL_R - o.length_squared()).max(0.0).sqrt();
    o - axis * h
}

/// Direction of the angular velocity the strike imparts (∝ contact point x cue axis).
pub fn spin_axis(aim: Vec2, elevation: f32, offset: Vec2) -> Vec3 {
    let w = contact_point(aim, elevation, offset).cross(cue_axis(aim, elevation));
    if w.length_squared() > 1e-9 {
        w.normalize()
    } else {
        Vec3::ZERO
    }
}

// ---------------------------------------------------------------------------
// Aim guides.

#[derive(Clone, Copy, Debug)]
pub enum Guide {
    /// First ball on the aim line: ghost ball, object-ball line, tangent line.
    Ball {
        ghost: Vec2,
        target: Vec2,
        travel: Vec2,
        tangent: Vec2,
        cut_deg: f32,
    },
    /// No ball: first cushion and the reflection.
    Cushion { hit: Vec2, reflect: Vec2 },
}

/// First contact of the aim ray with an object ball (2R centre distance), else the first
/// cushion of the playing surface.
pub fn aim_guide(cue: Vec2, aim: Vec2, objects: &[Vec2]) -> Guide {
    let mut best: Option<(f32, Vec2)> = None;
    for &c in objects {
        let d = c - cue;
        let along = d.dot(aim);
        if along <= 0.0 {
            continue;
        }
        let perp2 = d.length_squared() - along * along;
        let contact2 = (2.0 * BALL_R) * (2.0 * BALL_R);
        if perp2 > contact2 {
            continue;
        }
        let t = along - (contact2 - perp2).sqrt();
        if t < 0.0 {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, c));
        }
    }
    if let Some((t, c)) = best {
        let ghost = cue + aim * t;
        let travel = (c - ghost).normalize();
        let mut tangent = Vec2::new(-travel.y, travel.x);
        if tangent.dot(aim) < 0.0 {
            tangent = -tangent;
        }
        Guide::Ball {
            ghost,
            target: c,
            travel,
            tangent,
            cut_deg: travel.dot(aim).clamp(-1.0, 1.0).acos().to_degrees(),
        }
    } else {
        let mut best_t = f32::INFINITY;
        let mut normal = Vec2::ZERO;
        for (axis, limit) in [(0usize, TABLE_HALF_X), (1usize, TABLE_HALF_Y)] {
            for sign in [1.0f32, -1.0] {
                let dir = axis_dir(aim, axis);
                if dir * sign <= 1e-6 {
                    continue;
                }
                let t = (limit * sign - axis_pos(cue, axis)) / dir;
                if t > 0.0 && t < best_t {
                    best_t = t;
                    normal = if axis == 0 {
                        Vec2::new(sign, 0.0)
                    } else {
                        Vec2::new(0.0, sign)
                    };
                }
            }
        }
        let hit = cue + aim * best_t;
        Guide::Cushion {
            hit,
            reflect: aim - 2.0 * aim.dot(normal) * normal,
        }
    }
}

fn axis_pos(v: Vec2, axis: usize) -> f32 {
    if axis == 0 { v.x } else { v.y }
}

fn axis_dir(v: Vec2, axis: usize) -> f32 {
    if axis == 0 { v.x } else { v.y }
}

/// Rotation of a 3D table-frame vector into Bevy's Y-up frame (x → X, y → −Z, z → Y).
pub fn to_bevy(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y)
}

/// One scripted screenshot state: a fixed declaration plus a drag-back distance.
pub struct Preset {
    pub file: &'static str,
    pub label: &'static str,
    pub decl: Declaration,
    /// Drag-back distance in mm — what the top-down cue and the power bar show.
    pub pull_mm: f32,
    /// Show the post-commit read-out state (the green COMMITTED line).
    pub commit: bool,
}

/// The states the shot-authoring flow is judged on: overviews, the aim guide, the
/// drag-back power ladder, the spin ladder, the elevation/masse ladder, the envelope
/// edge, and the rejected over-limit offset.
pub fn presets() -> Vec<Preset> {
    let env = envelope_tip_radii();
    let deg = |d: f32| d * std::f32::consts::PI / 180.0;
    let d = |speed: f32, a: f32, b: f32, elev_deg: f32| Declaration {
        aim: PRESET_AIM,
        speed,
        a,
        b,
        elevation: deg(elev_deg),
    };
    let spd = |pull: f32| (pull / MAX_DRAG).clamp(0.0, 1.0) * MAX_SPEED;
    let mid = 175.0f32;
    vec![
        Preset {
            file: "01-overview-level",
            label: "overview | level cue, mid drag-back",
            decl: d(spd(mid), 0.0, 0.0, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "02-overview-elevated-70",
            label: "overview | 70 deg cue (top-down foreshortening)",
            decl: d(spd(mid), 0.0, 0.0, 70.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "03-aim-guide-cut",
            label: "aim guide | cut 27 deg, cue at the ball (no power)",
            decl: d(0.0, 0.0, 0.0, 0.0),
            pull_mm: 0.0,
            commit: false,
        },
        Preset {
            file: "04-power-low",
            label: "drag-back | low power",
            decl: d(spd(60.0), 0.0, 0.0, 0.0),
            pull_mm: 60.0,
            commit: false,
        },
        Preset {
            file: "05-power-mid",
            label: "drag-back | mid power",
            decl: d(spd(175.0), 0.0, 0.0, 0.0),
            pull_mm: 175.0,
            commit: false,
        },
        Preset {
            file: "06-power-max",
            label: "drag-back | full power (break speed)",
            decl: d(spd(MAX_DRAG), 0.0, 0.0, 0.0),
            pull_mm: MAX_DRAG,
            commit: false,
        },
        Preset {
            file: "07-spin-draw-max",
            label: "spin | max draw at the envelope edge",
            decl: d(spd(mid), 0.0, -env, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "08-spin-follow-max",
            label: "spin | max follow at the envelope edge",
            decl: d(spd(mid), 0.0, env, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "09-spin-side-right-max",
            label: "spin | max right english at the envelope edge",
            decl: d(spd(mid), env, 0.0, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "10-spin-side-left-max",
            label: "spin | max left english at the envelope edge",
            decl: d(spd(mid), -env, 0.0, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "11-masse-45",
            label: "elevation 45 deg | masse (right-low contact)",
            decl: d(spd(mid), 0.5 * env, -0.5 * env, 45.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "12-masse-70",
            label: "elevation 70 deg | deep masse",
            decl: d(spd(mid), 0.6 * env, -0.5 * env, 70.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "13-envelope-edge",
            label: "miscue limit | offset exactly at the envelope (legal)",
            decl: d(spd(mid), 0.7071 * env, -0.7071 * env, 0.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "14-over-limit-rejected",
            label: "miscue limit | over-limit offset - REJECTED",
            decl: d(spd(mid), 1.3 * env, -1.3 * env, 20.0),
            pull_mm: mid,
            commit: false,
        },
        Preset {
            file: "15-commit-readout",
            label: "commit | declaration committed (read-out state)",
            decl: d(spd(mid), 0.35 * env, -0.5 * env, 20.0),
            pull_mm: mid,
            commit: true,
        },
    ]
}
