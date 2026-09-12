//! The break option trees (`rules-break.md` §3, `rules.md` §4): their ids and their choosers.
//!
//! `rules-break.json`'s `trees` section is the contract for the ids and the choosers; the option texts
//! are WPA's 4.3 wording, held here because the corpus records them in every offer it asserts. An
//! offer is a value: the machine presents it, the state carries it, and the `option` input names one
//! of its ids.

use serde::{Deserialize, Serialize};

use crate::state::Chooser;

/// One of the declared option trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tree {
    /// 4.3(d): nothing pocketed and fewer than four distinct object balls driven to rails.
    IllegalBreak,
    /// 4.3(f)(g)(h): a break foul other than the 8-on-break cases.
    BreakFoul,
    /// 4.3(e): the 8 pocketed on a legal break.
    EightOnBreakLegal,
    /// 4.3(f): the 8 pocketed or driven off the table on a foul break.
    EightOnBreakFoul,
    /// 1.13/4.11: the stalemate declaration, re-racked by mutual agreement (`rules.md` §7).
    Stalemate,
}

impl Tree {
    /// The tree's name, as the corpus writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::IllegalBreak => "illegal_break",
            Self::BreakFoul => "break_foul",
            Self::EightOnBreakLegal => "eight_on_break_legal",
            Self::EightOnBreakFoul => "eight_on_break_foul",
            Self::Stalemate => "stalemate",
        }
    }

    /// Who chooses among the tree's options.
    #[must_use]
    pub const fn chooser(self) -> Chooser {
        match self {
            Self::IllegalBreak | Self::BreakFoul | Self::EightOnBreakFoul => Chooser::Incoming,
            Self::EightOnBreakLegal => Chooser::Breaker,
            Self::Stalemate => Chooser::Other,
        }
    }

    /// The tree's options, in WPA's order.
    #[must_use]
    pub const fn options(self) -> &'static [OptionId] {
        match self {
            Self::IllegalBreak => &[
                OptionId::AcceptInPosition,
                OptionId::ReRackAndBreak,
                OptionId::ReRackAndOffenderBreaks,
            ],
            Self::BreakFoul => &[
                OptionId::AcceptInPosition,
                OptionId::CueBallInHandAboveHeadString,
            ],
            Self::EightOnBreakLegal => &[OptionId::SpotEightAndAccept, OptionId::ReBreak],
            Self::EightOnBreakFoul => &[
                OptionId::SpotEightAndCueBallInHandAboveHeadString,
                OptionId::ReBreak,
            ],
            Self::Stalemate => &[OptionId::ReRackAndOriginalBreakerBreaks],
        }
    }

    /// The tree naming `token`, if any — the inverse of the option ids' names for the log's
    /// `option_id` inputs.
    #[must_use]
    pub fn by_name(token: &str) -> Option<Self> {
        [
            Self::IllegalBreak,
            Self::BreakFoul,
            Self::EightOnBreakLegal,
            Self::EightOnBreakFoul,
            Self::Stalemate,
        ]
        .into_iter()
        .find(|tree| tree.name() == token)
    }

    /// The offer this tree presents.
    #[must_use]
    pub fn offer(self) -> Offer {
        Offer {
            tree: self,
            chooser: self.chooser(),
            options: self
                .options()
                .iter()
                .enumerate()
                .map(|(index, option)| OfferOption {
                    index: index as u8 + 1,
                    option: *option,
                    description: option.description().to_string(),
                })
                .collect(),
        }
    }
}

