//! Break legality and classification (`rules-break.md` §3, `rules.md` §4).
//!
//! One break is classified once, from the fact stream, in the pinned order of §3.2: the 8 leaving the
//! table beats a pocket, and a pocket beats the rail count. Fouls are classified alongside the break,
//! never instead of it, and an illegal break that is also a foul keeps the illegal-break tree
//! (CSI 2-3-3).

use crate::facts::{Observation, OutOfPlay};
use crate::offer::Tree;
use crate::rail::{self, RailCount};
use crate::record::{Classification, Foul, FoulReason, FoulRule, Verdict};

/// 4.3(d)'s requirement: four distinct object balls driven to rails (`rules-break.md` §3.2).
pub const BREAK_RAIL_MINIMUM: usize = 4;

/// What the break's classification decided.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakRuling {
    /// The classification.
    pub classification: Classification,
    /// The overall verdict.
    pub verdict: Verdict,
    /// The fouls found, in rule order.
    pub fouls: Vec<Foul>,
    /// The tree the break raises, when one applies.
    pub tree: Option<Tree>,
    /// The balls the classification itself spots: 4.3(g)'s 8, driven off the table.
    pub spotted: Vec<u8>,
    /// The break's rail count: 4.3(d) reads `rails.counted_2_7`.
    pub rails: RailCount,
}

/// Classify one break (`rules-break.md` §3.2's precedence) and evaluate its fouls (§3.3).
#[must_use]
pub fn classify(observation: &Observation) -> BreakRuling {
    let out = OutOfPlay::of(observation);
    let rails = RailCount::of(
        &observation.facts,
        &observation.rest,
        &observation.pre_state,
    );
    let fouls = fouls_of(observation, &out);

    let eight_pocketed = out.contains_pocketed(8);
    let eight_off_table = out.contains_off_table(8);
    let object_pocketed = out
        .pocketed_balls()
        .iter()
        .any(|ball| *ball != 0 && *ball != 8);

    let (classification, verdict, tree) = if eight_pocketed || eight_off_table {
        // Precedence 1: the rail count is not consulted. 4.3(e) when the break was clean, 4.3(f)
        // otherwise — the 8 driven off the table is itself 3.5's foul, so it lands on (f).
        if fouls.is_empty() {
            (
                Classification::EightOnBreakLegal,
                Verdict::EightOnBreak,
                Some(Tree::EightOnBreakLegal),
            )
        } else {
            (
                Classification::EightOnBreakFoul,
                Verdict::EightOnBreak,
                Some(Tree::EightOnBreakFoul),
            )
        }
    } else if object_pocketed {
        // Precedence 2: a ball pocketed; the rail requirement sits behind 4.3(d)'s chapeau and is
        // never evaluated.
        if fouls.is_empty() {
            (Classification::LegalBreakByPocket, Verdict::Legal, None)
        } else {
            (break_foul(&out), Verdict::Foul, Some(Tree::BreakFoul))
        }
    } else if rails.counted_2_7.len() < BREAK_RAIL_MINIMUM {
        // Precedence 3: this tree governs whether or not a foul occurred.
        (
            Classification::IllegalBreak {
                foul: !fouls.is_empty(),
            },
            Verdict::IllegalBreak,
            Some(Tree::IllegalBreak),
        )
    } else if fouls.is_empty() {
        // Precedence 4: a clean legal break; the turn passes and the table stays open.
        (Classification::LegalBreakClean, Verdict::Legal, None)
    } else {
        (break_foul(&out), Verdict::Foul, Some(Tree::BreakFoul))
    };

    BreakRuling {
        classification,
        verdict,
        fouls,
        tree,
        spotted: if eight_off_table { vec![8] } else { Vec::new() },
        rails,
    }
}

/// 4.3(g) when an object ball left the table on the break, 4.3(h) otherwise.
fn break_foul(out: &OutOfPlay) -> Classification {
    Classification::BreakFoul {
        object_ball_off_table: !out.off_table_objects().is_empty(),
    }
}

/// The break-reachable fouls (`rules-break.md` §3.3): 3.1, 3.3 and 3.5. 3.2 is not evaluated
/// (4.3(b)), and the placement faults belong to the placement validator (`rules.md` §6).
#[must_use]
pub fn fouls_of(observation: &Observation, out: &OutOfPlay) -> Vec<Foul> {
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
    // 3.3: contact occurred, nothing was pocketed, and no rail contact followed the contact. A shot
    // that contacts nothing cannot be a 3.3 foul: 4.3(d) covers the total miss of the rack.
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
    fouls
}
