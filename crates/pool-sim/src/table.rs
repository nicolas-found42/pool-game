//! Table geometry: cushion faces, pocket jaws, mouths, and the drop regions of `physics.md` §3.5.
//!
//! Frame (`physics.md` §2): SI millimetres, origin at the centre of the playing surface; x along the
//! long axis (head → foot positive), y across, z up, right-handed.

use serde::{Deserialize, Serialize};

use crate::constants::{HALF_LEN_MM, HALF_WIDTH_MM, POCKET_MOUTH_CORNER_MM, POCKET_MOUTH_SIDE_MM};
use crate::math::{V3, v3};
use crate::profile::Profile;

/// The corner jaw faces' `cos` and `sin`: the face leaves the rail at `180° − 142°` = 38°
/// ([`POCKET_CUT_CORNER_DEG`], `physics.md` §3.5.4). Correctly-rounded `f64` literals — the core
/// evaluates no transcendentals (`architecture.md` §3).
const CORNER_JAW_COS: f64 = 0.788_010_753_606_721_9;
const CORNER_JAW_SIN: f64 = 0.615_661_475_325_658_3;
/// The side jaw faces' `cos` and `sin`, at `180° − 104°` = 76° ([`POCKET_CUT_SIDE_DEG`]).
const SIDE_JAW_COS: f64 = 0.241_921_895_599_667_67;
const SIDE_JAW_SIN: f64 = 0.970_295_726_275_996_5;

/// A cushion face (`physics.md` §3.3). A ball that touches one is driven to that rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rail {
    /// The short rail at `x` = −1270 mm.
    HeadShort,
    /// The short rail at `x` = +1270 mm.
    FootShort,
    /// The long rail at `y` = −635 mm.
    LeftLong,
    /// The long rail at `y` = +635 mm.
    RightLong,
}

/// The six pockets (`physics.md` §3.5), named by corner and side of the frame. The order is the
/// construction order of [`Table::new`], and the variant's index is the pocket's identity in facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PocketId {
    /// The corner at `x` = +1270, `y` = +635.
    FootPlusY,
    /// The corner at `x` = +1270, `y` = −635.
    FootMinusY,
    /// The corner at `x` = −1270, `y` = +635.
    HeadPlusY,
    /// The corner at `x` = −1270, `y` = −635.
    HeadMinusY,
    /// The side mouth at `y` = −635.
    SideMinusY,
    /// The side mouth at `y` = +635.
    SidePlusY,
}

impl PocketId {
    /// Every pocket, in construction order.
    pub const ALL: [Self; 6] = [
        Self::FootPlusY,
        Self::FootMinusY,
        Self::HeadPlusY,
        Self::HeadMinusY,
        Self::SideMinusY,
        Self::SidePlusY,
    ];

    /// The pocket's index, 0..6, matching [`PocketId::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::FootPlusY => 0,
            Self::FootMinusY => 1,
            Self::HeadPlusY => 2,
            Self::HeadMinusY => 3,
            Self::SideMinusY => 4,
            Self::SidePlusY => 5,
        }
    }
}

/// A cushion or jaw face: a line segment with an inward horizontal normal (into the playing volume).
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    /// A point on the face line.
    pub p: V3,
    /// The inward unit normal.
    pub n: V3,
    /// The unit tangent; the face runs from `p` to `p + t·len`.
    pub t: V3,
    /// The face extent along `t`.
    pub len: f64,
    /// The rail this face is a cushion face of, if it is one.
    pub rail: Option<Rail>,
    /// The pocket this face is a jaw of, if it is one.
    pub pocket: Option<PocketId>,
}

/// A mouth's drop region (`physics.md` §3.5.1): a continuous function whose zero is the boundary and
/// which is positive inside the region (the pocket side).
#[derive(Debug, Clone, PartialEq)]
pub enum DropShape {
    /// A corner mouth: the half-plane beyond the jaw-tip chord, laterally bounded by the chord's
    /// half-width — the diagonal chord that spans the jaws.
    Chord {
        /// The chord's midpoint, i.e. the midpoint of the two jaw tips.
        mid: V3,
        /// The unit normal pointing from the playing surface into the pocket.
        outward: V3,
        /// Half the chord's length: the lateral extent of the mouth.
        half_width: f64,
    },
    /// A side mouth: the disc centred on the cushion line, radius from the pinned side-mouth width.
    Disc {
        /// The disc's centre, on the cushion line.
        centre: V3,
        /// The disc's radius.
        radius: f64,
    },
}

