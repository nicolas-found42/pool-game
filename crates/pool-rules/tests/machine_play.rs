//! The ordinary shot through the machine (`rules.md` §2–§5): targets and assignment, the fouls the
//! corpus's break phase cannot reach, the win and the four loss conditions, and the transitions the
//! state machine owns — placement, the 1.6 ¶2 request, and the re-rack.
//!
//! Every test drives [`Rack::adjudicate`], never an internal helper: these are the records a caller
//! sees.

use pool_rules::facts::{
    FactView, Observation, PocketName, PreState, RestBall, RestView, RulesFact,
};
use pool_rules::machine::{Input, InputError, Rack};
use pool_rules::offer::{OptionId, Tree};
use pool_rules::placement::PlacementFault;
use pool_rules::record::{
    Action, Adjudication, Classification, Foul, FoulReason, FoulRule, Verdict,
};
use pool_rules::state::{Chooser, Group, Player, RulesState, Target, Winner};
use pool_rules::vocab::{BallNumber, Call, PlacementDomain, ShotDeclaration, Spin, Vec2};

/// The rack seed the tests build their snapshot from.
const SEED: u64 = 7;
/// The seven solids, for the tables where the shooter's group is already down.
const SOLIDS: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];

fn arrangement() -> pool_sim::rack::Arrangement {
    pool_sim::rack::generate(SEED)
}

/// The rack's own positions as the rules layer's table view.
fn rack_table() -> PreState {
    PreState::from_states(&arrangement().rest_states(), Vec::new())
}

/// The rack's positions with `down` taken off the table.
fn table_without(down: &[u8]) -> PreState {
    let mut table = rack_table();
    table.balls.retain(|ball| !down.contains(&ball.id));
    table
}

/// A machine at the table for `P1` on an open table, with `table` as its view, past the rack's break
/// (the break's own call is `Call::Break`, `rules.md` §1).
fn shooter(table: PreState) -> Rack {
    Rack {
        state: RulesState::AwaitingShot {
            shooter: Player::P1,
            target: Target::Open,
        },
        shot_count: 1,
        table,
        ..Rack::new(SEED, arrangement(), Player::P1)
    }
}

fn declaration(call: Call) -> ShotDeclaration {
    ShotDeclaration {
        call,
        aim: Vec2 { x: 1.0, y: 0.0 },
        speed: 1000.0,
        spin: Spin { a: 0.0, b: 0.0 },
        elevation: 0.0,
    }
}

fn contact(t_s: f64, a: u8, b: u8) -> RulesFact {
    RulesFact::BallBall { t_s, a, b }
}

fn pocket(t_s: f64, ball: u8, pocket: PocketName) -> RulesFact {
    RulesFact::Pocket { t_s, ball, pocket }
}

fn off_table(t_s: f64, ball: u8) -> RulesFact {
    RulesFact::OffTable { t_s, ball }
}

/// The observation of a shot that left `still_on` on the table: the rest block repeats the pre-shot
/// positions of the balls that are still there.
fn observed(pre: &PreState, facts: Vec<RulesFact>, still_on: &[u8]) -> Observation {
    let balls = pre
        .balls
        .iter()
        .filter(|ball| still_on.contains(&ball.id))
        .map(|ball| RestBall {
            id: ball.id,
            x_mm: ball.x_mm,
            y_mm: ball.y_mm,
            z_mm: 0.0,
            vx_mm_s: 0.0,
            vy_mm_s: 0.0,
            vz_mm_s: 0.0,
            wx: 0.0,
            wy: 0.0,
            wz: 0.0,
        })
        .collect();
    Observation {
        facts: FactView {
            facts,
            simultaneity_groups: Vec::new(),
        },
        rest: RestView {
            balls,
            supported_over_mouth: Vec::new(),
            frozen_rails: Vec::new(),
        },
        pre_state: pre.clone(),
    }
}

fn all_balls() -> Vec<u8> {
    (0..=15).collect()
}

fn without(balls: &[u8], down: &[u8]) -> Vec<u8> {
    balls
        .iter()
        .copied()
        .filter(|ball| !down.contains(ball))
        .collect()
}

fn play(rack: &Rack, call: Call, observation: Observation) -> (Adjudication, Rack) {
    rack.adjudicate(Input::Shot {
        declaration: declaration(call),
        observation: Box::new(observation),
    })
    .expect("the state awaits a shot")
}

