//! The property invariants of `physics.md` §9.2, over representative shots:
//!
//! - no cushion penetration and no ball–ball overlap;
//! - monotone energy dissipation between timeline segments;
//! - finite state at every event and at rest (a NaN is a bug, never a game state);
//! - `state_at` exact at rest:
//! - a ball is pocketed only while it is moving — the drop predicate fires on a crossing, never on a
//!   state (the prototype's own silent-pocket failure);
//! - the cushion's square-hit retention, which is the `e_n` fit's observable and the one check that
//!   tells the ruled 3D tangential channel from the rejected 2D projection.

use pool_sim::ball::BallState;
use pool_sim::facts::FactKind;
use pool_sim::profile::Profile;
use pool_sim::rack::{self, Arrangement};
use pool_sim::sim::{Shot, Sim};
use pool_sim::strike::StrikeDecl;
use pool_sim::table::Table;

/// The penetration bound (mm): 1 µm, three orders below the thinnest modelled gap.
const MAX_PENETRATION_MM: f64 = 1e-6;
/// The ball–ball gap bound (mm): the separation step resolves to a gap of exactly `2R`, and a pair it
/// has just handled can sit up to its own resolution inside contact. Anything past this is an overlap
/// the model never resolved; the rack's exact-contact lattice and the break's own worst gap
/// (`-1.6e-13` mm) are orders below it.
const MIN_GAP_TOLERANCE_MM: f64 = 1e-6;
/// The largest relative energy rise allowed between consecutive timeline segments.
const MAX_ENERGY_RISE: f64 = 1e-9;

/// The shots the invariants run over: the pinned break, plus hard, soft, cut, spin and elevated
/// declarations so the sweep touches every mode and channel.
fn shots() -> Vec<(&'static str, StrikeDecl)> {
    vec![
        (
            "level break",
            StrikeDecl {
                aim: [1.0, 0.0],
                speed_mm_s: 6200.0,
                spin: [0.0, 0.0],
                elevation_rad: 0.0,
            },
        ),
        (
            "hard cut with high english",
            StrikeDecl {
                aim: [0.999_849_999_887_5, 0.017_452_406_076_9],
                speed_mm_s: 4200.0,
                spin: [0.8, 0.4],
                elevation_rad: 0.05,
            },
        ),
        (
            "draw with a raised cue",
            StrikeDecl {
                aim: [0.998_750_234_414_1, -0.049_979_166_479_2],
                speed_mm_s: 3000.0,
                spin: [-0.6, -0.7],
                elevation_rad: 0.35,
            },
        ),
        (
            "slow nudge",
            StrikeDecl {
                aim: [1.0, 0.0],
                speed_mm_s: 220.0,
                spin: [0.0, 0.1],
                elevation_rad: 0.0,
            },
        ),
        (
            "side-spin cut",
            StrikeDecl {
                aim: [0.999_950_001_249_9, 0.009_999_833_336_1],
                speed_mm_s: 1500.0,
                spin: [0.95, 0.3],
                elevation_rad: 0.0,
            },
        ),
    ]
}

fn run(decl: &StrikeDecl, arrangement: &Arrangement) -> Shot {
    let mut sim = Sim::new(Table::new(), *arrangement);
    sim.place_cue([-800.0, 0.0])
        .expect("the standard break position is clear of the rack");
    sim.strike(*decl)
        .expect("the declarations are inside the miscue envelope")
}

fn total_energy_j(shot: &Shot, t: f64) -> f64 {
    shot.state_at(t).iter().map(BallState::total_energy_j).sum()
}

#[test]
fn no_penetration_no_overlap_and_no_runaway() {
    let arrangement = rack::generate(1);
    for (name, decl) in shots() {
        let shot = run(&decl, &arrangement);
        let outcome = shot.outcome();
        println!(
            "{name}: {:.3} s, {} events in {} groups, penetration {:.3e} mm, min gap {:.3e} mm, \
             runaway {:?}",
            outcome.t_rest_s,
            outcome.events,
            outcome.groups,
            outcome.max_penetration_mm,
            outcome.min_pair_gap_mm,
            outcome.runaway
        );
        assert_eq!(
            outcome.runaway, None,
            "{name}: the shot must reach rest under its own guards"
        );
        assert!(
            outcome.max_penetration_mm <= MAX_PENETRATION_MM,
            "{name}: cushion penetration {:.3e} mm",
            outcome.max_penetration_mm
        );
        assert!(
            outcome.min_pair_gap_mm >= -MIN_GAP_TOLERANCE_MM,
            "{name}: balls overlapped by {:.3e} mm",
            -outcome.min_pair_gap_mm
        );
    }
}

#[test]
fn energy_dissipates_monotonically_between_timeline_segments() {
    let arrangement = rack::generate(1);
    for (name, decl) in shots() {
        let shot = run(&decl, &arrangement);
        let initial = total_energy_j(&shot, 0.0);
        let mut previous = initial;
        let mut worst_rise = 0.0_f64;
        for segment in shot.timeline() {
            let energy = total_energy_j(&shot, segment.t);
            let rise = (energy - previous) / initial;
            if rise > worst_rise {
                worst_rise = rise;
            }
            assert!(
                energy <= previous + MAX_ENERGY_RISE * initial,
                "{name}: energy rose from {previous} to {energy} J at t = {}",
                segment.t
            );
            previous = energy;
        }
        let at_rest = total_energy_j(&shot, shot.t_rest_s());
        assert!(
            at_rest <= initial,
            "{name}: the shot gained energy ({initial} -> {at_rest} J)"
        );
        println!("{name}: {initial:.4} J -> {at_rest:.4} J, worst segment rise {worst_rise:.3e}");
    }
}

