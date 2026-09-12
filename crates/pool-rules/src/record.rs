//! The adjudication record (`rules.md` §1/§3, `rules-break.md` §"Corpus contract"): what the machine
//! decides, in the corpus's own shape.
//!
//! The record is the assertion surface: `docs/spec/rules-break.json` asserts every field of it, so the
//! serde shapes here are the corpus's — verdict, classification text, ordered foul reasons, the offer
//! sets and their choosers, the chosen option where a tree applies, and the applied action with the
//! resulting state.

use serde::{Deserialize, Serialize};

use crate::offer::{ChosenOption, Offer, Tree};
use crate::state::Player;
use crate::state::RulesState;

/// The record's overall verdict (`break-corpus.schema.json`'s enum, plus the rack-ending outcomes of
/// `rules.md` §5, which the break-phase corpus cannot reach).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// A legal shot.
    Legal,
    /// A standard foul (4.9's penalty).
    Foul,
    /// 4.3(d): the break did not meet 4.3(d)'s requirement.
    IllegalBreak,
    /// 4.3(e)/(f): the 8 left the table on the break.
    EightOnBreak,
    /// A declaration event that adjudicates no shot: an option, a placement, a spot request, the
    /// stalemate declaration.
    NoShot,
    /// 4.8/§5: the shooter won the rack.
    Win,
    /// 4.8/§5: the shooter lost the rack.
    Loss,
}

/// A foul's WPA rule id (`rules.md` §3's table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FoulRule {
    /// 3.1: the cue ball was pocketed or driven off the table.
    #[serde(rename = "3.1")]
    CueBallOffTheTable,
    /// 3.2: the first contact was not a legal ball for the target.
    #[serde(rename = "3.2")]
    WrongBallFirst,
    /// 3.3: nothing was pocketed and no ball reached a rail after the contact.
    #[serde(rename = "3.3")]
    NoRailAfterContact,
    /// 3.5: an object ball was driven off the table.
    #[serde(rename = "3.5")]
    BallOffTheTable,
    /// 3.11: shot from a restricted placement whose first contact was above the head string.
    #[serde(rename = "3.11")]
    FromAboveTheHeadString,
}

/// Why a foul fired: the machine token the corpus records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoulReason {
    /// 3.1: the cue ball was pocketed.
    CueBallPocketed,
    /// 3.1: the cue ball was driven off the table.
    CueBallOffTable,
    /// 3.2: the first contact was not a legal target.
    WrongBallFirst,
    /// 3.3: no ball reached a rail after the contact.
    NoRailAfterContact,
    /// 3.5: an object ball other than the 8 was driven off the table.
    ObjectBallOffTable,
    /// 3.5: the 8 was driven off the table (4.3(g) spots it; 4.8(d) loses the rack).
    EightDrivenOffTable,
    /// 3.11: a restricted placement whose first contact never crossed the head string.
    FromAboveTheHeadString,
}

/// One foul with the rule it comes from (`rules.md` §3's evidence table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Foul {
    /// The rule id.
    pub rule: FoulRule,
    /// The machine token.
    pub reason: FoulReason,
}

/// The machine's structured classification: the decision behind the record's `legality` text.
///
/// The label is the record's free-text field of the corpus schema; its leading token is what the
/// harness compares, since the corpus's tails carry per-entry commentary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// A legal ordinary shot (`rules.md` §3).
    LegalShot,
    /// A standard foul on an ordinary shot; the rule is the first foul in rule order, since 3
    /// chapeau's severity order puts every standard foul on the same penalty.
    StandardFoul {
        /// The first foul's rule id.
        rule: FoulRule,
    },
    /// The shooter won the rack (`rules.md` §5).
    Win,
    /// The shooter lost the rack (`rules.md` §5); the clause is 4.8(a)–(d).
    Loss {
        /// The loss clause.
        clause: &'static str,
    },
    /// 4.3(c): a ball was pocketed on the break.
    LegalBreakByPocket,
    /// 4.3(d): nothing pocketed, at least four distinct object balls driven to rails.
    LegalBreakClean,
    /// 4.3(d): nothing pocketed, fewer than four distinct object balls driven to rails.
    IllegalBreak {
        /// Whether the break shot was also a foul (it keeps this tree; CSI 2-3-3).
        foul: bool,
    },
    /// 4.3(e): the 8 pocketed on a legal break.
    EightOnBreakLegal,
    /// 4.3(f): the 8 pocketed or driven off the table on a foul break.
    EightOnBreakFoul,
    /// 4.3(g)/(h): any other break foul.
    BreakFoul {
        /// Whether an object ball was driven off the table (4.3(g)); otherwise 4.3(h).
        object_ball_off_table: bool,
    },
    /// An option of a tree was applied.
    OptionApplied {
        /// The tree.
        tree: Tree,
        /// The option's index.
        index: u8,
    },
    /// The stalemate declaration (`rules.md` §7).
    StalemateDeclaration,
    /// A cue-ball placement was accepted (`rules.md` §6; 3.10 is enforced at the input boundary).
    Placement,
    /// The 1.6 ¶2 spot request was applied to `ball`.
    SpotRequest {
        /// The spotted ball.
        ball: u8,
    },
}

