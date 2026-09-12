//! The contact-validity property the ball–ball solver must not lose (`physics.md` §1/§3.2): a
//! contact event is only ever emitted where the two balls are actually in contact, and a pair that
//! is approaching inside its laws does meet.
//!
//! Both halves are regression pins on the phantom-contact repair. The first: a candidate root solved
//! on a segment law extrapolated **past its validity** (`t ≈ 1.15 s` on a sliding law that ends at
//! `0.71 s`) was refined out of the root's basin, a `t <= 0` clamp turned the step into `t = 1e-9`,
//! and `hit_pair` applied a full impulse to `BallBall { a: 0, b: 6 }` with the two centres
//! **1633.97 mm** apart — on the ticket's own repro, whose first simultaneity group was those two
//! phantom pairs. The second: the mirror image — a genuine approach whose root the refinement has
//! not converged (a fast sliding pair's residual after four Newton steps is a few nanometres, above
//! the contact slop) must not be *read* as a miss, because the pair then passes through itself
//! between events and no contact fact is emitted at all.
//!
//! The sample shots are the ticket's repro (the standard rack struck at 1400 mm/s down the long
//! string) and the break hook's break at 6200 mm/s, plus the TP B-8 survey row whose cue ball is
//! fast and heavily spun.

use pool_sim::constants::BALL_DIAMETER_MM;
use pool_sim::facts::FactKind;
use pool_sim::math::v3;
use pool_sim::profile::Profile;
use pool_sim::rack;
use pool_sim::sim::{Shot, Sim};
use pool_sim::strike::StrikeDecl;
use pool_sim::table::{DropShape, Pocket, PocketId, Table};

/// The solver's contact slop (mm), `sim.rs`'s private `CONTACT_SLOP_MM`: the scale at which two
/// surfaces are in contact, and the tolerance every contact fact must satisfy. Repeated here rather
/// than exported, because it is a solver-internal rounding guard, not a model constant.
const CONTACT_SLOP_MM: f64 = 1e-6;

/// The rack fixture seed (`docs/spec/rack-fixtures.json` seed 1, the break rack) and the cue ball's
/// standard break position (mm).
const RACK_SEED: u64 = 1;
const CUE_POS_MM: [f64; 2] = [-800.0, 0.0];

/// The ticket's repro: a level, spin-free strike straight down the long string.
fn repro_shot() -> Shot {
    let mut sim = Sim::new(Table::new(), rack::generate(RACK_SEED));
    sim.place_cue(CUE_POS_MM).expect("cue placement");
    sim.strike(StrikeDecl {
        aim: [1.0, 0.0],
        speed_mm_s: 1400.0,
        spin: [0.0, 0.0],
        elevation_rad: 0.0,
    })
    .expect("a level centre strike is valid")
}

/// The break hook's break (`tests/break_hook.rs`), where the phantom set moved which balls reached
/// rails.
fn break_shot() -> Shot {
    let mut sim = Sim::new(Table::new(), rack::generate(RACK_SEED));
    sim.place_cue(CUE_POS_MM).expect("cue placement");
    sim.strike(StrikeDecl {
        aim: [1.0, 0.0],
        speed_mm_s: 6200.0,
        spin: [0.0, 0.0],
        elevation_rad: 0.0,
    })
    .expect("a level centre strike is valid")
}

/// The centre-to-centre distance (mm) of a pair's states at a fact's instant.
fn distance_at(shot: &Shot, t: f64, a: u8, b: u8) -> f64 {
    let states = shot.state_at(t);
    let (pa, pb) = (states[a as usize].pos_mm, states[b as usize].pos_mm);
    let (dx, dy, dz) = (pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Every `BallBall` fact of a shot is a real contact at its own instant: the pair's surfaces are
/// within the contact slop — an air gap is the phantom, a deep overlap is a pass-through.
///
/// The approach test is deliberately **not** asserted here: a fact is stamped at the instant the
/// impulse is applied, and `state_at` reads the segment installed *after* it, where the pair is
/// separating by construction (`v_n` reversed by the restitution). Approach is what the candidate
/// filter requires before an event is proposed at all, not a property of the post-impulse state.
fn assert_ball_ball_facts_are_contacts(shot: &Shot, label: &str) {
    let mut checked = 0_u32;
    for fact in shot.events() {
        let FactKind::BallBall { a, b } = fact.kind else {
            continue;
        };
        let distance = distance_at(shot, fact.t, a, b);
        let gap = distance - BALL_DIAMETER_MM;
        assert!(
            gap.abs() <= CONTACT_SLOP_MM,
            "{label}: BallBall {a}–{b} at t = {} is not a contact: centres {distance} mm apart \
             (gap {gap} mm)",
            fact.t
        );
        checked += 1;
    }
    assert!(checked > 0, "{label}: the shot emitted no ball–ball fact");
}

/// A free table: no rails, no jaws, no pockets — the ball–ball channel alone (`ladder.rs`'s own
/// `free_table`).
fn free_table(profile: &Profile) -> Table {
    let mut table = Table::with_profile(profile.clone());
    table.walls.clear();
    table.tips.clear();
    table.pockets = std::array::from_fn(|i| Pocket {
        id: PocketId::ALL[i],
        shape: DropShape::Disc {
            centre: v3(1.0e6, 1.0e6, 0.0),
            radius: 0.0,
        },
        mouth_center: v3(0.0, 0.0, 0.0),
    });
    table
}

#[test]
fn every_ball_ball_fact_is_a_contact_at_its_instant() {
    assert_ball_ball_facts_are_contacts(&repro_shot(), "repro");
    assert_ball_ball_facts_are_contacts(&break_shot(), "break");
}

#[test]
fn a_fast_spinning_cue_ball_meets_the_object_ball() {
    // TP B-8's survey row `cb_speed 4.859 m/s, cb_spin -106.277 rad/s, drag 1.2192 m`: the pair's
    // root needs the refinement's full convergence, because a fast sliding cue ball's curvature is
    // large over the approach. Four Newton steps leave a residual above the contact slop, and a
    // solver that reads that residual as a miss never reports the contact — the balls pass through
    // each other between events (measured: two events, no `BallBall` fact, the object ball at rest
    // where it started).
    let (speed, spin, drag) = (4859.0, -106.277, 1219.2);
    let profile = Profile::default_profile();
    let mut sim = Sim::cleared(free_table(&profile));
    sim.place_ball(0, [-drag / 2.0, 0.0])
        .expect("pair placement");
    sim.place_ball(1, [drag / 2.0, 0.0])
        .expect("pair placement");
    let shot = sim.launch(0, v3(speed, 0.0, 0.0), v3(0.0, spin, 0.0));

    let contact = shot
        .events()
        .iter()
        .find(|fact| matches!(fact.kind, FactKind::BallBall { .. }))
        .expect("the cue ball meets the object ball");
    let gap = distance_at(&shot, contact.t, 0, 1) - BALL_DIAMETER_MM;
    assert!(
        gap.abs() <= CONTACT_SLOP_MM,
        "the contact at t = {} is not a contact: gap {gap} mm",
        contact.t
    );
    // The impulse lands: the object ball leaves its start, and the pair never overlaps at a fact
    // instant (a pass-through would leave the cue ball beyond it with the object ball untouched).
    let rest = shot.rest();
    let object_rest = rest.states[1].pos_mm[0];
    let start = -drag / 2.0 + drag;
    assert!(
        object_rest > start + BALL_DIAMETER_MM,
        "the object ball did not take the impulse: it rests at {object_rest} mm, started at {start}"
    );
}