impl DropShape {
    /// The region function: positive inside the mouth, zero on its boundary, negative outside.
    #[must_use]
    pub fn f(&self, p: V3) -> f64 {
        match self {
            Self::Chord { mid, outward, .. } => (p - *mid).dot(*outward),
            Self::Disc { centre, radius } => *radius - (p - *centre).len(),
        }
    }

    /// The gradient of [`DropShape::f`] at `p`, used by the drop root finder. Unit length except at a
    /// disc's centre.
    #[must_use]
    pub fn grad(&self, p: V3) -> V3 {
        match self {
            Self::Chord { outward, .. } => *outward,
            Self::Disc { centre, .. } => -(p - *centre).norm(),
        }
    }

    /// Whether the crossing point is inside the mouth's lateral extent — the jaw-tip window
    /// (`physics.md` §3.5.2c). A side mouth's disc is its own window.
    #[must_use]
    pub fn lateral_ok(&self, p: V3) -> bool {
        match self {
            Self::Chord {
                mid,
                outward,
                half_width,
            } => {
                let tangent = v3(-outward.y, outward.x, 0.0);
                let lateral = (p - *mid).dot(tangent);
                lateral.abs() <= *half_width
            }
            Self::Disc { .. } => true,
        }
    }
}

/// One pocket mouth: its identity, its drop region, and the centre used to name it in facts.
#[derive(Debug, Clone, PartialEq)]
pub struct Pocket {
    /// The pocket's identity.
    pub id: PocketId,
    /// The drop region of `physics.md` §3.5.1.
    pub shape: DropShape,
    /// The mouth's centre, on the cushion line (corner: the chord's midpoint).
    pub mouth_center: V3,
}

/// The playing surface's colliders and mouths, with the condition it is played under.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// The cushion and jaw faces.
    pub walls: Vec<Wall>,
    /// The six mouths, indexed by [`PocketId::index`].
    pub pockets: [Pocket; 6],
    /// The jaw tips, as sharp point colliders (`physics.md` §3.5.4).
    pub tips: Vec<V3>,
    /// The condition record — the interaction coefficients this table is played with (`physics.md`
    /// §5). The table carries the profile (`architecture.md` §4).
    pub profile: Profile,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    /// The 9 ft WPA table of `physics.md` §5 under the default profile.
    #[must_use]
    pub fn new() -> Self {
        Self::with_profile(Profile::default_profile())
    }

    /// The 9 ft WPA table of `physics.md` §5: four cushion rails split by the side and corner mouths,
    /// twelve jaw faces at the pinned cut angles, and six mouths at the pinned widths.
    ///
    /// # Panics
    ///
    /// Panics if the mouth list does not hold exactly six pockets — a build bug, never caller input:
    /// the layout below pushes four corner mouths and two side mouths unconditionally.
    #[must_use]
    pub fn with_profile(profile: Profile) -> Self {
        let d_corner = POCKET_MOUTH_CORNER_MM / 2.0_f64.sqrt(); // the jaw tip's offset along each rail
        let half_corner = POCKET_MOUTH_CORNER_MM / 2.0;
        let half_side = POCKET_MOUTH_SIDE_MM / 2.0;

        let mut walls: Vec<Wall> = Vec::with_capacity(18);
        let mut tips: Vec<V3> = Vec::with_capacity(12);

        // The long rails (y = ±635), two segments each, split by the side mouth.
        for sy in [-1.0_f64, 1.0] {
            let y = sy * HALF_WIDTH_MM;
            let n = v3(0.0, -sy, 0.0);
            for (x0, x1) in [
                (-HALF_LEN_MM + d_corner, -half_side),
                (half_side, HALF_LEN_MM - d_corner),
            ] {
                walls.push(Wall {
                    p: v3(x0, y, 0.0),
                    n,
                    t: v3(1.0, 0.0, 0.0),
                    len: x1 - x0,
                    rail: Some(if sy > 0.0 {
                        Rail::RightLong
                    } else {
                        Rail::LeftLong
                    }),
                    pocket: None,
                });
            }
        }
        // The short rails (x = ±1270), one segment each.
        for sx in [-1.0_f64, 1.0] {
            let x = sx * HALF_LEN_MM;
            walls.push(Wall {
                p: v3(x, -HALF_WIDTH_MM + d_corner, 0.0),
                n: v3(-sx, 0.0, 0.0),
                t: v3(0.0, 1.0, 0.0),
                len: 2.0 * (HALF_WIDTH_MM - d_corner),
                rail: Some(if sx > 0.0 {
                    Rail::FootShort
                } else {
                    Rail::HeadShort
                }),
                pocket: None,
            });
        }

        // The corner mouths. The chords and jaw tips are built in the order of `PocketId::ALL`.
        let mut pockets: Vec<Pocket> = Vec::with_capacity(6);
        add_corner_mouths(d_corner, half_corner, &mut walls, &mut pockets, &mut tips);
        // The side mouths, centred on the long rails.
        for sy in [-1.0_f64, 1.0] {
            let centre = v3(0.0, sy * HALF_WIDTH_MM, 0.0);
            let id = PocketId::ALL[pockets.len()];
            pockets.push(Pocket {
                id,
                shape: DropShape::Disc {
                    centre,
                    radius: half_side,
                },
                mouth_center: centre,
            });
            for sx in [-1.0_f64, 1.0] {
                let tip = v3(sx * half_side, sy * HALF_WIDTH_MM, 0.0);
                add_jaw(
                    tip,
                    v3(sx * SIDE_JAW_COS, sy * SIDE_JAW_SIN, 0.0),
                    centre,
                    id,
                    &mut walls,
                );
                tips.push(tip);
            }
        }

        let pockets: [Pocket; 6] = pockets.try_into().expect("six pockets are built");
        debug_assert!(pockets.iter().enumerate().all(|(i, p)| p.id.index() == i));

        Self {
            walls,
            pockets,
            tips,
            profile,
        }
    }
}

