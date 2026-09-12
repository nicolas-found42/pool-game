//! The ordinary shot (`rules.md` §2–§5): targets, the open-table claim, the fouls, and the 8-ball
//! phase's win and loss conditions.
//!
//! The break never comes here: 4.3 replaces the shot's requirements, so [`crate::break_rules`] owns it.
//! Everything below reads the same observation view the break does, so a corpus entry and a played shot
//! take identical paths.

use crate::facts::{FactView, Observation, OutOfPlay, PreState, RulesFact};
use crate::rail;
use crate::record::{Classification, Foul, FoulReason, FoulRule, Verdict};
use crate::state::{Group, Player, Target, Winner};
use crate::vocab::{Call, PlacementDomain};

/// The rack facts an ordinary shot reads (`rules.md` §2–§5).
#[derive(Debug, Clone, Copy)]
pub struct ShotContext<'a> {
    /// Who shoots.
    pub shooter: Player,
    /// The group assignment (4.4), if it has happened: the seat's group, indexed by
    /// [`Player::index`].
    pub assignment: Option<[Group; 2]>,
    /// The state before the shot.
    pub pre_state: &'a PreState,
}

/// What the shot's legality decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The called ball was pocketed legally: the shooter shoots again.
    Continue,
    /// The turn passes with no penalty.
    PassTurn,
    /// A standard foul: the incoming player has cue ball in hand anywhere (4.9).
    BallInHand,
    /// The rack ended (`rules.md` §5).
    RackOver(Winner),
}

/// What the shot's first contact was (`rules.md` §3's simultaneity resolution).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstContact {
    /// The cue met `ball` first.
    Ball {
        /// The ball.
        ball: u8,
        /// Whether it was a legal ball for the target.
        legal: bool,
    },
    /// Nothing was contacted at all: 3.2's requirement is violated.
    None,
    /// Contacts happened but none names the cue ball: undecidable, and undecidable defaults to legal
    /// (Reg 25, `rules.md` §3).
    Undecidable,
}

/// What the shot decided.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotRuling {
    /// The classification.
    pub classification: Classification,
    /// The overall verdict.
    pub verdict: Verdict,
    /// The fouls found, in rule order.
    pub fouls: Vec<Foul>,
    /// The assignment this shot made, if any (4.4).
    pub assignment: Option<[Group; 2]>,
    /// What follows the shot.
    pub outcome: Outcome,
}

/// The shooter's target class from the assignment and the table (`rules.md` §2).
#[must_use]
pub fn target_of(shooter: Player, assignment: Option<[Group; 2]>, on_table: &[u8]) -> Target {
    match assignment {
        None => Target::Open,
        Some(groups) => {
            if cleared(groups[shooter.index()], on_table) {
                Target::OnTheEight
            } else {
                Target::Group
            }
        }
    }
}

/// Whether every one of a group's seven balls is off the table.
#[must_use]
pub fn cleared(group: Group, on_table: &[u8]) -> bool {
    group.balls().iter().all(|ball| !on_table.contains(ball))
}

/// The legal balls a first contact may be, for a target.
#[must_use]
pub fn legal_balls(target: Target, group: Option<Group>) -> Vec<u8> {
    match target {
        Target::Open => (1..=15).filter(|ball| *ball != 8).collect(),
        Target::Group => group.map_or_else(
            || (1..=15).filter(|ball| *ball != 8).collect(),
            |group| group.balls().to_vec(),
        ),
        Target::OnTheEight => vec![8],
    }
}

