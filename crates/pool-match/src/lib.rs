//! The match layer (`architecture.md` §1/§6/§8): the input-log types, the match bookkeeping, the
//! `Session` drive loop, and the per-rack seed derivation.
//!
//! The workspace's shape: `pool-rules` is the rack-scoped machine, `pool-sim` the physics core, and
//! this crate is everything that outlives a rack — the race, the breaker, the seeds, and the one loop
//! the binaries drive.

pub mod log;
pub mod match_layer;
pub mod noise;
pub mod session;

pub use log::{
    Declaration, Difficulty, DifficultyLevel, Entry, FORMAT_VERSION, InputLog, LogError,
};
pub use match_layer::{MatchConfig, RACE_TARGET, breaker_of, first_breaker, rack_seed};
pub use noise::Noise;
pub use pool_rules::{BallNumber, Call, PlacementDomain, Spin, Vec2};
pub use session::{InputError, Replay, Request, Session, SessionView};
