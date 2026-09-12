//! The table's visuals (`architecture.md` §10's `render.rs`): the playing surface, the cushions with
//! their six pocket gaps, the pockets, the markings, and the ball faces.
//!
//! World units are millimetres and the view is top-down, so the sim's frame (`physics.md` §2) *is*
//! the scene: `pool_sim::constants` supplies every geometric value, converted once at this module's
//! `f32` boundary. The shell's display math is exempt from the numeric discipline (§3), and nothing
//! here can feed game state — the sync runs the other way (`§10`).
//!
//! §10 also lists the aim-line and ghost-ball gizmos for this module; they are drawn from a
//! declaration being authored, so they arrive with the input slice.

use bevy::prelude::*;
use pool_sim::constants as c;

// ---------------------------------------------------------------------------
// The frame (`physics.md` §2/§5), converted once into the shell's f32 world.

/// Half the playing surface's length (mm): x runs head → foot through the origin, at the centre.
const HALF_LEN: f32 = c::HALF_LEN_MM as f32;
/// Half the playing surface's width (mm).
const HALF_WIDTH: f32 = c::HALF_WIDTH_MM as f32;
/// Ball radius (mm).
pub const BALL_R: f32 = c::BALL_RADIUS_MM as f32;
/// The corner pocket's mouth (mm).
const MOUTH_CORNER: f32 = c::POCKET_MOUTH_CORNER_MM as f32;
/// The side pocket's mouth (mm).
const MOUTH_SIDE: f32 = c::POCKET_MOUTH_SIDE_MM as f32;
/// The head string (mm): the quarter line the cue ball is parked behind at rest.
const HEAD_STRING_X: f32 = c::HEAD_STRING_X_MM as f32;
/// The foot spot (mm): the rack's apex.
const FOOT_SPOT_X: f32 = c::FOOT_SPOT_X_MM as f32;

// ---------------------------------------------------------------------------
// Display-only geometry. `pool_sim::constants` owns the model's frame, which has nose lines and
// pocket mouths but no cushion, rail, or marking widths; the ones below are the look.

/// A cushion's drawn width, from the nose line outward (mm).
const CUSHION_W: f32 = 50.0;
/// The wood frame's outer edge, measured from the nose line (mm).
const FRAME_W: f32 = 90.0;
/// The head string's drawn width (mm).
const MARKING_W: f32 = 6.0;
/// The foot spot's drawn radius (mm).
const SPOT_R: f32 = 8.0;
/// The distance the camera keeps beyond the wood frame, as a fraction of the table's extent.
const VIEW_MARGIN: f32 = 1.05;

/// A stripe ball's band height as a fraction of the ball's diameter.
const BAND_H: f32 = 0.55;
/// The number disc's radius (mm): two digits fit inside it.
const NUMBER_R: f32 = 17.0;
/// The number's em size. `Text2d` sizes are world units, and the world here is millimetres.
const NUMBER_FONT: f32 = 30.0;

/// Painter's order: the view is a flat stack on the world plane, sorted by z.
const Z_RAIL: f32 = -1.0;
const Z_CLOTH: f32 = 0.0;
const Z_MARKING: f32 = 0.05;
const Z_CUSHION: f32 = 0.1;
const Z_POCKET: f32 = 0.2;
/// The balls' layer. The bridge spawns them here and the sync writes only x/y.
pub const Z_BALL: f32 = 1.0;
/// Above a ball's own disc: its stripe band, its number disc, and its number.
const Z_BAND: f32 = 0.01;
const Z_NUMBER_DISC: f32 = 0.02;
const Z_NUMBER: f32 = 0.03;

/// The palette: cloth, wood, cushion, pocket, marking, and the two ball-face whites.
const CLOTH: Color = Color::srgb(0.05, 0.30, 0.18);
const WOOD: Color = Color::srgb(0.26, 0.16, 0.09);
const CUSHION: Color = Color::srgb(0.07, 0.36, 0.22);
const POCKET: Color = Color::srgb(0.02, 0.02, 0.03);
const MARKING: Color = Color::srgb(0.52, 0.70, 0.58);
const WHITE: Color = Color::srgb(0.96, 0.96, 0.94);
const NUMBER_INK: Color = Color::srgb(0.05, 0.05, 0.06);

/// The vertical world size the top-down camera shows: the table's outer extent (wood included) plus
/// [`VIEW_MARGIN`], grown further if the window's aspect would otherwise crop the length.
#[must_use]
pub fn viewport_height(window_aspect: f32) -> f32 {
    let outer_w = 2.0 * (HALF_LEN + FRAME_W);
    let outer_h = 2.0 * (HALF_WIDTH + FRAME_W);
    (outer_w * VIEW_MARGIN / window_aspect).max(outer_h * VIEW_MARGIN)
}

// ---------------------------------------------------------------------------
// The table

