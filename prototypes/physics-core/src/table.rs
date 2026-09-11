//! Table geometry: cushion segments, pocket jaws and mouths, rack slots.
//!
//! Frame (#7 §2): SI millimetres, origin at the centre of the playing surface,
//! x along the long axis (head -> foot positive), y across, z up.

use crate::consts::*;
use crate::vec::{v3, V3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    HeadShort,  // x = -1270
    FootShort,  // x = +1270
    LeftLong,   // y = -635
    RightLong,  // y = +635
}

#[derive(Clone, Debug)]
pub struct Wall {
    pub p: V3,   // a point on the face line
    pub n: V3,   // inward unit normal (into the playing volume)
    pub t: V3,   // unit tangent along the face
    pub len: f64, // face extent from p along t
    pub rail: Option<Rail>,
    pub pocket: Option<usize>, // jaw face of this pocket
}

#[derive(Clone, Debug)]
pub enum DropShape {
    /// Corner: the half-plane beyond the mouth chord, laterally bounded.
    ChordPlane { m: V3, outward: V3, half_width: f64 },
    /// Side: the mouth disc centred on the cushion line.
    Disc { c: V3, r: f64 },
}

impl DropShape {
    /// Continuous function whose zero is the drop boundary: positive inside
    /// the drop region (pocket side). Root-found in the event loop.
    pub fn f(&self, p: V3) -> f64 {
        match self {
            DropShape::ChordPlane { m, outward, .. } => (p - *m).dot(*outward),
            DropShape::Disc { c, r } => *r - (p - *c).len(),
        }
    }
    /// Lateral-in-bound check applied at the crossing instant.
    pub fn lateral_ok(&self, p: V3) -> bool {
        match self {
            DropShape::ChordPlane {
                m,
                outward,
                half_width,
            } => lateral(*m, *outward, p).abs() <= *half_width,
            DropShape::Disc { .. } => true,
        }
    }
}

fn lateral(m: V3, outward: V3, p: V3) -> f64 {
    let t = v3(-outward.y, outward.x, 0.0);
    (p - m).dot(t)
}

#[derive(Clone, Debug)]
pub struct Pocket {
    pub id: usize,
    pub corner: bool,
    pub tips: [V3; 2],
    pub shape: DropShape,
    pub mouth_center: V3,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub walls: Vec<Wall>,
    pub pockets: Vec<Pocket>,
    /// Point colliders: pocket jaw tips (sharp in this prototype).
    pub tips: Vec<V3>,
}

/// Rack slot centre (#14 / rack-fixtures invariant): row r = 1..5 apex-first,
/// index k = 0..r-1 ascending from the y-negative end.
pub fn rack_slot(r: usize, k: usize) -> V3 {
    let three_sqrt = 1.732_050_807_568_877_2_f64; // sqrt(3), literal per #14
    v3(
        FOOT_SPOT_X + (r as f64 - 1.0) * three_sqrt * R,
        (k as f64 - (r as f64 - 1.0) / 2.0) * 2.0 * R,
        0.0,
    )
}

/// "1.0" -> (1, 0)
pub fn parse_slot(s: &str) -> (usize, usize) {
    let mut it = s.split('.');
    let r: usize = it.next().unwrap().parse().unwrap();
    let k: usize = it.next().unwrap().parse().unwrap();
    (r, k)
}

