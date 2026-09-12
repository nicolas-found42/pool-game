//! Driven to a rail (`rules-break.md` §3.1, `rules.md` §3): the 2.7 predicate and the two counts.
//!
//! The physical count and 2.7's count are different numbers and this module never conflates them: the
//! physical count is the object balls that actually touched a rail and finished in play, and 2.7's
//! count adds the object balls pocketed or driven off the table, which 2.7 treats as having reached a
//! rail.

use crate::facts::{FactView, PreState, RestView, RulesFact};

/// What one shot's facts say about the 2.7 predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailCount {
    /// The object balls whose contacts counted and that finished in play, ascending: the *physical*
    /// count (`pool_sim::RailSummary::distinct_object_balls`).
    pub physical: Vec<u8>,
    /// The object balls with at least one suppressed contact: frozen at shot start and never having
    /// left that rail (2.7's exception, `rules.md` §10.2). A ball with both a suppressed and a counted
    /// contact appears here and in `physical`.
    pub suppressed_frozen: Vec<u8>,
    /// The object balls 2.7 counts: the physical count plus the object balls that finished out of
    /// play. This is the number 4.3(d)'s "at least four distinct object-balls" reads.
    pub counted_2_7: Vec<u8>,
}

impl RailCount {
    /// 2.7's count for a shot: distinct *object* balls, not contacts. The cue ball is never one of
    /// them (4.3(d)).
    #[must_use]
    pub fn of(facts: &FactView, rest: &RestView, pre_state: &PreState) -> Self {
        let mut touched: Vec<u8> = Vec::new();
        let mut suppressed: Vec<u8> = Vec::new();
        for fact in &facts.facts {
            let RulesFact::Rail { ball, .. } = *fact else {
                continue;
            };
            if ball == 0 {
                continue;
            }
            if contact_counts(fact, pre_state) {
                if !touched.contains(&ball) {
                    touched.push(ball);
                }
            } else if !suppressed.contains(&ball) {
                suppressed.push(ball);
            }
        }
        let mut physical: Vec<u8> = touched
            .iter()
            .copied()
            .filter(|ball| rest.on_table(*ball))
            .collect();
        physical.sort_unstable();
        suppressed.sort_unstable();
        let mut counted_2_7 = physical.clone();
        for ball in out_of_play_object_balls(rest) {
            if !counted_2_7.contains(&ball) {
                counted_2_7.push(ball);
            }
        }
        counted_2_7.sort_unstable();
        Self {
            physical,
            suppressed_frozen: suppressed,
            counted_2_7,
        }
    }
}

/// Whether a rail contact counts toward "driven to a rail" (`rules.md` §10.2):
/// `¬frozen_at_shot_start ∨ left_since_shot_start`.
///
/// The contact's own field is the simulation's answer; the pre-state's frozen list is the corpus's
/// declaration of the same annotation, and a ball in either is frozen.
#[must_use]
pub fn contact_counts(fact: &RulesFact, pre_state: &PreState) -> bool {
    let RulesFact::Rail {
        ball,
        frozen_at_shot_start,
        left_since_shot_start,
        ..
    } = *fact
    else {
        return false;
    };
    let frozen = frozen_at_shot_start || pre_state.frozen_to_rail.contains(&ball);
    !frozen || left_since_shot_start
}

/// The object balls that finished out of play — pocketed or off the table — which 2.7 counts as
/// driven to a rail. Ascending; the cue ball is not an object ball.
#[must_use]
pub fn out_of_play_object_balls(rest: &RestView) -> Vec<u8> {
    rest.out_of_play()
        .into_iter()
        .filter(|ball| *ball != 0)
        .collect()
}

/// Whether a rail contact follows the shot's first ball–ball contact, for 3.3's "after contact"
/// requirement (`rules.md` §3). `None` when nothing was contacted at all: 3.3's evidence requires that
/// contact occurred.
///
/// Within one simultaneity group the tie resolves in favour of legality (`rules.md` §3): a rail
/// contact in the first contact's own group counts as simultaneous with it.
#[must_use]
pub fn rail_after_first_contact(facts: &FactView) -> Option<bool> {
    let first = facts.facts.iter().position(RulesFact::is_ball_ball)?;
    let group = facts.group_of(first);
    let last = group.iter().copied().max().unwrap_or(first);
    let in_group = group
        .iter()
        .any(|index| matches!(facts.facts.get(*index), Some(RulesFact::Rail { .. })));
    Some(
        in_group
            || facts
                .facts
                .iter()
                .skip(last + 1)
                .any(|fact| matches!(fact, RulesFact::Rail { .. })),
    )
}