/// Spawn the table: the wood slab, the cloth, the six cushion segments, the six pockets, and the
/// markings (the head string and the foot spot, `physics.md` §2).
pub fn spawn_table(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    // A corner mouth's gap is shared by the two rails that meet there, so each is cut back by half
    // the mouth's constant; a side mouth's gap sits wholly on one rail. The long rails therefore run
    // between a corner gap and the side gap, and the short rails between their two corner gaps.
    let corner_half = MOUTH_CORNER / 2.0;
    let side_half = MOUTH_SIDE / 2.0;
    let long_len = HALF_LEN - corner_half - side_half;
    let long_centre = side_half + long_len / 2.0;
    let short_len = 2.0 * (HALF_WIDTH - corner_half);
    let long_y = HALF_WIDTH + CUSHION_W / 2.0;
    let short_x = HALF_LEN + CUSHION_W / 2.0;

    let slabs: [(Color, Vec2, Vec2, f32); 9] = [
        (
            WOOD,
            Vec2::ZERO,
            Vec2::new(2.0 * (HALF_LEN + FRAME_W), 2.0 * (HALF_WIDTH + FRAME_W)),
            Z_RAIL,
        ),
        (
            CLOTH,
            Vec2::ZERO,
            Vec2::new(2.0 * HALF_LEN, 2.0 * HALF_WIDTH),
            Z_CLOTH,
        ),
        (
            MARKING,
            Vec2::new(HEAD_STRING_X, 0.0),
            Vec2::new(MARKING_W, 2.0 * HALF_WIDTH),
            Z_MARKING,
        ),
        (
            CUSHION,
            Vec2::new(-long_centre, long_y),
            Vec2::new(long_len, CUSHION_W),
            Z_CUSHION,
        ),
        (
            CUSHION,
            Vec2::new(long_centre, long_y),
            Vec2::new(long_len, CUSHION_W),
            Z_CUSHION,
        ),
        (
            CUSHION,
            Vec2::new(-long_centre, -long_y),
            Vec2::new(long_len, CUSHION_W),
            Z_CUSHION,
        ),
        (
            CUSHION,
            Vec2::new(long_centre, -long_y),
            Vec2::new(long_len, CUSHION_W),
            Z_CUSHION,
        ),
        (
            CUSHION,
            Vec2::new(-short_x, 0.0),
            Vec2::new(CUSHION_W, short_len),
            Z_CUSHION,
        ),
        (
            CUSHION,
            Vec2::new(short_x, 0.0),
            Vec2::new(CUSHION_W, short_len),
            Z_CUSHION,
        ),
    ];
    for (colour, centre, size, z) in slabs {
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_xyz(centre.x, centre.y, z),
        ));
    }

    let pockets: [(Vec2, f32); 6] = [
        (Vec2::new(-HALF_LEN, -HALF_WIDTH), corner_half),
        (Vec2::new(HALF_LEN, -HALF_WIDTH), corner_half),
        (Vec2::new(-HALF_LEN, HALF_WIDTH), corner_half),
        (Vec2::new(HALF_LEN, HALF_WIDTH), corner_half),
        (Vec2::new(0.0, -HALF_WIDTH), side_half),
        (Vec2::new(0.0, HALF_WIDTH), side_half),
    ];
    for (centre, radius) in pockets {
        commands.spawn((
            Mesh2d(meshes.add(Circle::new(radius))),
            MeshMaterial2d(materials.add(POCKET)),
            Transform::from_xyz(centre.x, centre.y, Z_POCKET),
        ));
    }

    // The foot spot, under the rack's apex ball.
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(SPOT_R))),
        MeshMaterial2d(materials.add(MARKING)),
        Transform::from_xyz(FOOT_SPOT_X, 0.0, Z_MARKING),
    ));
}

// ---------------------------------------------------------------------------
// The balls

/// Add one ball's face to `parent`: its disc, its stripe band (9–15 only), its number disc, and its
/// number. The caller owns the entity's transform — the bridge puts it where the sim says, and this
/// only decorates it.
pub fn add_ball_face(
    parent: &mut ChildSpawnerCommands,
    ball: u8,
    disc: &Handle<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let surface = ball_color(ball);
    let striped = (9..=15).contains(&ball);
    parent.spawn((
        Mesh2d(disc.clone()),
        MeshMaterial2d(materials.add(if striped { WHITE } else { surface })),
    ));
    if ball == 0 {
        return; // the cue ball is plain: no band, no number
    }
    if striped {
        // Viewed straight down, a stripe is a band across the face at full width.
        parent.spawn((
            Mesh2d(disc.clone()),
            MeshMaterial2d(materials.add(surface)),
            Transform::from_scale(Vec3::new(1.0, BAND_H, 1.0))
                .with_translation(Vec3::new(0.0, 0.0, Z_BAND)),
        ));
    }
    parent.spawn((
        Mesh2d(disc.clone()),
        MeshMaterial2d(materials.add(WHITE)),
        Transform::from_scale(Vec3::splat(NUMBER_R / BALL_R)).with_translation(Vec3::new(
            0.0,
            0.0,
            Z_NUMBER_DISC,
        )),
    ));
    parent.spawn((
        Text2d::new(ball.to_string()),
        TextFont::from_font_size(NUMBER_FONT),
        TextColor(NUMBER_INK),
        Transform::from_xyz(0.0, 0.0, Z_NUMBER),
    ));
}

/// The classic palette: solids 1–8 (8 is black), stripes 9–15 in 1–7's colours, the cue white.
#[must_use]
fn ball_color(ball: u8) -> Color {
    match ball {
        1 | 9 => Color::srgb(0.94, 0.78, 0.10),  // yellow
        2 | 10 => Color::srgb(0.10, 0.28, 0.72), // blue
        3 | 11 => Color::srgb(0.78, 0.12, 0.12), // red
        4 | 12 => Color::srgb(0.42, 0.18, 0.62), // purple
        5 | 13 => Color::srgb(0.90, 0.45, 0.08), // orange
        6 | 14 => Color::srgb(0.10, 0.50, 0.22), // green
        7 | 15 => Color::srgb(0.50, 0.10, 0.16), // maroon
        8 => Color::srgb(0.06, 0.06, 0.07),      // black
        _ => WHITE,                              // the cue ball (0)
    }
}
