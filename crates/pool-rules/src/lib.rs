//! The 8-ball rules layer (`rules.md`): the rack-scoped pure function
//! `(state, input) → (adjudication record, state')`.
//!
//! The machine is rack-scoped; `pool-match` wraps it with the match bookkeeping and the `Session` drive
//! loop (`architecture.md` §8). The input vocabulary lives in [`vocab`]; [`machine::Rack`] is the
//! machine; [`facts`] is the rules side of the observation contract; [`placement`] and [`spot`] are the
//! geometry-shaped procedures `architecture.md` §9 assigns here.

pub mod break_rules;
pub mod facts;
pub mod machine;
pub mod offer;
pub mod placement;
pub mod rail;
pub mod record;
pub mod shot_rules;
pub mod spot;
pub mod state;
pub mod vocab;

pub use facts::{
    FactView, Observation, OutOfPlay, PocketName, PreBall, PreState, RestBall, RestView, RulesFact,
    SupportedBall,
};
pub use machine::{Input, InputError, Rack};
pub use offer::{ChosenOption, Offer, OfferOption, OptionId, Tree};
pub use placement::{PlacementFault, in_domain, validate};
pub use record::{
    Action, Adjudication, Apply, Classification, Foul, FoulReason, FoulRule, Verdict,
};
pub use spot::{Spot, spot_position};
pub use state::{Chooser, Group, Player, RulesState, Target, Winner};
pub use vocab::{BallNumber, Call, PlacementDomain, ShotDeclaration, Spin, Vec2};
