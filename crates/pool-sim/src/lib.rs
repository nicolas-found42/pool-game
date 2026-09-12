//! The physics core (`physics.md`), the frame and constants of `physics.md` §2/§5, the frozen rack
//! generator of `rules-break.md` §2, and the determinism primitives of `architecture.md` §11.
//!
//! The crate holds no file I/O (`architecture.md` §1): callers read profiles and corpora and hand the
//! values in. It holds no RNG either (`physics.md` §1) — the rack seed is an input.

pub mod ball;
pub mod constants;
pub mod facts;
pub mod frame;
pub mod math;
pub mod profile;
pub mod rack;
pub mod sim;
pub mod state_hash;
pub mod strike;
pub mod table;

pub use ball::{BallState, MotionMode};
pub use facts::{Fact, FactKind, RailSummary};
pub use frame::{Slot, slot_position_mm};
pub use math::V3;
pub use profile::{MuB, Profile};
pub use rack::Arrangement;
pub use sim::{
    PlacementError, RestBlock, Runaway, Segment, Shot, ShotOutcome, Sim, SupportedOverMouth,
};
pub use strike::{StrikeDecl, StrikeError, miscue_envelope_mm};
pub use table::{PocketId, Rail, Table};
