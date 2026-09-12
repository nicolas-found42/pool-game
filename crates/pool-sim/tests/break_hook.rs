//! The break acceptance hook (`physics.md` §6): the one gate that exercises the whole model.
//!
//! A level-cue break from the standard rack with the prototype's pinned parameters (cue at
//! (−800, 0) mm, 6200 mm/s, no spin, level) must show:
//!
//! - **no tunnelling** — the largest cushion penetration over the shot stays inside the stated bound
//!   (contacts are solved, never sampled, so the residual is rounding);
//! - **bounded energy** — no gain over the shot, and no rise between consecutive timeline segments;
//! - **at least four distinct object balls driven to a rail** — the physical count of `physics.md` §7;
//! - **a bit-identical rerun** — the same declaration twice, compared by `state_hash`;
//! - and it reports the `e_slate` it ran at, plus the ceiling note: a level-cue break does not
//!   off-table a ball at the default `e_slate`.

use pool_sim::ball::BallState;
use pool_sim::constants::CUSHION_NOSE_HEIGHT_MM;
use pool_sim::facts::rail_summary;
use pool_sim::rack;
use pool_sim::sim::{Shot, Sim};
use pool_sim::state_hash::state_hash;
use pool_sim::strike::StrikeDecl;
use pool_sim::table::Table;

/// The rack's fixture seed: `docs/spec/rack-fixtures.json` seed 1, the prototype's break rack.
const RACK_SEED: u64 = 1;
/// The cue ball's standard break position (mm), above the head string on the long string.
const CUE_POS_MM: [f64; 2] = [-800.0, 0.0];
/// The level-cue break's launch speed (mm/s) and aim (down the long string into the apex ball).
const LAUNCH_SPEED_MM_S: f64 = 6200.0;
const AIM: [f64; 2] = [1.0, 0.0];
/// The penetration bound (mm): 1 µm — three orders below a rounded contact's own footprint.
const MAX_PENETRATION_MM: f64 = 1e-6;
/// The ball–ball gap bound (mm): the rack's exact-contact lattice sits within rounding of zero, so a
/// negative gap is tolerated only at that scale.
const MIN_GAP_TOLERANCE_MM: f64 = 1e-6;
/// The largest relative energy rise allowed between consecutive timeline segments.
const MAX_ENERGY_RISE: f64 = 1e-9;

fn break_strike() -> StrikeDecl {
    StrikeDecl {
        aim: AIM,
        speed_mm_s: LAUNCH_SPEED_MM_S,
        spin: [0.0, 0.0],
        elevation_rad: 0.0,
    }
}

fn run_break() -> Shot {
    let table = Table::new();
    let mut sim = Sim::new(table, rack::generate(RACK_SEED));
    sim.place_cue(CUE_POS_MM)
        .expect("the standard break position is on the table and clear of the rack");
    sim.strike(break_strike())
        .expect("a level centre strike is legal")
}

fn total_energy_j(shot: &Shot, t: f64) -> f64 {
    shot.state_at(t).iter().map(BallState::total_energy_j).sum()
}

/// The largest relative rise of the total energy between consecutive timeline segments.
fn max_relative_energy_rise(shot: &Shot) -> f64 {
    let initial = total_energy_j(shot, 0.0);
    let mut previous = initial;
    let mut worst: f64 = 0.0;
    for segment in shot.timeline() {
        let energy = total_energy_j(shot, segment.t);
        let rise = (energy - previous) / initial;
        if rise > worst {
            worst = rise;
        }
        previous = energy;
    }
    worst
}