fn call_of(ball: u8, pocket: &str) -> Call {
    Call::Ball {
        ball: BallNumber(ball),
        pocket: pocket.to_string(),
    }
}

/// Ball 1 into the foot-left pocket on an open table: the called ball is pocketed and the shooter
/// continues, claiming the solids (4.4).
#[test]
fn a_called_ball_pocketed_continues_and_assigns_the_group() {
    let table = rack_table();
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 1), pocket(0.5, 1, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[1]));
    let (record, next) = play(&rack, call_of(1, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Legal);
    assert!(record.fouls.is_empty());
    assert_eq!(record.apply.action, Action::Continue);
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingShot {
            shooter: Player::P1,
            target: Target::Group
        }
    );
    assert_eq!(next.assignment, Some([Group::Solids, Group::Stripes]));
    assert_eq!(next.target_of(Player::P1), Target::Group);
    assert_eq!(next.target_of(Player::P2), Target::Group);
}

/// Nothing the call named goes down: the turn passes and the table stays open (4.4).
#[test]
fn an_uncalled_pocket_passes_the_turn_without_assigning() {
    let table = rack_table();
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 1), pocket(0.5, 2, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[2]));
    let (record, next) = play(&rack, call_of(1, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Legal);
    assert_eq!(record.apply.action, Action::PassTurn);
    assert_eq!(next.assignment, None);
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingShot {
            shooter: Player::P2,
            target: Target::Open
        }
    );
}

/// A safety is a call type: the turn passes whatever falls (3.2's requirements still bind).
#[test]
fn a_safety_passes_the_turn() {
    let table = rack_table();
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 1), pocket(0.5, 1, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[1]));
    let (record, next) = play(&rack, Call::Safety, observation);

    assert_eq!(record.verdict, Verdict::Legal);
    assert_eq!(record.apply.action, Action::PassTurn);
    assert_eq!(next.assignment, None, "a safety never assigns");
}

/// A scratch: 3.1's foul, the incoming player's cue ball in hand anywhere (4.9).
#[test]
fn a_scratch_gives_ball_in_hand_anywhere() {
    let table = rack_table();
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 1), pocket(0.4, 0, PocketName::HeadLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[0]));
    let (record, next) = play(&rack, call_of(1, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Foul);
    assert_eq!(
        record.fouls,
        vec![Foul {
            rule: FoulRule::CueBallOffTheTable,
            reason: FoulReason::CueBallPocketed
        }]
    );
    assert_eq!(record.apply.action, Action::CueBallInHandAnywhere);
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingPlacement {
            shooter: Player::P2,
            domain: PlacementDomain::Anywhere
        }
    );
    assert_eq!(next.state, record.apply.state);
}

/// 3.2 with an assigned group: a solid's shooter who meets a stripe first commits the foul.
#[test]
fn wrong_ball_first_is_a_foul() {
    let table = rack_table();
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 9)];
    let observation = observed(&table, facts, &all_balls());
    let (record, _next) = play(&rack, call_of(1, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Foul);
    assert!(
        record
            .fouls
            .iter()
            .any(|foul| foul.rule == FoulRule::WrongBallFirst),
        "3.2 must be reported: {:?}",
        record.fouls
    );
}

/// The open-table claim (`rules.md` §2): calling the 8 while one group is completely pocketed makes
/// the 8 the target, and pocketing it in the called pocket wins the rack.
#[test]
fn the_open_table_claim_wins_the_rack() {
    let table = table_without(&SOLIDS);
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 8), pocket(0.5, 8, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, next) = play(&rack, call_of(8, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Win);
    assert!(
        record.fouls.is_empty(),
        "the claim is the legal call: {:?}",
        record.fouls
    );
    assert_eq!(record.apply.action, Action::RackOver);
    assert_eq!(
        record.apply.state,
        RulesState::RackOver { winner: Winner::P1 }
    );
    assert_eq!(next.state, record.apply.state);
}

/// Without the claim the 8 is no legal first contact: striking it first is 3.2, and pocketing it is a
/// loss.
#[test]
fn striking_the_eight_first_without_the_claim_loses() {
    let table = rack_table();
    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 8), pocket(0.5, 8, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, _next) = play(&rack, call_of(8, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Loss);
    assert!(
        record
            .fouls
            .iter()
            .any(|foul| foul.rule == FoulRule::WrongBallFirst)
    );
    assert_eq!(
        record.legality,
        Classification::Loss { clause: "4.8(a)" }.label(),
        "a foul with the 8 pocketed is 4.8(a)"
    );
}