/// The four corner mouths, in the order of `PocketId::ALL`: each drops along the chord between its
/// jaw tips (`physics.md` §3.5.3), and its two jaws leave the long and the short rail at the pinned
/// cut angle.
fn add_corner_mouths(
    d_corner: f64,
    half_corner: f64,
    walls: &mut Vec<Wall>,
    pockets: &mut Vec<Pocket>,
    tips: &mut Vec<V3>,
) {
    for (sx, sy) in [(1.0_f64, 1.0_f64), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let a = v3(sx * (HALF_LEN_MM - d_corner), sy * HALF_WIDTH_MM, 0.0);
        let b = v3(sx * HALF_LEN_MM, sy * (HALF_WIDTH_MM - d_corner), 0.0);
        let corner = v3(sx * HALF_LEN_MM, sy * HALF_WIDTH_MM, 0.0);
        let mid = (a + b) / 2.0;
        let outward = (corner - mid).norm();
        let id = PocketId::ALL[pockets.len()];
        pockets.push(Pocket {
            id,
            shape: DropShape::Chord {
                mid,
                outward,
                half_width: half_corner,
            },
            mouth_center: mid,
        });
        // Jaw A leaves the long rail; jaw B leaves the short rail, at the same cut angle. The
        // playable side of a jaw face is the side holding the mouth centre.
        add_jaw(
            a,
            v3(sx * CORNER_JAW_COS, sy * CORNER_JAW_SIN, 0.0),
            mid,
            id,
            walls,
        );
        add_jaw(
            b,
            v3(sx * CORNER_JAW_SIN, sy * CORNER_JAW_COS, 0.0),
            mid,
            id,
            walls,
        );
        tips.push(a);
        tips.push(b);
    }
}

/// Add one jaw face at `tip`, running along `dir` into the pocket whose mouth centre is `mouth_center`.
fn add_jaw(tip: V3, dir: V3, mouth_center: V3, id: PocketId, walls: &mut Vec<Wall>) {
    let candidates = [v3(-dir.y, dir.x, 0.0), v3(dir.y, -dir.x, 0.0)];
    let to_mouth = mouth_center - tip;
    let n = if candidates[0].dot(to_mouth) > 0.0 {
        candidates[0]
    } else {
        candidates[1]
    };
    walls.push(Wall {
        p: tip,
        n,
        t: dir,
        // The face runs past the pocket's mouth; only its first stretch is ever reached by a ball.
        len: 220.0,
        rail: None,
        pocket: Some(id),
    });
}
