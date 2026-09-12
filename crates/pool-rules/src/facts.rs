//! The rules side of the observation contract (`rules.md` §10, `rules-break.md` §"Corpus contract"):
//! the four fact kinds the rules layer reads, the rest state with its annotations, and the pre-shot
//! state.
//!
//! The serde shapes are the corpus's: `docs/spec/rules-break.json`'s entries and this view are the same
//! data, so a corpus entry and a played shot drive the machine identically. The mapping from
//! `pool_sim`'s fact stream drops the kinds no rule reads (`physics.md` §7's jaw, kick, rest, mode and
//! depenetration events) and keeps the four, with the rail contact's two first-class frozen fields
//! (§10.2) and the pocket's identity (§10.3).

use pool_sim::{BallState, Fact, FactKind, PocketId, RestBlock, SupportedOverMouth};
use serde::{Deserialize, Serialize};

use crate::vocab::PlacementDomain;

/// A pocket's identity in the rules tier's vocabulary (`break-corpus.schema.json`'s `pocketId`).
///
/// The frame's `y` axis is the table's own left/right (`physics.md` §2; `Rail::RightLong` is `y` =
/// +635, `physics-break.json`'s `pb-13` pins ball 4 at `y` = +606.4 as "the right long cushion"), so
/// `+y` is right and `−y` is left in the pocket names too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PocketName {
    /// The foot corner at `y` < 0.
    FootLeft,
    /// The foot corner at `y` > 0.
    FootRight,
    /// The side mouth at `y` < 0.
    SideLeft,
    /// The side mouth at `y` > 0.
    SideRight,
    /// The head corner at `y` < 0.
    HeadLeft,
    /// The head corner at `y` > 0.
    HeadRight,
}

impl From<PocketId> for PocketName {
    fn from(pocket: PocketId) -> Self {
        match pocket {
            PocketId::FootMinusY => Self::FootLeft,
            PocketId::FootPlusY => Self::FootRight,
            PocketId::SideMinusY => Self::SideLeft,
            PocketId::SidePlusY => Self::SideRight,
            PocketId::HeadMinusY => Self::HeadLeft,
            PocketId::HeadPlusY => Self::HeadRight,
        }
    }
}

impl PocketName {
    /// The pocket's `PocketId` in the simulation's vocabulary.
    #[must_use]
    pub const fn pocket_id(self) -> PocketId {
        match self {
            Self::FootLeft => PocketId::FootMinusY,
            Self::FootRight => PocketId::FootPlusY,
            Self::SideLeft => PocketId::SideMinusY,
            Self::SideRight => PocketId::SidePlusY,
            Self::HeadLeft => PocketId::HeadMinusY,
            Self::HeadRight => PocketId::HeadPlusY,
        }
    }

    /// The pocket's name in the corpus's vocabulary: what the input log's `call.pocket` carries.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FootLeft => "foot_left",
            Self::FootRight => "foot_right",
            Self::SideLeft => "side_left",
            Self::SideRight => "side_right",
            Self::HeadLeft => "head_left",
            Self::HeadRight => "head_right",
        }
    }
}

/// One rules-tier fact (`rules-break.md`'s corpus contract).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RulesFact {
    /// Two balls touched.
    BallBall {
        /// Event time (s).
        t_s: f64,
        /// One ball's id.
        a: u8,
        /// The other ball's id.
        b: u8,
    },
    /// A ball touched a cushion. The corpus writes the two words of the carom's rail; the sim's
    /// per-contact `frozen_at_shot_start` / `left_since_shot_start` pair (`rules.md` §10.2) rides
    /// along, absent from the corpus's shorthand and defaulting to "not frozen".
    Rail {
        /// Event time (s).
        t_s: f64,
        /// The ball that touched the rail.
        ball: u8,
        /// Was the ball in contact with this rail in the state at shot start?
        #[serde(default)]
        frozen_at_shot_start: bool,
        /// Has the ball separated from this rail since shot start?
        #[serde(default)]
        left_since_shot_start: bool,
    },
    /// A ball was pocketed, with the pocket's identity (`rules.md` §10.3).
    Pocket {
        /// Event time (s).
        t_s: f64,
        /// The ball that dropped.
        ball: u8,
        /// The pocket it dropped into.
        pocket: PocketName,
    },
    /// A ball left the playing surface.
    OffTable {
        /// Event time (s).
        t_s: f64,
        /// The ball that left.
        ball: u8,
    },
}