/// 4.8(b): the 8 goes down before the shooter's group is cleared — an uncalled drop is enough.
#[test]
fn pocketing_the_eight_before_the_group_is_cleared_loses() {
    let table = rack_table();
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 1), pocket(0.5, 8, PocketName::FootLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, _next) = play(&rack, call_of(1, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Loss);
    assert!(record.fouls.is_empty());
    assert_eq!(
        record.legality,
        Classification::Loss { clause: "4.8(b)" }.label()
    );
    assert_eq!(
        record.apply.state,
        RulesState::RackOver { winner: Winner::P2 }
    );
}

/// 4.8(c): the 8 drops with the group cleared, but not in the called pocket.
#[test]
fn pocketing_the_eight_in_an_uncalled_pocket_loses() {
    let table = table_without(&SOLIDS);
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 8), pocket(0.5, 8, PocketName::FootRight)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, _next) = play(&rack, call_of(8, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Loss);
    assert_eq!(
        record.legality,
        Classification::Loss { clause: "4.8(c)" }.label()
    );
}

/// A legal win: the group is cleared and the 8 drops in its called pocket.
#[test]
fn the_eight_in_its_called_pocket_wins() {
    let table = table_without(&SOLIDS);
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 8), pocket(0.5, 8, PocketName::SideLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, _next) = play(&rack, call_of(8, "side_left"), observation);

    assert_eq!(record.verdict, Verdict::Win);
    assert!(record.fouls.is_empty());
}

/// 4.8(d): the 8 driven off the table loses the rack, and 3.5's reason names it.
#[test]
fn the_eight_off_the_table_loses() {
    let table = table_without(&SOLIDS);
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 8), off_table(0.5, 8)];
    let observation = observed(&table, facts, &without(&all_balls(), &[8]));
    let (record, _next) = play(&rack, call_of(8, "side_left"), observation);

    assert_eq!(record.verdict, Verdict::Loss);
    assert!(
        record
            .fouls
            .iter()
            .any(|foul| foul.rule == FoulRule::BallOffTheTable
                && foul.reason == FoulReason::EightDrivenOffTable),
        "3.5's reason names the 8: {:?}",
        record.fouls
    );
    assert_eq!(
        record.legality,
        Classification::Loss { clause: "4.8(d)" }.label()
    );
}

/// The last group ball and the 8 in one stroke: an explicit loss (CSI 2-10(c)/APA, `rules.md` §5).
#[test]
fn the_last_group_ball_and_the_eight_in_one_stroke_loses() {
    let last: u8 = 3;
    let table = table_without(&[1, 2, 4, 5, 6, 7]);
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![
        contact(0.1, 0, last),
        pocket(0.5, last, PocketName::FootLeft),
        pocket(0.6, 8, PocketName::FootLeft),
    ];
    let observation = observed(&table, facts, &without(&all_balls(), &[last, 8]));
    let (record, _next) = play(&rack, call_of(last, "foot_left"), observation);

    assert_eq!(record.verdict, Verdict::Loss);
    assert_eq!(
        record.legality,
        Classification::Loss {
            clause: "CSI 2-10(c)/APA"
        }
        .label()
    );
}

/// Fouling while shooting the 8 without pocketing it is a standard foul only — the 8 stays
/// (CSI 2-9-2).
#[test]
fn fouling_on_the_eight_is_a_standard_foul() {
    let table = table_without(&SOLIDS);
    let rack = Rack {
        assignment: Some([Group::Solids, Group::Stripes]),
        ..shooter(table.clone())
    };
    let facts = vec![contact(0.1, 0, 8), pocket(0.4, 0, PocketName::HeadLeft)];
    let observation = observed(&table, facts, &without(&all_balls(), &[0]));
    let (record, _next) = play(&rack, call_of(8, "side_left"), observation);

    assert_eq!(record.verdict, Verdict::Foul);
    assert_eq!(record.fouls[0].rule, FoulRule::CueBallOffTheTable);
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingPlacement {
            shooter: Player::P2,
            domain: PlacementDomain::Anywhere
        }
    );
}

