//! Placement geometry and the spot search (`rules.md` §4/§6, `architecture.md` §9).
//!
//! The predicates are composed here from the simulation's published constants, so this file holds the
//! agreement test against the simulation's own answer (`Sim::place_cue`) and the four clauses of
//! WPA 1.5's algorithm, one case per clause.

use pool_rules::facts::PreBall;
use pool_rules::placement::{self, PlacementFault};
use pool_rules::spot::{CUE_SEPARATION_MM, spot_position};
use pool_rules::vocab::{PlacementDomain, Vec2};
use pool_sim::constants::{
    BALL_DIAMETER_MM, FOOT_SPOT_X_MM, HALF_LEN_MM, HALF_WIDTH_MM, HEAD_STRING_X_MM,
};
use pool_sim::rack;
use pool_sim::sim::Sim;
use pool_sim::table::Table;

fn ball(id: u8, x: f64, y: f64) -> PreBall {
    PreBall {
        id,
        x_mm: x,
        y_mm: y,
    }
}

fn table(balls: &[PreBall]) -> Vec<PreBall> {
    balls.to_vec()
}

/// The predicates must agree with the simulation's own placement rule on every point: the surface's
/// containment (`|x| <= HALF_LEN − R`, `|y| <= HALF_WIDTH − R`) and one diameter of clearance to every
/// ball, with exact contact accepted.
#[test]
fn the_predicates_agree_with_the_sims_placement() {
    let arrangement = rack::generate(1);
    let states = arrangement.rest_states();
    let balls: Vec<PreBall> = states
        .iter()
        .enumerate()
        .filter(|(_, state)| state.in_play())
        .map(|(id, state)| ball(id as u8, state.pos_mm[0], state.pos_mm[1]))
        .collect();
    let mut sim = Sim::new(Table::new(), arrangement);

    let mut candidates: Vec<[f64; 2]> = Vec::new();
    // A grid over the whole surface and a margin past its edge.
    let mut x = -HALF_LEN_MM - 20.0;
    while x <= HALF_LEN_MM + 20.0 {
        let mut y = -HALF_WIDTH_MM - 20.0;
        while y <= HALF_WIDTH_MM + 20.0 {
            candidates.push([x, y]);
            y += 25.0;
        }
        x += 25.0;
    }
    // The interesting boundary of every rack ball: exactly in contact, and a hair closer.
    for state in &states {
        if state.pos_mm[0] == 0.0 && state.pos_mm[1] == 0.0 {
            continue;
        }
        for offset in [
            [-1.0, 0.0],
            [1.0, 0.0],
            [0.0, -1.0],
            [0.0, 1.0],
            [-0.7, 0.7],
            [0.7, 0.7],
            [0.7, -0.7],
            [-0.7, -0.7],
        ] {
            for gap in [-1e-6, 0.0, 1e-6] {
                candidates.push([
                    state.pos_mm[0] + offset[0] * (BALL_DIAMETER_MM + gap),
                    state.pos_mm[1] + offset[1] * (BALL_DIAMETER_MM + gap),
                ]);
            }
        }
    }

    let mut mismatches: Vec<String> = Vec::new();
    for [x, y] in candidates {
        let pos = Vec2 { x, y };
        let sim_answer = sim.place_cue([x, y]);
        let ours = placement::validate(PlacementDomain::Anywhere, pos, &balls);
        if sim_answer.is_ok() != ours.is_ok() {
            mismatches.push(format!("({x}, {y}): sim {sim_answer:?}, rules {ours:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of the sampled placements disagree with the simulation:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// 2.13's reading: a ball on the head string is not above it, and the domain follows the centre.
#[test]
fn the_head_string_reading_is_two_thirteen() {
    let on_the_line = Vec2 {
        x: HEAD_STRING_X_MM,
        y: 0.0,
    };
    let above = Vec2 {
        x: HEAD_STRING_X_MM - 1e-9,
        y: 0.0,
    };
    let below = Vec2 {
        x: HEAD_STRING_X_MM + 1e-9,
        y: 0.0,
    };
    assert!(!placement::above_head_string(on_the_line));
    assert!(placement::above_head_string(above));
    assert!(!placement::above_head_string(below));

    let clear = table(&[ball(1, 1000.0, 0.0)]);
    assert_eq!(
        placement::validate(PlacementDomain::AboveHeadString, on_the_line, &clear).unwrap_err(),
        PlacementFault::OutsideDomain {
            domain: PlacementDomain::AboveHeadString
        }
    );
    assert!(placement::validate(PlacementDomain::Anywhere, on_the_line, &clear).is_ok());
    assert!(placement::validate(PlacementDomain::AboveHeadString, above, &clear).is_ok());
}

/// The surface and clearance halves of the validator, with exact contact accepted.
#[test]
fn the_validator_checks_the_surface_and_the_ball_clearance() {
    let balls = table(&[ball(0, -800.0, 0.0), ball(3, 100.0, 0.0)]);
    let past_the_rail = Vec2 {
        x: HALF_LEN_MM - pool_sim::constants::BALL_RADIUS_MM + 1e-6,
        y: 0.0,
    };
    assert_eq!(
        placement::validate(PlacementDomain::Anywhere, past_the_rail, &balls).unwrap_err(),
        PlacementFault::OutsideSurface {
            pos_mm: [past_the_rail.x, past_the_rail.y]
        }
    );
    let touching_the_rail = Vec2 {
        x: HALF_LEN_MM - pool_sim::constants::BALL_RADIUS_MM,
        y: 0.0,
    };
    assert!(
        placement::validate(PlacementDomain::Anywhere, touching_the_rail, &balls).is_ok(),
        "a ball may touch the cushion"
    );
    assert_eq!(
        placement::validate(PlacementDomain::Anywhere, Vec2 { x: 100.0, y: 0.0 }, &balls)
            .unwrap_err(),
        PlacementFault::Overlaps {
            ball: 3,
            gap_mm: -BALL_DIAMETER_MM
        }
    );
    let in_contact = Vec2 {
        x: 100.0 + BALL_DIAMETER_MM,
        y: 0.0,
    };
    assert!(
        placement::validate(PlacementDomain::Anywhere, in_contact, &balls).is_ok(),
        "exact contact is not an overlap"
    );
    assert_eq!(
        placement::validate(
            PlacementDomain::Anywhere,
            Vec2 {
                x: f64::NAN,
                y: 0.0
            },
            &balls
        )
        .unwrap_err(),
        PlacementFault::NotFinite
    );
    // The cue ball is never an obstacle to its own placement.
    assert!(
        placement::validate(
            PlacementDomain::Anywhere,
            Vec2 { x: -800.0, y: 0.0 },
            &balls
        )
        .is_ok()
    );
}

/// 1.6 ¶2's precondition: every legal object ball behind the head string, and the spot takes the
/// nearest one.
#[test]
fn the_spot_requests_precondition_and_its_ball() {
    let behind = table(&[
        ball(1, HEAD_STRING_X_MM - 400.0, 10.0),
        ball(2, HEAD_STRING_X_MM - 100.0, 0.0),
        ball(3, HEAD_STRING_X_MM - 900.0, -20.0),
    ]);
    let legal = [1, 2, 3];
    assert!(placement::all_above_head_string(&behind, &legal));
    assert_eq!(placement::nearest_to_head_string(&behind, &legal), Some(2));

    let playable = table(&[
        ball(1, HEAD_STRING_X_MM - 400.0, 10.0),
        ball(2, -100.0, 0.0),
    ]);
    assert!(!placement::all_above_head_string(&playable, &legal));

    let on_the_line = table(&[ball(2, HEAD_STRING_X_MM, 0.0)]);
    assert!(
        !placement::all_above_head_string(&on_the_line, &[2]),
        "a ball resting on the line is playable (2.13)"
    );
}

/// Clause 1: with nothing in the way the spot is the foot spot itself.
#[test]
fn the_spot_finds_the_foot_spot() {
    let balls = table(&[ball(0, -800.0, 0.0), ball(1, 1100.0, 400.0)]);
    assert_eq!(spot_position(8, &balls), [FOOT_SPOT_X_MM, 0.0]);
}

/// Clause 2: with the foot spot taken the spot goes into contact with the interfering ball, as close
/// to the foot spot as it can.
#[test]
fn the_spot_goes_into_contact_with_the_interfering_ball() {
    let balls = table(&[ball(0, -800.0, 0.0), ball(3, FOOT_SPOT_X_MM, 0.0)]);
    let spot = spot_position(8, &balls);
    assert_eq!(spot[1], 0.0);
    assert!(
        (spot[0] - (FOOT_SPOT_X_MM + BALL_DIAMETER_MM)).abs() < 1e-9,
        "expected contact just below the foot spot, got {spot:?}"
    );
    let gap = (spot[0] - FOOT_SPOT_X_MM).abs() - BALL_DIAMETER_MM;
    assert!(
        gap.abs() < 1e-9,
        "the gap to the interfering ball is contact"
    );
}

/// Clause 3: where the contact position would touch the cue ball the separation wins and the next
/// admissible position is taken; the spotted ball is never within δ of the cue.
#[test]
fn the_spot_never_touches_the_cue_ball() {
    let cue_x = FOOT_SPOT_X_MM + BALL_DIAMETER_MM;
    let balls = table(&[ball(0, cue_x, 0.0), ball(3, FOOT_SPOT_X_MM, 0.0)]);
    let spot = spot_position(8, &balls);
    let gap = (spot[0] - cue_x).abs() - BALL_DIAMETER_MM;
    assert!(
        gap >= CUE_SEPARATION_MM - 1e-9,
        "the spot {spot:?} sits {gap} mm from the cue ball"
    );
    assert!(
        (spot[0] - (cue_x + BALL_DIAMETER_MM + CUE_SEPARATION_MM)).abs() < 1e-9,
        "expected the cue's separation edge, got {spot:?}"
    );

    // The cue anywhere along the long string: the separation always holds.
    let mut at = -1200.0;
    while at <= 1200.0 {
        let balls = table(&[ball(0, at, 0.0), ball(3, FOOT_SPOT_X_MM, 0.0)]);
        let spot = spot_position(8, &balls);
        let gap = ((spot[0] - at).powi(2) + spot[1].powi(2)).sqrt() - BALL_DIAMETER_MM;
        assert!(
            gap >= CUE_SEPARATION_MM - 1e-6,
            "cue at {at}: the spot {spot:?} sits {gap} mm away"
        );
        at += 37.0;
    }
}

/// Clause 4: with the whole stretch below the foot spot blocked, the spot goes above the foot spot, as
/// close to it as possible.
#[test]
fn the_spot_falls_back_above_a_blocked_stretch() {
    let mut balls = vec![ball(0, -800.0, 0.0)];
    for step in 0..12 {
        let id = u8::try_from(step + 1).expect("a small id");
        balls.push(ball(id, FOOT_SPOT_X_MM + 55.0 * f64::from(step), 0.0));
    }
    let spot = spot_position(8, &balls);
    assert_eq!(spot[1], 0.0);
    assert!(
        spot[0] < FOOT_SPOT_X_MM,
        "the spot must fall above the foot spot, got {spot:?}"
    );
    assert!(
        (spot[0] - (FOOT_SPOT_X_MM - BALL_DIAMETER_MM)).abs() < 1e-9,
        "expected contact above the foot spot, got {spot:?}"
    );
}