impl RulesFact {
    /// The fact's event time (s).
    #[must_use]
    pub const fn t_s(&self) -> f64 {
        match *self {
            Self::BallBall { t_s, .. }
            | Self::Rail { t_s, .. }
            | Self::Pocket { t_s, .. }
            | Self::OffTable { t_s, .. } => t_s,
        }
    }

    /// Whether the fact is a ball–ball contact.
    #[must_use]
    pub const fn is_ball_ball(&self) -> bool {
        matches!(self, Self::BallBall { .. })
    }
}

/// The facts of one shot with their simultaneity groups (`rules.md` §10.1).
///
/// The groups are indices into `facts`; index order is the stream's `seq` order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactView {
    /// The facts, in the stream's order.
    pub facts: Vec<RulesFact>,
    /// Groups the simulation cannot order within ε, each a list of fact indices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub simultaneity_groups: Vec<Vec<usize>>,
}

impl FactView {
    /// The rules-side view of the simulation's fact stream (`pool_sim::Fact`).
    ///
    /// The rail's own identity is not kept: no rule in this layer reads *which* cushion a contact was
    /// with — 2.7's count is per ball (`rules-break.md` §3.1) — and the corpus's rail facts carry only
    /// the ball. The frozen pair is kept, per contact, because 2.7's exception is what it decides.
    #[must_use]
    pub fn from_sim(facts: &[Fact]) -> Self {
        let mut view = Self {
            facts: Vec::new(),
            simultaneity_groups: Vec::new(),
        };
        let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
        for fact in facts {
            let converted = match fact.kind {
                FactKind::BallBall { a, b } => Some(RulesFact::BallBall { t_s: fact.t, a, b }),
                FactKind::RailContact {
                    ball,
                    frozen_at_shot_start,
                    left_since_shot_start,
                    ..
                } => Some(RulesFact::Rail {
                    t_s: fact.t,
                    ball,
                    frozen_at_shot_start,
                    left_since_shot_start,
                }),
                FactKind::Pocketed { ball, pocket } => Some(RulesFact::Pocket {
                    t_s: fact.t,
                    ball,
                    pocket: pocket.into(),
                }),
                FactKind::OffTable { ball } => Some(RulesFact::OffTable { t_s: fact.t, ball }),
                FactKind::JawContact { .. }
                | FactKind::Kick { .. }
                | FactKind::Rest { .. }
                | FactKind::SlideToRoll { .. }
                | FactKind::RollToStop { .. }
                | FactKind::SpinDown { .. }
                | FactKind::Depenetration { .. } => None,
            };
            let Some(converted) = converted else {
                continue;
            };
            let index = view.facts.len();
            view.facts.push(converted);
            if let Some(group) = fact.group {
                match groups.iter_mut().find(|(id, _)| *id == group) {
                    Some((_, members)) => members.push(index),
                    None => groups.push((group, vec![index])),
                }
            }
        }
        // A group of one is no simultaneity at all; keep only the real ones, in index order.
        let mut counted: Vec<(usize, Vec<usize>)> = groups
            .into_iter()
            .filter(|(_, members)| members.len() > 1)
            .map(|(_, members)| (members[0], members))
            .collect();
        counted.sort_unstable_by_key(|(first, _)| *first);
        view.simultaneity_groups = counted.into_iter().map(|(_, members)| members).collect();
        view
    }

    /// The indices of a group that contains the fact at `index`, if any.
    #[must_use]
    pub fn group_of(&self, index: usize) -> Vec<usize> {
        self.simultaneity_groups
            .iter()
            .find(|group| group.contains(&index))
            .cloned()
            .unwrap_or_else(|| vec![index])
    }
}

/// One ball's rest state (`rules.md` §10.6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestBall {
    /// The ball's id.
    pub id: u8,
    /// Position (mm, table frame).
    pub x_mm: f64,
    /// Position (mm, table frame).
    pub y_mm: f64,
    /// Height above the playing surface (mm).
    pub z_mm: f64,
    /// Velocity (mm/s).
    pub vx_mm_s: f64,
    /// Velocity (mm/s).
    pub vy_mm_s: f64,
    /// Velocity (mm/s).
    pub vz_mm_s: f64,
    /// Angular velocity (rad/s).
    pub wx: f64,
    /// Angular velocity (rad/s).
    pub wy: f64,
    /// Angular velocity (rad/s).
    pub wz: f64,
}

/// A ball at rest inside a pocket mouth, held up by another (`rules.md` §10.7): the one route by which
/// a ball counts as pocketed without a drop event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportedBall {
    /// The ball whose centre lies inside the mouth.
    pub ball: u8,
    /// The balls it rests on.
    pub supporting: Vec<u8>,
}

