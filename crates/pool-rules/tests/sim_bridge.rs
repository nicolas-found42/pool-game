//! The simulation bridge: the rules side of the fact stream (`rules.md` §10) and one whole break,
//! from a real `pool_sim` run through the machine.
//!
//! The corpus drives the machine from declared facts; this file drives it from the simulation's own
//! stream, so the two routes are shown to produce the same vocabulary.

use pool_rules::break_rules;
use pool_rules::facts::{FactView, Observation, PocketName, PreState, RulesFact};
use pool_rules::machine::{Input, Rack};
use pool_rules::record::Verdict;
use pool_rules::state::{Chooser, Player, RulesState, Target};
use pool_rules::vocab::{Call, PlacementDomain, ShotDeclaration, Spin, Vec2};
use pool_sim::ball::BallState;
use pool_sim::facts::{Fact, FactKind};
use pool_sim::rack;
use pool_sim::sim::{RestBlock, Shot, Sim};
use pool_sim::strike::StrikeDecl;
use pool_sim::table::{PocketId, Table};

/// The rack seed `pool-sim`'s break hook uses.
const RACK_SEED: u64 = 1;
/// The standard break position on the long string, above the head string.
const CUE_POS_MM: [f64; 2] = [-800.0, 0.0];

fn break_declaration(speed_mm_s: f64) -> ShotDeclaration {
    ShotDeclaration {
        call: Call::Break,
        aim: Vec2 { x: 1.0, y: 0.0 },
        speed: speed_mm_s,
        spin: Spin { a: 0.0, b: 0.0 },
        elevation: 0.0,
    }
}

fn fact(seq: u64, group: Option<u32>, kind: FactKind) -> Fact {
    Fact {
        seq,
        t: f64::from(u32::try_from(seq).expect("a small seq")) * 0.01,
        group,
        kind,
    }
}

/// The view keeps the four rules kinds, in stream order, carries the rail contact's frozen pair, and
/// keeps only the real simultaneity groups — over the kept indices.
#[test]
fn the_fact_view_maps_the_sims_stream() {
    let stream = vec![
        fact(0, Some(0), FactKind::BallBall { a: 0, b: 2 }),
        fact(1, Some(0), FactKind::BallBall { a: 2, b: 4 }),
        fact(2, Some(1), FactKind::Rest { ball: 5 }),
        fact(
            3,
            Some(1),
            FactKind::RailContact {
                ball: 4,
                rail: pool_sim::table::Rail::LeftLong,
                frozen_at_shot_start: false,
                left_since_shot_start: true,
            },
        ),
        fact(
            4,
            Some(2),
            FactKind::JawContact {
                ball: 6,
                pocket: PocketId::FootPlusY,
            },
        ),
        fact(
            5,
            None,
            FactKind::Pocketed {
                ball: 6,
                pocket: PocketId::SideMinusY,
            },
        ),
        fact(6, None, FactKind::OffTable { ball: 15 }),
    ];
    let view = FactView::from_sim(&stream);

    assert_eq!(view.facts.len(), 5, "the physics-only kinds are dropped");
    assert_eq!(
        view.facts[0],
        RulesFact::BallBall {
            t_s: 0.0,
            a: 0,
            b: 2
        }
    );
    assert_eq!(
        view.facts[1],
        RulesFact::BallBall {
            t_s: 0.01,
            a: 2,
            b: 4
        }
    );
    assert_eq!(
        view.facts[2],
        RulesFact::Rail {
            t_s: 0.03,
            ball: 4,
            frozen_at_shot_start: false,
            left_since_shot_start: true
        },
        "the rail contact keeps the frozen pair"
    );
    assert_eq!(
        view.facts[3],
        RulesFact::Pocket {
            t_s: 0.05,
            ball: 6,
            pocket: PocketName::SideLeft
        },
        "the pocket's identity maps into the corpus's vocabulary"
    );
    assert_eq!(
        view.facts[4],
        RulesFact::OffTable {
            t_s: 0.06,
            ball: 15
        }
    );

    // The first group keeps its two members at their kept indices; the group the dropped `Rest` fact
    // sat in is a group of one and is no simultaneity at all.
    assert_eq!(view.simultaneity_groups, vec![vec![0, 1]]);
    assert_eq!(view.group_of(0), vec![0, 1]);
    assert_eq!(view.group_of(3), vec![3]);
    assert_eq!(
        PocketName::FootLeft.pocket_id(),
        PocketId::FootMinusY,
        "the pocket vocabulary round-trips"
    );
}

