//! The bridge (`architecture.md` §10): the ball entities, their `BallIndex` links, and the one-way
//! sync from the simulation's ball states to the ECS transforms.
//!
//! The simulation's numbers are the only source of a position. [`BallStates`] mirrors the sim's ball
//! states, [`sync_ball_transforms`] writes each entity's transform from it, and nothing reads a
//! transform back: the shell reaches game state only through a logged input, and this slice has no
//! session yet (§10).

use bevy::prelude::*;
use pool_sim::BallState;

use crate::render;

/// A ball entity's index in the sim's canonical order (`architecture.md` §11): 0 is the cue ball,
/// 1..=15 the object balls in ball-number order.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BallIndex(pub usize);

/// The shell-side mirror of the sim's ball states, in `architecture.md` §11's order. The playback
/// slice samples `Shot::state_at(t)` into it; the bridge only reads it.
#[derive(Resource, Debug, Clone)]
pub struct BallStates(pub [BallState; 16]);

/// Spawn the sixteen ball entities and install the state mirror the sync reads. Their initial
/// transforms come from `states`, so the first frame is right before the sync has run.
pub fn spawn_balls(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    states: &[BallState; 16],
) {
    let disc = meshes.add(Circle::new(render::BALL_R));
    for (index, state) in states.iter().enumerate() {
        // The loop runs over a 16-element array, so the index is a ball number by construction.
        let ball = index as u8;
        let [x_mm, y_mm, _z_mm] = state.pos_mm;
        let entity = commands
            .spawn((
                BallIndex(index),
                Transform::from_xyz(x_mm as f32, y_mm as f32, render::Z_BALL),
                Visibility::default(),
            ))
            .id();
        commands
            .entity(entity)
            .with_children(|parent| render::add_ball_face(parent, ball, &disc, materials));
    }
    commands.insert_resource(BallStates(*states));
}

/// The one-way sync: write every ball's transform from its sim state, each frame.
///
/// The sim's x/y are the position; its z — the ball's height off the cloth — is dropped, because the
/// top-down view has no third axis. The transform's z is the painter's order instead.
pub fn sync_ball_transforms(
    states: Res<BallStates>,
    mut balls: Query<(&BallIndex, &mut Transform)>,
) {
    for (index, mut transform) in &mut balls {
        let [x_mm, y_mm, _z_mm] = states.0[index.0].pos_mm;
        transform.translation.x = x_mm as f32;
        transform.translation.y = y_mm as f32;
    }
}
