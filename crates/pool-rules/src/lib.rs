//! The 8-ball rules layer (`rules.md`): the rack-scoped pure function
//! `(state, input) → (adjudication record, state')`.
//!
//! The machine is rack-scoped; `pool-match` wraps it with the match bookkeeping and the `Session` drive
//! loop (`architecture.md` §8). The vocabulary lives in [`vocab`]; the machine itself lands with M2.

pub mod vocab;

pub use vocab::{BallNumber, Call, PlacementDomain, ShotDeclaration, Spin, Vec2};