/// One option of a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionId {
    /// 4.3(d)(1), 4.3(e)(1), 4.3(g)(1), 4.3(h)(1): play the balls where they lie.
    AcceptInPosition,
    /// 4.3(d)(2): re-rack and the incoming player breaks.
    ReRackAndBreak,
    /// 4.3(d)(3): re-rack and the offending breaker breaks again.
    ReRackAndOffenderBreaks,
    /// 4.3(f)(1), 4.3(g)(2), 4.3(h)(2): cue ball in hand above the head string.
    CueBallInHandAboveHeadString,
    /// 4.3(e)(1): spot the 8 and take the balls in position.
    SpotEightAndAccept,
    /// 4.3(e)(2), 4.3(f)(2): reconstruct the rack and break again.
    ReBreak,
    /// 4.3(f)(1): spot the 8 and shoot with the cue ball in hand above the head string.
    SpotEightAndCueBallInHandAboveHeadString,
    /// 1.13/4.11: re-rack and the original breaker breaks again.
    ReRackAndOriginalBreakerBreaks,
}

impl OptionId {
    /// The option's id, as the corpus writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AcceptInPosition => "accept_in_position",
            Self::ReRackAndBreak => "re_rack_and_break",
            Self::ReRackAndOffenderBreaks => "re_rack_and_offender_breaks",
            Self::CueBallInHandAboveHeadString => "cue_ball_in_hand_above_head_string",
            Self::SpotEightAndAccept => "spot_eight_and_accept",
            Self::ReBreak => "re_break",
            Self::SpotEightAndCueBallInHandAboveHeadString => {
                "spot_eight_and_cue_ball_in_hand_above_head_string"
            }
            Self::ReRackAndOriginalBreakerBreaks => "re_rack_and_original_breaker_breaks",
        }
    }

    /// The option's human-readable text (WPA 4.3's wording).
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::AcceptInPosition => "Accept the table in position",
            Self::ReRackAndBreak => "Re-rack and break",
            Self::ReRackAndOffenderBreaks => {
                "Re-rack and allow the offending player to break again"
            }
            Self::CueBallInHandAboveHeadString => "Take cue ball in hand above the head string",
            Self::SpotEightAndAccept => "Spot the 8-ball and accept the balls in position",
            Self::ReBreak => "Re-break",
            Self::SpotEightAndCueBallInHandAboveHeadString => {
                "Spot the 8-ball and shoot with cue ball in hand above the head string"
            }
            Self::ReRackAndOriginalBreakerBreaks => {
                "Re-rack and the original breaker breaks again (mutual agreement)"
            }
        }
    }

    /// The option id `token`, if it names one — the log's `option_id` input (`architecture.md` §6).
    #[must_use]
    pub fn by_name(token: &str) -> Option<Self> {
        [
            Self::AcceptInPosition,
            Self::ReRackAndBreak,
            Self::ReRackAndOffenderBreaks,
            Self::CueBallInHandAboveHeadString,
            Self::SpotEightAndAccept,
            Self::ReBreak,
            Self::SpotEightAndCueBallInHandAboveHeadString,
            Self::ReRackAndOriginalBreakerBreaks,
        ]
        .into_iter()
        .find(|option| option.name() == token)
    }

    /// This option's index within its tree, from 1.
    #[must_use]
    pub fn index_in(self, tree: Tree) -> Option<u8> {
        tree.options()
            .iter()
            .position(|option| *option == self)
            .map(|index| index as u8 + 1)
    }
}

/// One option in a presented offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferOption {
    /// The option's index, from 1.
    pub index: u8,
    /// The option.
    pub option: OptionId,
    /// The option's text.
    pub description: String,
}

/// One tree's presented option set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    /// The tree.
    pub tree: Tree,
    /// Who chooses.
    pub chooser: Chooser,
    /// The options, in order.
    pub options: Vec<OfferOption>,
}

/// The option taken from a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChosenOption {
    /// The tree the option came from.
    pub tree: Tree,
    /// The chosen option's index, from 1.
    pub index: u8,
    /// The chosen option.
    pub option: OptionId,
}

impl ChosenOption {
    /// The option `option` of `tree`, as chosen.
    #[must_use]
    pub fn of(tree: Tree, option: OptionId) -> Self {
        Self {
            tree,
            index: option.index_in(tree).unwrap_or_else(|| {
                panic!("option {} is not in tree {}", option.name(), tree.name())
            }),
            option,
        }
    }
}