/// The break of `physics.md` §6, driven end to end: place the cue ball above the head string, strike,
/// and hand the simulation's own report to the machine.
#[test]
fn a_real_break_reaches_the_machine() {
    let arrangement = rack::generate(RACK_SEED);
    let states = arrangement.rest_states();
    let mut sim = Sim::new(Table::new(), arrangement);
    sim.place_cue(CUE_POS_MM)
        .expect("the standard break position is legal");
    let shot: Shot = sim
        .strike(StrikeDecl {
            aim: [1.0, 0.0],
            speed_mm_s: 6200.0,
            spin: [0.0, 0.0],
            elevation_rad: 0.0,
        })
        .expect("a level centre strike is legal");

    let pre_state = PreState::from_states(&states, Vec::new());
    let observation = Observation::from_sim(shot.events(), shot.rest(), pre_state.clone());
    assert!(
        !observation.facts.facts.is_empty(),
        "the break's stream is not empty"
    );
    assert_the_rules_count_matches_the_physics(&observation, &shot);

    let rack = Rack::new(RACK_SEED, arrangement, Player::P1);
    let (placement, rack) = rack
        .adjudicate(Input::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: Vec2 {
                x: CUE_POS_MM[0],
                y: CUE_POS_MM[1],
            },
        })
        .expect("the standard break position is inside the domain and clear");
    assert_eq!(
        placement.apply.state,
        RulesState::AwaitingShot {
            shooter: Player::P1,
            target: Target::Open
        }
    );

    let (record, next) = rack
        .adjudicate(Input::Shot {
            declaration: break_declaration(6200.0),
            observation: Box::new(observation),
        })
        .expect("the rack awaits its break");
    assert_eq!(
        record.verdict,
        Verdict::Legal,
        "the hook's break is a legal break: {record:?}"
    );
    assert_eq!(record.fouls, Vec::new());
    assert!(record.offers.is_empty());
    assert!(
        record.legality.starts_with("legal_break_"),
        "{}",
        record.legality
    );
    // A legal break continues with the breaker (4.3(c)) or passes the turn (4.3(d)); the table stays
    // open either way.
    let shooter = match record.apply.action {
        pool_rules::record::Action::Continue => Player::P1,
        pool_rules::record::Action::PassTurn => Player::P2,
        other => panic!("a legal break continues or passes the turn, got {other:?}"),
    };
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingShot {
            shooter,
            target: Target::Open
        }
    );
    assert_eq!(next.state, record.apply.state);
    assert_eq!(next.shot_count, 1);
    assert!(
        next.assignment.is_none(),
        "4.3(c): no group is assigned on the break"
    );
}

/// The machine's own count of 4.3(d) against the simulation's physical count: 2.7's count is the
/// physical count plus the object balls that finished out of play (`rules.md` §3).
fn assert_the_rules_count_matches_the_physics(observation: &Observation, shot: &Shot) {
    let ruling = break_rules::classify(observation);
    let physical = pool_sim::facts::rail_summary(shot.events(), &shot.rest().states);
    assert!(
        physical.distinct_object_balls >= 4,
        "the hook's break drives {} object balls to rails",
        physical.distinct_object_balls
    );
    assert!(
        ruling.rails.counted_2_7.len() >= 4,
        "2.7's count over the same facts is {}",
        ruling.rails.counted_2_7.len()
    );
    let out_of_play: Vec<u8> = shot
        .rest()
        .pocketed
        .iter()
        .chain(&shot.rest().off_table)
        .copied()
        .filter(|ball| *ball != 0)
        .collect();
    let added = out_of_play
        .iter()
        .filter(|ball| !ruling.rails.physical.contains(ball))
        .count();
    assert_eq!(
        ruling.rails.counted_2_7.len(),
        physical.distinct_object_balls + added,
        "2.7's count is the physical count plus {out_of_play:?}"
    );
}

