//! The ball-drop test (`physics.md` §12): the measurement that settles `e_slate`.
//!
//! `physics.md` §3.4/§5 carries `e_slate` at 0.60 with **no measured band**, pinned by a ball-drop
//! test: drop a ball onto the slate from a known height and read back `e = √(h_rebound / h_drop)`.
//! The model has no hand that lifts a ball — its only way to a height is a strike — so the drop is
//! made with a near-vertical launch: the cue ball is struck at 89.999° so that its flight is
//! effectively vertical (horizontal component 0.05 mm/s), rises to a known apex, and falls back onto
//! the cloth under gravity alone. Both heights are then read from the *returned* `Shot`:
//! `h_drop` at the first flight's apex and `h_rebound` at the first rebound's apex, each at the
//! ballistic apex time of the state the run reports (`v_z / g`), so nothing is sampled and nothing
//! re-derives the launch from the declaration.
//!
//! The run must also report the `e_slate` it ran at (`ShotOutcome::e_slate`, `physics.md` §3.4), and
//! this test fails if the profile's value and the reported one ever drift apart.

use pool_sim::constants::{BALL_RADIUS_MM, GRAVITY_MM_S2};
use pool_sim::facts::{FactKind, KickCause};
use pool_sim::profile::DEFAULT_E_SLATE;
use pool_sim::rack;
use pool_sim::sim::{Shot, Sim};
use pool_sim::strike::StrikeDecl;
use pool_sim::table::Table;

/// The drop's launch speed (mm/s): its vertical share sets the drop height on the ballistic arc.
const LAUNCH_SPEED_MM_S: f64 = 3000.0;
/// The launch's elevation (degrees): near-vertical, so the drop is a drop and not a throw.
const ELEVATION_DEG: f64 = 89.999;
/// The measurement's tolerance. The chain is exact arithmetic (ballistic flight, one restitution at
/// the landing, one square root), so the model's own resolution is rounding, not a physical band.
const TOLERANCE: f64 = 1e-9;

fn drop_shot() -> Shot {
    // The cue ball drops in the middle of the table, far from the rack and every mouth.
    let mut sim = Sim::new(Table::new(), rack::generate(1));
    sim.place_cue([0.0, 0.0])
        .expect("the drop position is clear");
    let decl = StrikeDecl {
        aim: [1.0, 0.0],
        speed_mm_s: LAUNCH_SPEED_MM_S,
        spin: [0.0, 0.0],
        elevation_rad: ELEVATION_DEG.to_radians(),
    };
    sim.strike(decl)
        .expect("a near-vertical launch is a valid strike")
}

/// The rise of the cue ball's centre above its resting height at the apex `tau` seconds after `t0`.
fn apex_rise_mm(shot: &Shot, t0: f64) -> f64 {
    let state = shot.state_at(t0)[0];
    let vz = state.vel_mm_s[2];
    assert!(vz > 0.0, "the apex is measured from a rising state");
    let apex_t = t0 + vz / GRAVITY_MM_S2;
    shot.state_at(apex_t)[0].pos_mm[2] - BALL_RADIUS_MM
}

/// The first landing of the cue ball, from the fact stream (`physics.md` §3.7/§7).
fn first_landing_s(shot: &Shot) -> f64 {
    shot.events()
        .iter()
        .find(|fact| {
            matches!(
                fact.kind,
                FactKind::Kick {
                    ball: 0,
                    cause: KickCause::Landing
                }
            )
        })
        .expect("the ball lands")
        .t
}

#[test]
fn the_ball_drop_pins_e_slate() {
    let shot = drop_shot();
    let e_slate = shot.outcome().e_slate;
    assert_eq!(
        e_slate, DEFAULT_E_SLATE,
        "the run must report the profile's e_slate"
    );

    let h_drop = apex_rise_mm(&shot, 0.0);
    let t_land = first_landing_s(&shot);
    let h_rebound = apex_rise_mm(&shot, t_land);
    let measured = (h_rebound / h_drop).sqrt();

    // The run's own max-hop measurement is an independent read of the same drop height.
    assert!(
        (shot.outcome().max_hop_mm - h_drop).abs() < TOLERANCE,
        "the drop height read at the apex ({h_drop} mm) and the run's max hop ({} mm) disagree",
        shot.outcome().max_hop_mm
    );
    println!(
        "ball-drop test at e_slate {e_slate} (reported by the run): dropped {h_drop} mm, rebounded \
         {h_rebound} mm, measured e = {measured}"
    );
    assert!(
        (measured - e_slate).abs() < TOLERANCE,
        "the ball-drop test ran at e_slate {e_slate} and measured e = {measured} \
         (h_drop {h_drop} mm, h_rebound {h_rebound} mm, landing at t = {t_land} s)"
    );
    assert!(
        h_rebound < h_drop,
        "a rebound taller than its drop ({h_rebound} ≥ {h_drop} mm) is not a recovery"
    );
}