impl Table {
    pub fn new() -> Table {
        let mut walls = Vec::new();
        let mut pockets = Vec::new();
        let mut tips = Vec::new();

        let d_corner = MOUTH_CORNER / 2.0_f64.sqrt(); // tip offset along each rail
        let half_corner = MOUTH_CORNER / 2.0;
        let half_side = MOUTH_SIDE / 2.0;

        // ---- long rails (y = +-635): two segments each, split by the side pocket
        for sy in [-1.0f64, 1.0] {
            let y = sy * HALF_WID;
            let n = v3(0.0, -sy, 0.0);
            for (x0, x1) in [
                (-HALF_LEN + d_corner, -half_side),
                (half_side, HALF_LEN - d_corner),
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
        // ---- short rails (x = +-1270): one segment each
        for sx in [-1.0f64, 1.0] {
            let x = sx * HALF_LEN;
            let n = v3(-sx, 0.0, 0.0);
            walls.push(Wall {
                p: v3(x, -HALF_WID + d_corner, 0.0),
                n,
                t: v3(0.0, 1.0, 0.0),
                len: 2.0 * (HALF_WID - d_corner),
                rail: Some(if sx > 0.0 {
                    Rail::FootShort
                } else {
                    Rail::HeadShort
                }),
                pocket: None,
            });
        }

        // ---- pockets
        // corner cutting angle: the jaw face leaves the rail at (180 - cut) deg
        let theta_corner = (180.0 - CUT_CORNER_DEG).to_radians();
        let theta_side = (180.0 - CUT_SIDE_DEG).to_radians();

        // The playable side of a jaw face is the side holding the pocket's
        // mouth centre (a point in the playing area); the face segment then
        // runs into the pocket. Selecting the normal this way is correct for
        // both corner and side jaws (the rail-inward test is not).
        let mut add_jaw = |tip: V3, dir: V3, mouth_center: V3, pocket_id: usize, walls: &mut Vec<Wall>| {
            let cand = [v3(-dir.y, dir.x, 0.0), v3(dir.y, -dir.x, 0.0)];
            let to_mouth = mouth_center - tip;
            let n = if cand[0].dot(to_mouth) > 0.0 {
                cand[0]
            } else {
                cand[1]
            };
            walls.push(Wall {
                p: tip,
                n,
                t: dir,
                len: 220.0,
                rail: None,
                pocket: Some(pocket_id),
            });
        };

        for (sx, sy) in [(1.0f64, 1.0f64), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let a = v3(sx * (HALF_LEN - d_corner), sy * HALF_WID, 0.0);
            let b = v3(sx * HALF_LEN, sy * (HALF_WID - d_corner), 0.0);
            let corner = v3(sx * HALF_LEN, sy * HALF_WID, 0.0);
            let m = (a + b) / 2.0;
            let outward = (corner - m).norm();
            let id = pockets.len();
            pockets.push(Pocket {
                id,
                corner: true,
                tips: [a, b],
                shape: DropShape::ChordPlane {
                    m,
                    outward,
                    half_width: half_corner,
                },
                mouth_center: m,
            });
            // jaw A: leaves the long rail
            let dir_a = v3(sx * theta_corner.cos(), sy * theta_corner.sin(), 0.0);
            add_jaw(a, dir_a, m, id, &mut walls);
            // jaw B: leaves the short rail, same 142 deg cut angle
            let dir_b = v3(sx * theta_corner.sin(), sy * theta_corner.cos(), 0.0);
            add_jaw(b, dir_b, m, id, &mut walls);
            tips.push(a);
            tips.push(b);
        }
        for sy in [-1.0f64, 1.0] {
            let c = v3(0.0, sy * HALF_WID, 0.0);
            let id = pockets.len();
            pockets.push(Pocket {
                id,
                corner: false,
                tips: [v3(-half_side, sy * HALF_WID, 0.0), v3(half_side, sy * HALF_WID, 0.0)],
                shape: DropShape::Disc { c, r: half_side },
                mouth_center: c,
            });
            for sx in [-1.0f64, 1.0] {
                let tip = v3(sx * half_side, sy * HALF_WID, 0.0);
                let dir = v3(sx * theta_side.cos(), sy * theta_side.sin(), 0.0);
                add_jaw(tip, dir, c, id, &mut walls);
                tips.push(tip);
            }
        }

        Table { walls, pockets, tips }
    }

    /// True when a ball at rest would be "on the playing surface" for the
    /// purpose of the frame: used only by diagnostics.
    pub fn inside_playing_area(&self, p: V3) -> bool {
        p.x.abs() <= HALF_LEN + 1e-6 && p.y.abs() <= HALF_WID + 1e-6
    }
}