/// 3.11: a shot played from a restricted placement whose first contact was above the head string,
/// with the cue ball never having crossed it.
#[test]
fn a_restricted_shot_that_never_crosses_the_line_is_a_foul() {
    let mut table = rack_table();
    let cue = table
        .balls
        .iter_mut()
        .find(|ball| ball.id == 0)
        .expect("the cue ball");
    cue.x_mm = pool_sim::constants::HEAD_STRING_X_MM - 100.0;
    cue.y_mm = 0.0;
    let target = table
        .balls
        .iter_mut()
        .find(|ball| ball.id == 2)
        .expect("ball 2");
    target.x_mm = pool_sim::constants::HEAD_STRING_X_MM - 200.0;
    target.y_mm = 100.0;
    table.placement_domain = Some(PlacementDomain::AboveHeadString);

    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 2)];
    let observation = observed(&table, facts, &all_balls());
    let (record, _next) = play(&rack, call_of(2, "foot_left"), observation);

    assert!(
        record
            .fouls
            .iter()
            .any(|foul| foul.rule == FoulRule::FromAboveTheHeadString),
        "3.11 must be reported: {:?}",
        record.fouls
    );
}

/// The same restricted shot whose first contact is *below* the line crossed it: no 3.11 foul.
#[test]
fn a_restricted_shot_that_crosses_the_line_is_legal() {
    let mut table = rack_table();
    let cue = table
        .balls
        .iter_mut()
        .find(|ball| ball.id == 0)
        .expect("the cue ball");
    cue.x_mm = pool_sim::constants::HEAD_STRING_X_MM - 100.0;
    cue.y_mm = 0.0;
    table.placement_domain = Some(PlacementDomain::AboveHeadString);

    let rack = shooter(table.clone());
    let facts = vec![contact(0.1, 0, 2)];
    let observation = observed(&table, facts, &all_balls());
    let (record, _next) = play(&rack, call_of(2, "foot_left"), observation);

    assert!(
        !record
            .fouls
            .iter()
            .any(|foul| foul.rule == FoulRule::FromAboveTheHeadString),
        "3.11 must not fire when the cue ball crossed the line: {:?}",
        record.fouls
    );
}

/// The placement transition (`rules.md` §6): a domain-correct placement is taken, a placement below
/// the line is not, and the machine moves to the shot with the shooter's target.
#[test]
fn a_placement_moves_to_the_shot_and_is_validated() {
    let rack = Rack {
        state: RulesState::AwaitingPlacement {
            shooter: Player::P2,
            domain: PlacementDomain::AboveHeadString,
        },
        table: rack_table(),
        ..Rack::new(SEED, arrangement(), Player::P1)
    };
    let below = Vec2 {
        x: pool_sim::constants::HEAD_STRING_X_MM + 10.0,
        y: 0.0,
    };
    let refused = rack.adjudicate(Input::Placement {
        domain: PlacementDomain::AboveHeadString,
        pos: below,
    });
    assert_eq!(
        refused.unwrap_err(),
        InputError::Placement(PlacementFault::OutsideDomain {
            domain: PlacementDomain::AboveHeadString
        })
    );

    let above = Vec2 {
        x: pool_sim::constants::HEAD_STRING_X_MM - 10.0,
        y: 0.0,
    };
    let (record, next) = rack
        .adjudicate(Input::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: above,
        })
        .expect("the placement is inside its domain, on the surface and clear");
    assert_eq!(record.verdict, Verdict::NoShot);
    assert_eq!(record.apply.action, Action::PlaceCue);
    assert_eq!(
        record.apply.state,
        RulesState::AwaitingShot {
            shooter: Player::P2,
            target: Target::Open
        }
    );
    assert_eq!(next.state, record.apply.state);
    assert_eq!(next.table.position(0), Some([above.x, above.y]));
}

/// A placement whose domain is not the awaited one is refused at the boundary.
#[test]
fn a_placement_in_the_wrong_domain_is_refused() {
    let rack = Rack {
        state: RulesState::AwaitingPlacement {
            shooter: Player::P1,
            domain: PlacementDomain::AboveHeadString,
        },
        table: rack_table(),
        ..Rack::new(SEED, arrangement(), Player::P1)
    };
    let error = rack
        .adjudicate(Input::Placement {
            domain: PlacementDomain::Anywhere,
            pos: Vec2 { x: 0.0, y: 0.0 },
        })
        .unwrap_err();
    assert_eq!(
        error,
        InputError::DomainMismatch {
            awaited: PlacementDomain::AboveHeadString,
            given: PlacementDomain::Anywhere
        }
    );
}