/// The rest state after a shot (`rules.md` §10.6–§10.7): the balls still on the table, the balls held
/// over a mouth, and the balls in contact with a rail.
///
/// A ball absent from `balls` is out of play — pocketed or off the table; neither is on the surface,
/// and the machine reads which from the facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestView {
    /// The balls still on the table.
    pub balls: Vec<RestBall>,
    /// The §10.7 supported-over-mouth annotation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_over_mouth: Vec<SupportedBall>,
    /// The balls in contact with a rail at rest, ascending: the next shot's frozen annotation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen_rails: Vec<u8>,
}

impl RestView {
    /// The rules-side view of the simulation's rest block.
    #[must_use]
    pub fn from_sim(rest: &RestBlock) -> Self {
        let balls = rest
            .states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.in_play())
            .map(|(id, state)| RestBall {
                id: id as u8,
                x_mm: state.pos_mm[0],
                y_mm: state.pos_mm[1],
                z_mm: state.pos_mm[2],
                vx_mm_s: state.vel_mm_s[0],
                vy_mm_s: state.vel_mm_s[1],
                vz_mm_s: state.vel_mm_s[2],
                wx: state.spin_rad_s[0],
                wy: state.spin_rad_s[1],
                wz: state.spin_rad_s[2],
            })
            .collect();
        let supported_over_mouth = rest
            .supported_over_mouth
            .iter()
            .map(SupportedBall::from_sim)
            .collect();
        let frozen_rails = rest.frozen_rails.iter().map(|(ball, _)| *ball).collect();
        Self {
            balls,
            supported_over_mouth,
            frozen_rails,
        }
    }

    /// Whether a ball is still on the table.
    #[must_use]
    pub fn on_table(&self, ball: u8) -> bool {
        self.balls.iter().any(|state| state.id == ball)
    }

    /// A ball's rest position, if it is on the table.
    #[must_use]
    pub fn position(&self, ball: u8) -> Option<[f64; 2]> {
        self.balls
            .iter()
            .find(|state| state.id == ball)
            .map(|state| [state.x_mm, state.y_mm])
    }

    /// The balls out of play at rest, ascending: off the table by position, pocketed by absence. The
    /// split between the two comes from the facts, not from this block.
    #[must_use]
    pub fn out_of_play(&self) -> Vec<u8> {
        (0..=15).filter(|ball| !self.on_table(*ball)).collect()
    }
}

impl SupportedBall {
    fn from_sim(supported: &SupportedOverMouth) -> Self {
        Self {
            ball: supported.ball,
            supporting: supported.supporting_balls.clone(),
        }
    }
}

/// Which balls left the table and how (`rules.md` §10.3, §10.4, §10.7).
///
/// The facts name *which* way a ball left; the rest block corroborates it and adds the one route with
/// no drop event (§10.7's supported-over-mouth annotation, which the rules layer counts as pocketed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfPlay {
    /// The balls pocketed, with the pocket each dropped into, ascending by ball. A ball counted
    /// pocketed by §10.7's annotation has no pocket identity: it never dropped.
    pub pocketed: Vec<(u8, Option<PocketName>)>,
    /// The balls driven off the table, ascending.
    pub off_table: Vec<u8>,
}

impl OutOfPlay {
    /// The out-of-play sets of a shot's report.
    #[must_use]
    pub fn of(observation: &Observation) -> Self {
        let mut pocketed: Vec<(u8, Option<PocketName>)> = Vec::new();
        let mut off_table: Vec<u8> = Vec::new();
        for fact in &observation.facts.facts {
            match *fact {
                RulesFact::Pocket { ball, pocket, .. }
                    if !pocketed.iter().any(|(id, _)| *id == ball) =>
                {
                    pocketed.push((ball, Some(pocket)));
                }
                RulesFact::OffTable { ball, .. } if !off_table.contains(&ball) => {
                    off_table.push(ball);
                }
                _ => {}
            }
        }
        for supported in &observation.rest.supported_over_mouth {
            if !pocketed.iter().any(|(id, _)| *id == supported.ball) {
                pocketed.push((supported.ball, None));
            }
        }
        pocketed.retain(|(ball, _)| !off_table.contains(ball));
        pocketed.sort_unstable_by_key(|(ball, _)| *ball);
        off_table.sort_unstable();
        Self {
            pocketed,
            off_table,
        }
    }