/// The shot's first contact, resolved within its simultaneity group in favour of legality
/// (`rules.md` §3: legal ball assumed first, and any undecidable determination defaults to legal).
#[must_use]
pub fn first_contact(facts: &FactView, legal: &[u8]) -> FirstContact {
    let Some(first) = facts.facts.iter().position(RulesFact::is_ball_ball) else {
        return FirstContact::None;
    };
    let group = facts.group_of(first);
    let partners: Vec<u8> = group
        .iter()
        .filter_map(|index| match facts.facts.get(*index) {
            Some(RulesFact::BallBall { a, b, .. }) => match (*a == 0, *b == 0) {
                (true, false) => Some(*b),
                (false, true) => Some(*a),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if let Some(found) = partners.iter().find(|ball| legal.contains(ball)) {
        return FirstContact::Ball {
            ball: *found,
            legal: true,
        };
    }
    match partners.first() {
        Some(ball) => FirstContact::Ball {
            ball: *ball,
            legal: false,
        },
        None => FirstContact::Undecidable,
    }
}

/// Classify one ordinary shot: its fouls, its 4.4 assignment, and what follows it.
#[must_use]
pub fn classify(context: &ShotContext, call: &Call, observation: &Observation) -> ShotRuling {
    let out = OutOfPlay::of(observation);
    let on_table = observation.pre_state.on_table();
    let shooter_group = context
        .assignment
        .map(|groups| groups[context.shooter.index()]);

    // The open-table claim (`rules.md` §2): calling the 8 with a group already completely pocketed
    // claims that group for the shot and makes the 8 the target.
    let claim = matches!(call, Call::Ball { ball, .. } if ball.0 == 8)
        && context.assignment.is_none()
        && [Group::Solids, Group::Stripes]
            .iter()
            .any(|group| cleared(*group, &on_table));
    let target = if claim {
        Target::OnTheEight
    } else {
        target_of(context.shooter, context.assignment, &on_table)
    };
    let legal = legal_balls(target, shooter_group);
    let contact = first_contact(&observation.facts, &legal);

    let mut fouls = Vec::new();
    if out.contains_pocketed(0) {
        fouls.push(Foul {
            rule: FoulRule::CueBallOffTheTable,
            reason: FoulReason::CueBallPocketed,
        });
    } else if out.contains_off_table(0) {
        fouls.push(Foul {
            rule: FoulRule::CueBallOffTheTable,
            reason: FoulReason::CueBallOffTable,
        });
    }
    if !matches!(
        contact,
        FirstContact::Ball { legal: true, .. } | FirstContact::Undecidable
    ) {
        fouls.push(Foul {
            rule: FoulRule::WrongBallFirst,
            reason: FoulReason::WrongBallFirst,
        });
    }
    if !out.any_pocketed() && rail::rail_after_first_contact(&observation.facts) == Some(false) {
        fouls.push(Foul {
            rule: FoulRule::NoRailAfterContact,
            reason: FoulReason::NoRailAfterContact,
        });
    }
    for ball in out.off_table_objects() {
        fouls.push(Foul {
            rule: FoulRule::BallOffTheTable,
            reason: if ball == 8 {
                FoulReason::EightDrivenOffTable
            } else {
                FoulReason::ObjectBallOffTable
            },
        });
    }
    if played_from_above_the_head_string(context.pre_state, contact) {
        fouls.push(Foul {
            rule: FoulRule::FromAboveTheHeadString,
            reason: FoulReason::FromAboveTheHeadString,
        });
    }

    let eight_pocketed = out.contains_pocketed(8);
    let eight_target = claim || shooter_group.is_some_and(|group| cleared(group, &on_table));
    // The last group ball and the 8 in one stroke: an explicit loss (CSI 2-10(c)/APA, `rules.md` §5).
    let last_group_ball = shooter_group.and_then(|group| {
        let mut left = group
            .balls()
            .into_iter()
            .filter(|ball| on_table.contains(ball));
        let last = left.next()?;
        left.next().is_none().then_some(last)
    });
    let same_stroke =
        eight_pocketed && last_group_ball.is_some_and(|ball| out.contains_pocketed(ball));
    // 4.8(c): the 8 must drop in the called pocket. A ball counted pocketed by §10.7's
    // supported-over-mouth annotation never dropped, so the observation names no pocket for it.
    let called_pocket_ok = match call {
        Call::Ball { ball, pocket } if ball.0 == 8 => out
            .pocket_of(8)
            .is_none_or(|dropped| pocket.as_str() == dropped.name()),
        _ => false,
    };

    let loss = if out.contains_off_table(8) {
        Some("4.8(d)")
    } else if eight_pocketed && !fouls.is_empty() {
        Some("4.8(a)")
    } else if eight_pocketed && same_stroke {
        Some("CSI 2-10(c)/APA")
    } else if eight_pocketed && !eight_target {
        Some("4.8(b)")
    } else if eight_pocketed && !called_pocket_ok {
        Some("4.8(c)")
    } else {
        None
    };

    let (classification, verdict, outcome, assignment) = if let Some(clause) = loss {
        (
            Classification::Loss { clause },
            Verdict::Loss,
            Outcome::RackOver(Winner::of(context.shooter.other())),
            None,
        )
    } else if eight_pocketed {
        (
            Classification::Win,
            Verdict::Win,
            Outcome::RackOver(Winner::of(context.shooter)),
            None,
        )
    } else if !fouls.is_empty() {
        let rule = fouls
            .iter()
            .map(|foul| foul.rule)
            .min()
            .unwrap_or(FoulRule::CueBallOffTheTable);
        (
            Classification::StandardFoul { rule },
            Verdict::Foul,
            Outcome::BallInHand,
            None,
        )
    } else {
        let assignment = assignment_made(context, call, &out);
        let continues = match call {
            Call::Ball { ball, .. } => out.contains_pocketed(ball.0),
            Call::Safety | Call::Break => false,
        };
        (
            Classification::LegalShot,
            Verdict::Legal,
            if continues {
                Outcome::Continue
            } else {
                Outcome::PassTurn
            },
            assignment,
        )
    };

    ShotRuling {
        classification,
        verdict,
        fouls,
        assignment,
        outcome,
    }
}

/// 4.4: by legally pocketing the called ball the shooter claims its group, and the opponent gets the
/// other one. The call of the 8 claims nothing.
fn assignment_made(context: &ShotContext, call: &Call, out: &OutOfPlay) -> Option<[Group; 2]> {
    if context.assignment.is_some() {
        return None;
    }
    let Call::Ball { ball, .. } = call else {
        return None;
    };
    if !out.contains_pocketed(ball.0) {
        return None;
    }
    let group = Group::of(ball.0)?;
    let mut groups = [Group::Solids, Group::Stripes];
    groups[context.shooter.index()] = group;
    groups[context.shooter.other().index()] = group.other();
    Some(groups)
}

/// 3.11 (`rules.md` §3/§6): a shot played from a restricted placement whose first contact was above
/// the head string, with the cue ball never having crossed it.
///
/// The cue ball stood above the line and the contact was above the line, so the straight path between
/// them stayed above it: the crossing never happened.
fn played_from_above_the_head_string(pre_state: &PreState, contact: FirstContact) -> bool {
    if pre_state.placement_domain != Some(PlacementDomain::AboveHeadString) {
        return false;
    }
    let FirstContact::Ball { ball, .. } = contact else {
        return false;
    };
    let above = |id: u8| {
        pre_state.position(id).is_some_and(|[x, _]| {
            crate::placement::above_head_string(crate::vocab::Vec2 { x, y: 0.0 })
        })
    };
    above(0) && above(ball)
}