#[test]
fn the_break_hook_holds() {
    let shot = run_break();
    let outcome = shot.outcome();
    let rest = shot.rest();
    let rails = rail_summary(shot.events(), &rest.states);

    println!(
        "break hook: rack seed {RACK_SEED}, cue {CUE_POS_MM:?} at {LAUNCH_SPEED_MM_S} mm/s, level, \
         no spin; e_slate = {:.2}",
        outcome.e_slate
    );
    println!(
        "  rest: {:.3} s, {} events in {} groups, runaway {:?}",
        outcome.t_rest_s, outcome.events, outcome.groups, outcome.runaway
    );
    println!(
        "  penetration: max {:.9} mm, min ball–ball gap {:.6} mm, max hop {:.2} mm",
        outcome.max_penetration_mm, outcome.min_pair_gap_mm, outcome.max_hop_mm
    );
    println!(
        "  rails: {} counted contacts over {} distinct object balls ({} suppressed as frozen), \
         pocketed {:?}, off-table {:?}",
        rails.contacts,
        rails.distinct_object_balls,
        rails.suppressed_frozen,
        rest.pocketed,
        rest.off_table
    );
    let initial = total_energy_j(&shot, 0.0);
    let at_rest = total_energy_j(&shot, outcome.t_rest_s);
    println!(
        "  energy: {:.4} J -> {:.4} J ({:.1} % dissipated), largest segment rise {:.3e}",
        initial,
        at_rest,
        100.0 * (initial - at_rest) / initial,
        max_relative_energy_rise(&shot)
    );

    // The report carries the coefficient the vertical channel ran at (`physics.md` §6b).
    assert!(
        outcome.e_slate > 0.0 && outcome.e_slate < 1.0,
        "the run must report the e_slate it used, got {}",
        outcome.e_slate
    );

    // The run reaches rest under its own guards.
    assert_eq!(outcome.runaway, None, "the break must reach rest unaided");
    assert!(outcome.t_rest_s > 0.0 && outcome.t_rest_s.is_finite());

    // No tunnelling: contacts are solved, so penetration stays at rounding scale.
    assert!(
        outcome.max_penetration_mm <= MAX_PENETRATION_MM,
        "cushion penetration {:.9} mm exceeds the {MAX_PENETRATION_MM} mm bound",
        outcome.max_penetration_mm
    );
    // No overlap: the ball–ball surface gap never goes meaningfully negative.
    assert!(
        outcome.min_pair_gap_mm >= -MIN_GAP_TOLERANCE_MM,
        "ball–ball gap {:.9} mm is an overlap",
        outcome.min_pair_gap_mm
    );

    // Bounded energy: nothing is gained over the shot, and no segment boundary gains anything.
    assert!(
        at_rest <= initial,
        "energy grew over the shot: {initial} -> {at_rest}"
    );
    let rise = max_relative_energy_rise(&shot);
    assert!(
        rise <= MAX_ENERGY_RISE,
        "energy rose by {rise:e} between timeline segments"
    );

    // At least four distinct object balls to rails (the physical count of `physics.md` §7).
    assert!(
        rails.distinct_object_balls >= 4,
        "only {} distinct object balls reached a rail",
        rails.distinct_object_balls
    );

    // The ceiling note of `physics.md` §6c: a level-cue break does not off-table a ball at the default
    // `e_slate` — the hop stays below the cushion's top.
    assert!(
        rest.off_table.is_empty(),
        "a level-cue break drove {:?} off the table",
        rest.off_table
    );
    assert!(
        outcome.max_hop_mm < CUSHION_NOSE_HEIGHT_MM,
        "the break's hop {:.2} mm clears the {CUSHION_NOSE_HEIGHT_MM} mm cushion top",
        outcome.max_hop_mm
    );

    // A bit-identical rerun: same declaration, same bits.
    let rerun = run_break();
    assert_eq!(
        state_hash(&shot.rest().states),
        state_hash(&rerun.rest().states),
        "the rerun's rest state is not bit-identical"
    );
    assert_eq!(
        shot.t_rest_s().to_bits(),
        rerun.t_rest_s().to_bits(),
        "the rerun's rest time is not bit-identical"
    );
    assert_eq!(
        shot.events().len(),
        rerun.events().len(),
        "the rerun's fact count differs"
    );
}