/// The 1.6 ¶2 request: legal only while every legal object ball is above the head string, and it spots
/// the nearest one without changing what the state awaits.
#[test]
fn the_spot_request_needs_every_legal_ball_behind_the_line() {
    let mut table = rack_table();
    for ball in table.balls.iter_mut().filter(|ball| ball.id != 0) {
        ball.x_mm = pool_sim::constants::HEAD_STRING_X_MM - 50.0 - f64::from(ball.id);
        ball.y_mm = f64::from(ball.id) * 2.0;
    }
    let rack = Rack {
        state: RulesState::AwaitingPlacement {
            shooter: Player::P1,
            domain: PlacementDomain::AboveHeadString,
        },
        table: table.clone(),
        ..Rack::new(SEED, arrangement(), Player::P1)
    };
    let (record, next) = rack
        .adjudicate(Input::SpotRequest)
        .expect("every legal object ball is above the line");
    assert_eq!(record.verdict, Verdict::NoShot);
    assert_eq!(record.apply.action, Action::SpotBall);
    assert_eq!(
        record.apply.spotted,
        vec![1],
        "the nearest legal ball is spotted"
    );
    assert_eq!(next.state, rack.state, "the placement is still awaited");
    assert_eq!(next.spots.len(), 1);
    assert_eq!(next.table.position(1), Some(next.spots[0].pos_mm));

    // One legal ball playable below the line: the request is refused.
    let mut playable = table;
    playable
        .balls
        .iter_mut()
        .find(|ball| ball.id == 2)
        .expect("ball 2")
        .x_mm = 0.0;
    let rack = Rack {
        table: playable,
        ..rack
    };
    assert_eq!(
        rack.adjudicate(Input::SpotRequest).unwrap_err(),
        InputError::SpotRequestUnavailable
    );
}

/// The re-rack transition (`rules-break.md` §3.7): the snapshot restored, the seed unchanged, the
/// counter reset, and the breaker set by the option's text.
#[test]
fn a_re_rack_restores_the_snapshot_and_sets_the_breaker() {
    let rack = Rack {
        state: RulesState::AwaitingChoice {
            chooser: Chooser::Incoming,
            offers: vec![Tree::IllegalBreak.offer()],
        },
        breaker: Player::P1,
        shot_count: 3,
        table: table_without(&[1, 2, 3]),
        ..Rack::new(SEED, arrangement(), Player::P1)
    };

    let (record, next) = rack
        .adjudicate(Input::Option(OptionId::ReRackAndBreak))
        .expect("the illegal-break tree is pending");
    assert_eq!(record.apply.action, Action::ReRack);
    assert!(record.apply.shot_count_reset);
    assert!(record.apply.rack_seed_unchanged);
    assert_eq!(
        record.apply.breaker,
        Player::P2,
        "4.3(d)(2): the incoming player breaks"
    );
    assert_eq!(next.breaker, Player::P2);
    assert_eq!(next.rack_seed, SEED);
    assert_eq!(next.snapshot, arrangement());
    assert_eq!(next.shot_count, 0);
    assert_eq!(next.assignment, None);
    assert_eq!(next.target_of(Player::P2), Target::Open);
    assert_eq!(
        next.state,
        RulesState::AwaitingPlacement {
            shooter: Player::P2,
            domain: PlacementDomain::AboveHeadString
        }
    );
    for ball in 1..=15 {
        assert_eq!(
            next.table.position(ball),
            Some(pool_sim::slot_position_mm(arrangement().slot_of(ball))),
            "ball {ball} is back at its snapshot position"
        );
    }

    // 4.3(d)(3): the offending breaker breaks again.
    let (record, next) = rack
        .adjudicate(Input::Option(OptionId::ReRackAndOffenderBreaks))
        .expect("the illegal-break tree is pending");
    assert_eq!(record.apply.breaker, Player::P1);
    assert_eq!(next.breaker, Player::P1);
}

/// An input the state does not await is refused, and an option the pending tree does not offer too.
#[test]
fn the_machine_refuses_what_it_does_not_await() {
    let rack = shooter(rack_table());
    assert_eq!(
        rack.adjudicate(Input::SpotRequest).unwrap_err(),
        InputError::NotAwaited {
            awaited: "awaiting_shot",
            input: "spot_request"
        }
    );
    let rack = Rack {
        state: RulesState::AwaitingChoice {
            chooser: Chooser::Incoming,
            offers: vec![Tree::BreakFoul.offer()],
        },
        ..rack
    };
    assert_eq!(
        rack.adjudicate(Input::Option(OptionId::ReRackAndBreak))
            .unwrap_err(),
        InputError::UnknownOption(OptionId::ReRackAndBreak)
    );
}