#[test]
fn the_state_is_finite_at_every_event_and_at_rest() {
    let arrangement = rack::generate(1);
    for (name, decl) in shots() {
        let shot = run(&decl, &arrangement);
        for segment in shot.timeline() {
            for (i, state) in shot.state_at(segment.t).iter().enumerate() {
                assert!(
                    state.pos_mm.iter().all(|v| v.is_finite())
                        && state.vel_mm_s.iter().all(|v| v.is_finite())
                        && state.spin_rad_s.iter().all(|v| v.is_finite()),
                    "{name}: ball {i} is not finite at t = {}: {state:?}",
                    segment.t
                );
            }
        }
        for (i, state) in shot.rest().states.iter().enumerate() {
            assert!(
                state.pos_mm.iter().all(|v| v.is_finite())
                    && state.vel_mm_s.iter().all(|v| v.is_finite())
                    && state.spin_rad_s.iter().all(|v| v.is_finite()),
                "{name}: ball {i} is not finite at rest: {state:?}"
            );
        }
    }
}

#[test]
fn state_at_is_exact_at_rest() {
    let arrangement = rack::generate(1);
    for (name, decl) in shots() {
        let shot = run(&decl, &arrangement);
        let at_rest = shot.state_at(shot.t_rest_s());
        for (i, (evaluated, stored)) in at_rest.iter().zip(shot.rest().states.iter()).enumerate() {
            assert_eq!(
                evaluated, stored,
                "{name}: ball {i}'s evaluated state at rest differs from the rest block"
            );
        }
        // The evaluation is clamped, so a playback sampling past the rest time snaps to rest.
        assert_eq!(shot.state_at(shot.t_rest_s() + 10.0), at_rest);
        assert_eq!(shot.state_at(-1.0), shot.state_at(0.0));
    }
}

#[test]
fn a_ball_is_pocketed_only_while_it_is_moving() {
    // The amended drop predicate fires on a crossing of the mouth boundary, never on a state: this is
    // what stopped the prototype pocketing balls at rest in the rack.
    let arrangement = rack::generate(1);
    for (name, decl) in shots() {
        let shot = run(&decl, &arrangement);
        let mut pocketed = 0;
        for fact in shot.events() {
            let FactKind::Pocketed { ball, .. } = fact.kind else {
                continue;
            };
            pocketed += 1;
            assert!(
                fact.t > 0.0,
                "{name}: ball {ball} was pocketed at the strike instant, from its rack slot"
            );
            // The ball's state one segment earlier is the one the crossing was solved from.
            let before = shot.state_at(fact.t - 1e-9)[usize::from(ball)];
            assert!(
                before.in_play(),
                "{name}: ball {ball} was already out of play at t = {}",
                fact.t
            );
        }
        println!("{name}: {pocketed} pocketed");
    }
}

#[test]
fn the_cushion_square_hit_retention_reproduces_the_e_n_fit() {
    // `physics.md` §3.3: the horizontal COR from the normal channel alone is 0.6224 at e_n = 0.75,
    // while the measured retention with the tangential and vertical channels is 0.6714 — and 0.7178 at
    // e_n = 0.80. This is the one observable that tells the ruled 3D tangential channel from the
    // rejected 2D projection, and it is the `e_n` fit's own gate quantity.
    let expected = [(0.75, 0.6714), (0.78, 0.6992), (0.80, 0.7178)];
    for (e_n, want) in expected {
        let mut profile = Profile::default_profile();
        profile.e_n = e_n;
        let mut table = Table::with_profile(profile);
        // Only the four rails: the probe is a single square cushion hit.
        table.walls.retain(|wall| wall.rail.is_some());
        table.tips.clear();
        let mut sim = Sim::new(table, rack::generate(1));
        let shot = sim
            .strike(StrikeDecl {
                aim: [0.0, 1.0],
                speed_mm_s: 1000.0,
                spin: [0.0, 0.0],
                elevation_rad: 0.0,
            })
            .expect("a level centre strike is legal");
        let contact = shot
            .events()
            .iter()
            .find_map(|fact| match fact.kind {
                FactKind::RailContact { ball: 0, .. } => Some(fact.t),
                _ => None,
            })
            .expect("the cue ball reaches the long rail");
        let before = shot.state_at(contact - 1e-9)[0];
        let after = shot.state_at(contact + 1e-5)[0];
        let horizontal = |state: &BallState| {
            (state.vel_mm_s[0] * state.vel_mm_s[0] + state.vel_mm_s[1] * state.vel_mm_s[1]).sqrt()
        };
        let retention = horizontal(&after) / horizontal(&before);
        println!("e_n = {e_n}: square-hit retention {retention:.4} (expected {want})");
        assert!(
            (retention - want).abs() < 5e-4,
            "e_n = {e_n}: retention {retention:.4}, expected {want}"
        );
    }
}
