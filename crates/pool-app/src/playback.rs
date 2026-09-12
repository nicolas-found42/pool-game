//! The shot timeline (`architecture.md` §10's `playback.rs`): the presentation clock, the exact
//! `state_at(t)` sample per frame, the snap to rest, and the handoff to the next state.
//!
//! There is no interpolation and no `FixedUpdate` (§4): `Shot::state_at` is an analytic evaluation at
//! any instant, so the clock advances by the frame's delta and the sample is exact wherever the frame
//! lands. The clock never feeds game state — the shot was computed to rest in one call before the
//! clock started (§4), and the shell's route back in is a logged input.

use bevy::prelude::*;
use pool_sim::Shot;

use crate::bridge::BallStates;
use crate::session::Game;

/// The presentation clock (`architecture.md` §10).
#[derive(Resource, Default)]
pub struct Playback {
    /// The shot on screen, if one is being presented.
    shot: Option<Shot>,
    /// Seconds since the strike.
    t_s: f64,
    /// Whether the clock advances. A held clock (the screenshot walk) presents a still frame.
    running: bool,
}

impl Playback {
    /// Present `shot` from `t = 0`, clock running.
    pub fn begin(&mut self, shot: Shot) {
        self.t_s = 0.0;
        self.running = true;
        self.shot = Some(shot);
    }

    /// Present `shot` at `t_s`, clock held — the screenshot walk's in-flight frame.
    pub fn hold(&mut self, shot: Shot, t_s: f64) {
        self.t_s = t_s.clamp(0.0, shot.t_rest_s());
        self.running = false;
        self.shot = Some(shot);
    }

    /// Whether the table is at rest: nothing is being presented, so authoring is open.
    #[must_use]
    pub fn at_rest(&self) -> bool {
        self.shot.is_none()
    }

    /// The clock (s since the strike).
    #[must_use]
    pub const fn t_s(&self) -> f64 {
        self.t_s
    }

    /// The presented shot's duration (s), when one is on screen.
    #[must_use]
    pub fn t_rest_s(&self) -> Option<f64> {
        self.shot.as_ref().map(Shot::t_rest_s)
    }
}

/// Advance the presentation clock and sample the shot into the shell's ball states.
///
/// A frame past the shot's rest time snaps to the rest block and hands off: the shot is dropped, and
/// the states come from the session, which holds the same rest positions (the loop wrote them when the
/// shot was adjudicated). Sampling is exact at every other instant — no interpolation.
fn advance(
    time: Res<Time>,
    game: Res<Game>,
    mut playback: ResMut<Playback>,
    mut states: ResMut<BallStates>,
) {
    if let Some(shot) = playback.shot.take() {
        let t_rest = shot.t_rest_s();
        playback.t_s = (playback.t_s + f64::from(time.delta_secs())).min(t_rest);
        if playback.t_s < t_rest {
            states.0 = shot.state_at(playback.t_s);
            playback.shot = Some(shot);
        } else {
            // The snap to rest, and the handoff: from here the session owns the position.
            playback.running = false;
            states.0 = *game.positions();
        }
        return;
    }
    // At rest: the session's position, which covers a placement and a spot as well as a shot's rest.
    states.0 = *game.positions();
}

/// The playback systems (`architecture.md` §10: `Update`, never `FixedUpdate`).
pub fn systems(app: &mut App) {
    app.init_resource::<Playback>().add_systems(Update, advance);
}