    /// Whether a ball was pocketed.
    #[must_use]
    pub fn contains_pocketed(&self, ball: u8) -> bool {
        self.pocketed.iter().any(|(id, _)| *id == ball)
    }

    /// Whether a ball was driven off the table.
    #[must_use]
    pub fn contains_off_table(&self, ball: u8) -> bool {
        self.off_table.contains(&ball)
    }

    /// Whether a ball left the table either way.
    #[must_use]
    pub fn contains(&self, ball: u8) -> bool {
        self.contains_pocketed(ball) || self.contains_off_table(ball)
    }

    /// The pocket a ball dropped into, when the observation named one.
    #[must_use]
    pub fn pocket_of(&self, ball: u8) -> Option<PocketName> {
        self.pocketed
            .iter()
            .find(|(id, _)| *id == ball)
            .and_then(|(_, pocket)| *pocket)
    }

    /// The pocketed balls, ascending.
    #[must_use]
    pub fn pocketed_balls(&self) -> Vec<u8> {
        self.pocketed.iter().map(|(ball, _)| *ball).collect()
    }

    /// The object balls driven off the table, ascending; the cue ball is not one.
    #[must_use]
    pub fn off_table_objects(&self) -> Vec<u8> {
        self.off_table
            .iter()
            .copied()
            .filter(|ball| *ball != 0)
            .collect()
    }

    /// Whether any ball at all was pocketed: 3.3's exemption.
    #[must_use]
    pub fn any_pocketed(&self) -> bool {
        !self.pocketed.is_empty()
    }
}

/// One ball of the pre-shot state (`rules.md` §10.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreBall {
    /// The ball's id.
    pub id: u8,
    /// Centre position (mm, table frame).
    pub x_mm: f64,
    /// Centre position (mm, table frame).
    pub y_mm: f64,
}

/// The pre-shot state (`rules.md` §10.8): the ball array and the placement geometry, for the frozen
/// and head-string predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreState {
    /// The balls on the table before the shot.
    pub balls: Vec<PreBall>,
    /// The balls in contact with a rail at shot start (2.7's exception, as the corpus declares it);
    /// a rail contact's own `frozen_at_shot_start` field wins where the two disagree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen_to_rail: Vec<u8>,
    /// The domain the shot was played from, when it followed a ball-in-hand placement: 3.11's
    /// precondition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_domain: Option<PlacementDomain>,
}

impl PreState {
    /// The pre-shot state from a ball array (`Arrangement::rest_states` at the rack's start, or the
    /// previous shot's rest block), with the frozen-rail annotation carried over.
    #[must_use]
    pub fn from_states(states: &[BallState; 16], frozen_to_rail: Vec<u8>) -> Self {
        let balls = states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.in_play())
            .map(|(id, state)| PreBall {
                id: id as u8,
                x_mm: state.pos_mm[0],
                y_mm: state.pos_mm[1],
            })
            .collect();
        Self {
            balls,
            frozen_to_rail,
            placement_domain: None,
        }
    }

    /// The pre-shot state of the shot that follows `rest`.
    #[must_use]
    pub fn after(rest: &RestView) -> Self {
        Self {
            balls: rest
                .balls
                .iter()
                .map(|state| PreBall {
                    id: state.id,
                    x_mm: state.x_mm,
                    y_mm: state.y_mm,
                })
                .collect(),
            frozen_to_rail: rest.frozen_rails.clone(),
            placement_domain: None,
        }
    }

    /// A ball's position before the shot, if it was on the table.
    #[must_use]
    pub fn position(&self, ball: u8) -> Option<[f64; 2]> {
        self.balls
            .iter()
            .find(|state| state.id == ball)
            .map(|state| [state.x_mm, state.y_mm])
    }

    /// The balls on the table before the shot, ascending.
    #[must_use]
    pub fn on_table(&self) -> Vec<u8> {
        let mut balls: Vec<u8> = self.balls.iter().map(|state| state.id).collect();
        balls.sort_unstable();
        balls
    }
}

/// One shot's report (`rules.md` §10): what the machine reads to adjudicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// The ordered facts and their simultaneity groups.
    pub facts: FactView,
    /// The rest state.
    pub rest: RestView,
    /// The state before the shot.
    pub pre_state: PreState,
}

impl Observation {
    /// The rules-side view of one simulation run.
    #[must_use]
    pub fn from_sim(facts: &[Fact], rest: &RestBlock, pre_state: PreState) -> Self {
        Self {
            facts: FactView::from_sim(facts),
            rest: RestView::from_sim(rest),
            pre_state,
        }
    }
}
