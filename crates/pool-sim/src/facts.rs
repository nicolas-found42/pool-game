//! The fact stream (`physics.md` §7, `architecture.md` §4.5): the vocabulary the rules layer reads.
//!
//! The simulation reports facts and never knows about fouls, groups, or turns: the kinds below, their
//! `seq` order, and their simultaneity group are the whole observation contract's physics half.

use serde::{Deserialize, Serialize};

use crate::ball::BallState;
use crate::table::{PocketId, Rail};

/// Why a ball left the cloth vertically (`physics.md` §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KickCause {
    /// A cushion contact's tilted normal drove the ball down; the cloth returned it.
    CushionPop,
    /// An airborne ball landed and bounced.
    Landing,
}

/// A fact's kind (`physics.md` §7).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FactKind {
    /// Two balls touched. The two ids are in candidate order, not sorted.
    BallBall {
        /// One ball's id.
        a: u8,
        /// The other ball's id.
        b: u8,
    },
    /// A ball touched a cushion face. The two frozen fields are first-class (`rules.md` §10.2): a
    /// contact counts toward "driven to a rail" iff `¬frozen_at_shot_start ∨ left_since_shot_start`.
    RailContact {
        /// The ball that touched the rail.
        ball: u8,
        /// Which rail.
        rail: Rail,
        /// Was the ball in contact with this rail in the state at shot start?
        frozen_at_shot_start: bool,
        /// Has the ball separated from this rail at any time since shot start?
        left_since_shot_start: bool,
    },
    /// A ball touched a jaw tip or a jaw face (`physics.md` §3.5.4).
    JawContact {
        /// The ball that touched the jaw.
        ball: u8,
        /// The pocket whose jaw it was.
        pocket: PocketId,
    },
    /// A ball's centre crossed into a mouth (`physics.md` §3.5.2).
    Pocketed {
        /// The ball that crossed.
        ball: u8,
        /// The mouth it crossed into.
        pocket: PocketId,
    },
    /// A ball left the playing surface (`physics.md` §3.4's ceiling rule).
    OffTable {
        /// The ball that left.
        ball: u8,
    },
    /// A ball left the cloth vertically.
    Kick {
        /// The ball that popped.
        ball: u8,
        /// What drove it: a cushion contact or a landing.
        cause: KickCause,
    },
    /// A ball's state snapped below the sleep thresholds (`physics.md` §5).
    Rest {
        /// The ball that stopped.
        ball: u8,
    },
    /// A ball's contact point stopped slipping: sliding → rolling (`physics.md` §3.1).
    SlideToRoll {
        /// The ball that transitioned.
        ball: u8,
    },
    /// A ball's translation stopped: rolling → spinning or stationary.
    RollToStop {
        /// The ball that stopped translating.
        ball: u8,
    },
    /// A spinning ball's spin decayed below the sleep threshold: spinning → stationary.
    SpinDown {
        /// The ball whose spin died.
        ball: u8,
    },
    /// The position-only separation step ran for two overlapping balls (`physics.md` §1).
    Depenetration {
        /// The lower ball id.
        a: u8,
        /// The higher ball id.
        b: u8,
    },
}

/// One fact (`architecture.md` §4.5).
///
/// `group` is the simultaneity group of `physics.md` §1 where grouping applies: facts a group's event
/// applications emit carry `Some(group)`, and the rest-snaps the solver detects at a group boundary —
/// which are not events of that group — carry `None`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    /// The fact's monotone index within its shot, from 0.
    pub seq: u64,
    /// The exact event time (s).
    pub t: f64,
    /// The simultaneity group, where grouping applies.
    pub group: Option<u32>,
    /// What happened.
    pub kind: FactKind,
}

/// The rail-contact counts of `physics.md` §7, in the **physical** reading: distinct object balls that
/// actually touched a rail, excluding balls pocketed or driven off the table. Rule 2.7's reading adds
/// the out-of-play balls back; the rules layer is the only place the two are combined.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RailSummary {
    /// Counted contacts by object balls: all rail contacts minus the frozen ones of `rules.md` §10.2.
    pub contacts: usize,
    /// Contacts suppressed by the frozen-at-shot-start clause (ball never left that rail).
    pub suppressed_frozen: usize,
    /// Distinct object balls with at least one counted contact that finished in play.
    pub distinct_object_balls: usize,
    /// Per-ball rail sets, ascending by ball id, over the counted contacts.
    pub per_ball: Vec<(u8, Vec<Rail>)>,
}

/// Derive the rail summary from a shot's facts and its rest states (`physics.md` §7).
///
/// The cue ball is not an object ball and contributes nothing. A ball that is pocketed or off the
/// table at rest contributes no rail even if it touched one on the way.
#[must_use]
pub fn rail_summary(facts: &[Fact], rest_states: &[BallState; 16]) -> RailSummary {
    let mut summary = RailSummary::default();
    let mut per_ball: Vec<(u8, Vec<Rail>)> = Vec::new();
    for fact in facts {
        let FactKind::RailContact {
            ball,
            rail,
            frozen_at_shot_start,
            left_since_shot_start,
        } = fact.kind
        else {
            continue;
        };
        if ball == 0 {
            continue;
        }
        let counts = !frozen_at_shot_start || left_since_shot_start;
        if !counts {
            summary.suppressed_frozen += 1;
            continue;
        }
        summary.contacts += 1;
        match per_ball.iter_mut().find(|(id, _)| *id == ball) {
            Some((_, rails)) => {
                if !rails.contains(&rail) {
                    rails.push(rail);
                }
            }
            None => per_ball.push((ball, vec![rail])),
        }
    }
    summary.distinct_object_balls = per_ball
        .iter()
        .filter(|(ball, _)| {
            rest_states
                .get(usize::from(*ball))
                .is_some_and(BallState::in_play)
        })
        .count();
    per_ball.sort_unstable_by_key(|(ball, _)| *ball);
    summary.per_ball = per_ball;
    summary
}
