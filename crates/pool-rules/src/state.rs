//! The machine's states and the vocabulary they speak (`rules.md` §1–§3): who waits for what.
//!
//! `RulesState` is the machine's `(state, input) → (record, state')` half that persists; the serde
//! shapes are the corpus's (`break-corpus.schema.json`'s `state` definitions), so an adjudication
//! record reads the same whether it came from the corpus or from a played shot.

use serde::{Deserialize, Serialize};

use crate::offer::Offer;
use crate::vocab::PlacementDomain;

/// One of the rack's two seats. The breaker alternates per rack in the match layer (`rules.md` §8);
/// the rack machine only ever asks which seat is the breaker and which is the incoming player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Player {
    /// Seat one.
    P1,
    /// Seat two.
    P2,
}

impl Player {
    /// The other seat.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::P1 => Self::P2,
            Self::P2 => Self::P1,
        }
    }

    /// The seat's index, 0 for `P1` and 1 for `P2`.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
        }
    }
}

/// Who chooses among a tree's options (`break-corpus.schema.json`'s `roleChooser`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chooser {
    /// The incoming player: the seat that is not the rack's breaker.
    Incoming,
    /// The breaker of the rack.
    Breaker,
    /// The other player (`rules.md` §7's stalemate offer, which either player may raise).
    Other,
}

/// An object-ball group (`rules.md` §2/§4.4): balls 1–7 or 9–15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Balls 1–7.
    Solids,
    /// Balls 9–15.
    Stripes,
}

impl Group {
    /// The group a ball belongs to; `None` for the cue ball and the 8.
    #[must_use]
    pub const fn of(ball: u8) -> Option<Self> {
        match ball {
            1..=7 => Some(Self::Solids),
            9..=15 => Some(Self::Stripes),
            _ => None,
        }
    }

    /// The group's seven balls, ascending.
    #[must_use]
    pub const fn balls(self) -> [u8; 7] {
        match self {
            Self::Solids => [1, 2, 3, 4, 5, 6, 7],
            Self::Stripes => [9, 10, 11, 12, 13, 14, 15],
        }
    }

    /// The other group.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Solids => Self::Stripes,
            Self::Stripes => Self::Solids,
        }
    }
}

/// The shooter's target class (`rules.md` §2/§4.4): what a legal first contact may be.
///
/// The group's *identity* lives in the rack's assignment; this is the state's view of it, exactly as
/// the corpus writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The table is open: any object ball but the 8 (`rules.md` §2).
    Open,
    /// The shooter's group, while it still has balls on the table.
    Group,
    /// The 8, with the shooter's group cleared (`rules.md` §5).
    OnTheEight,
}

/// The rack's outcome (`rules.md` §1): who won it, or a drawn rack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Winner {
    /// Seat one won.
    P1,
    /// Seat two won.
    P2,
    /// A drawn rack. `rules.md` §1's table names it; no rule in this spec produces one (1.13's
    /// stalemate is re-racked by mutual agreement, §7).
    Drawn,
}

impl Winner {
    /// The winner of a seat.
    #[must_use]
    pub const fn of(player: Player) -> Self {
        match player {
            Player::P1 => Self::P1,
            Player::P2 => Self::P2,
        }
    }
}

/// What the machine awaits (`rules.md` §1's table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RulesState {
    /// A player must place the cue ball within `domain` (`rules.md` §6).
    AwaitingPlacement {
        /// Who places (and then shoots).
        shooter: Player,
        /// The placement domain.
        domain: PlacementDomain,
    },
    /// A player is at the table and must declare one atomic shot.
    AwaitingShot {
        /// Who shoots.
        shooter: Player,
        /// The shooter's target class.
        target: Target,
    },
    /// A player must choose one option of the presented offers (`rules-break.md` §3).
    AwaitingChoice {
        /// Who chooses.
        chooser: Chooser,
        /// The offer sets presented.
        offers: Vec<Offer>,
    },
    /// The rack has ended.
    RackOver {
        /// Who won it.
        winner: Winner,
    },
    /// The match has ended. The rack machine never enters this state; the match layer owns the race
    /// (`rules.md` §8) and the state exists for the machine-shape vocabulary of §1.
    MatchOver {
        /// Who won the match.
        winner: Player,
    },
}

impl RulesState {
    /// The state's kind, as its tag reads.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::AwaitingPlacement { .. } => "awaiting_placement",
            Self::AwaitingShot { .. } => "awaiting_shot",
            Self::AwaitingChoice { .. } => "awaiting_choice",
            Self::RackOver { .. } => "rack_over",
            Self::MatchOver { .. } => "match_over",
        }
    }
}