impl Classification {
    /// The record's `legality` text: the token, the clause citation, and nothing else.
    ///
    /// The corpus's `legality` strings append per-entry commentary after the citation; the machine has
    /// no knowledge to write that, so the harness compares the leading token (see
    /// `tests/rules_corpus.rs`).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::LegalShot => "legal_shot (3.2–3.3)".to_string(),
            Self::StandardFoul { rule } => format!("standard_foul ({})", rule.id()),
            Self::Win => "eight_pocketed_win (4.4, 4.8)".to_string(),
            Self::Loss { clause } => format!("eight_pocketed_loss ({clause})"),
            Self::LegalBreakByPocket => "legal_break_by_pocket (4.3(c))".to_string(),
            Self::LegalBreakClean => "legal_break_clean (4.3(d))".to_string(),
            Self::IllegalBreak { foul: false } => "illegal_break_clean (4.3(d))".to_string(),
            Self::IllegalBreak { foul: true } => {
                "illegal_break_with_foul (4.3(d) + the foul rules)".to_string()
            }
            Self::EightOnBreakLegal => "eight_on_break_legal (4.3(e))".to_string(),
            Self::EightOnBreakFoul => "eight_on_break_foul (4.3(f))".to_string(),
            Self::BreakFoul {
                object_ball_off_table: true,
            } => "break_foul (4.3(g))".to_string(),
            Self::BreakFoul {
                object_ball_off_table: false,
            } => "break_foul (4.3(h))".to_string(),
            Self::OptionApplied { tree, index } => {
                format!("{} option {index} applied", tree.name())
            }
            Self::StalemateDeclaration => "stalemate_declaration".to_string(),
            Self::Placement => "cue_ball_placed (1.6)".to_string(),
            Self::SpotRequest { ball } => format!("spot_request_1_6 (1.6 ¶2): ball {ball}"),
        }
    }
}

/// The leading token of a legality label: the text before the first `(` or `;`.
#[must_use]
pub fn token_of(label: &str) -> &str {
    let end = label.find(['(', ';']).unwrap_or(label.len());
    label[..end].trim_end()
}

impl FoulRule {
    /// The rule's id as WPA writes it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::CueBallOffTheTable => "3.1",
            Self::WrongBallFirst => "3.2",
            Self::NoRailAfterContact => "3.3",
            Self::BallOffTheTable => "3.5",
            Self::FromAboveTheHeadString => "3.11",
        }
    }
}

/// What the machine applied to the table or the rack after adjudicating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 4.3(c): the breaker continues.
    Continue,
    /// The turn passes.
    PassTurn,
    /// An option tree was presented and a choice is awaited.
    OfferChoice,
    /// The balls are played where they lie (`rules-break.md` §3.5).
    AcceptInPosition,
    /// The rack is reconstructed from the snapshot (`rules-break.md` §3.7).
    ReRack,
    /// The cue ball goes in hand above the head string (4.3(f)(g)(h)).
    CueBallInHandAboveHeadString,
    /// A standard foul: the cue ball goes in hand anywhere (4.9).
    CueBallInHandAnywhere,
    /// The 8 is spotted and the balls are played in position (4.3(e)(1)).
    SpotEightAndAccept,
    /// The 8 is spotted and the cue ball goes in hand above the head string (4.3(f)(1)).
    SpotEightAndCueBallInHand,
    /// An accepted cue-ball placement was applied (`rules.md` §6).
    PlaceCue,
    /// The 1.6 ¶2 request spotted an object ball.
    SpotBall,
    /// The rack ended (`rules.md` §5).
    RackOver,
}

/// The actions applied after adjudication.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Apply {
    /// The action.
    pub action: Action,
    /// The rack's breaker after the action (`rules-break.md` §3.7 step 4).
    pub breaker: Player,
    /// Whether the rack seed is unchanged. The machine never re-derives or repairs it (§2.7).
    pub rack_seed_unchanged: bool,
    /// Whether the action reset the rack's shot counter (§3.7 step 5).
    pub shot_count_reset: bool,
    /// The balls this action spotted, ascending.
    pub spotted: Vec<u8>,
    /// The state the machine is in after the action.
    pub state: RulesState,
}

/// The full adjudication record for one input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Adjudication {
    /// The overall verdict.
    pub verdict: Verdict,
    /// The classification text (`break-corpus.schema.json`'s `legality`).
    pub legality: String,
    /// The fouls found, in rule order (3 chapeau: the penalty is the most serious).
    pub fouls: Vec<Foul>,
    /// The option sets this input presented or took an option from; empty when no tree applies.
    pub offers: Vec<Offer>,
    /// The option chosen, where a tree applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen_option: Option<ChosenOption>,
    /// What was applied.
    pub apply: Apply,
}
