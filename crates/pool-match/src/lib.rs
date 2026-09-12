//! The match layer (`architecture.md` §1/§6/§8): the input-log types, the `Session` drive loop, and the
//! per-rack seed derivation.
//!
//! M2 fills the drive loop; the log types are the replay contract and land first.

pub mod log;

pub use log::{Declaration, Difficulty, DifficultyLevel, Entry, InputLog, LogError};
pub use pool_rules::{BallNumber, Call, PlacementDomain, Spin, Vec2};