/// A break that never reaches the rack: `rules-break.md` §3.3's total miss is an illegal break with no
/// foul (3.3's own evidence needs a contact), the cue ball's own rail contact is never one of the four,
/// and the incoming player is offered the tree.
#[test]
fn a_break_that_misses_the_rack_is_an_illegal_break() {
    let arrangement = rack::generate(RACK_SEED);
    let states = arrangement.rest_states();
    let mut sim = Sim::new(Table::new(), arrangement);
    sim.place_cue(CUE_POS_MM).expect("a legal placement");
    let shot = sim
        .strike(StrikeDecl {
            aim: [0.0, 1.0],
            speed_mm_s: 1200.0,
            spin: [0.0, 0.0],
            elevation_rad: 0.0,
        })
        .expect("a level centre strike is legal");
    let observation = Observation::from_sim(
        shot.events(),
        shot.rest(),
        PreState::from_states(&states, Vec::new()),
    );
    let contacts = observation
        .facts
        .facts
        .iter()
        .filter(|fact| fact.is_ball_ball())
        .count();
    assert_eq!(contacts, 0, "the cue ball must not reach the rack");
    assert!(
        observation
            .facts
            .facts
            .iter()
            .any(|fact| matches!(fact, RulesFact::Rail { ball: 0, .. })),
        "the cue ball's own rail contact is in the stream"
    );
    let ruling = break_rules::classify(&observation);
    assert!(
        ruling.rails.counted_2_7.is_empty(),
        "the cue ball is never one of the four: {:?}",
        ruling.rails.counted_2_7
    );

    let rack = Rack {
        state: RulesState::AwaitingShot {
            shooter: Player::P1,
            target: Target::Open,
        },
        ..Rack::new(RACK_SEED, arrangement, Player::P1)
    };
    let (record, next) = rack
        .adjudicate(Input::Shot {
            declaration: ShotDeclaration {
                aim: Vec2 { x: 0.0, y: 1.0 },
                ..break_declaration(1200.0)
            },
            observation: Box::new(observation),
        })
        .expect("the rack awaits its break");
    assert_eq!(record.verdict, Verdict::IllegalBreak);
    assert_eq!(record.fouls, Vec::new());
    assert_eq!(record.apply.action, pool_rules::record::Action::OfferChoice);
    assert!(matches!(
        next.state,
        RulesState::AwaitingChoice {
            chooser: Chooser::Incoming,
            ..
        }
    ));
    assert_eq!(next.state, record.apply.state);
}

/// The rest block's annotations survive the mapping: the supported-over-mouth route into "pocketed"
/// (§10.7) is read from the simulation's own annotation.
#[test]
fn the_rest_view_carries_the_annotations() {
    let states: [BallState; 16] =
        std::array::from_fn(|id| BallState::at_rest([id as f64, 0.0, 0.0]));
    let rest = RestBlock {
        states,
        pocketed: vec![3],
        off_table: vec![],
        frozen_pairs: vec![[1, 2]],
        frozen_rails: vec![(4, pool_sim::table::Rail::RightLong)],
        supported_over_mouth: vec![pool_sim::sim::SupportedOverMouth {
            ball: 5,
            supporting_balls: vec![6, 7],
        }],
    };
    let view = pool_rules::facts::RestView::from_sim(&rest);
    assert_eq!(view.balls.len(), 16);
    assert_eq!(view.frozen_rails, vec![4]);
    assert_eq!(view.supported_over_mouth.len(), 1);
    assert_eq!(view.supported_over_mouth[0].ball, 5);
    assert_eq!(view.supported_over_mouth[0].supporting, vec![6, 7]);
    assert_eq!(PreState::after(&view).frozen_to_rail, vec![4]);
}
