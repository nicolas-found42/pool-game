//! The physics core (`physics.md`), the frame and constants of `physics.md` §2/§5, the frozen rack
//! generator of `rules-break.md` §2, and the determinism primitives of `architecture.md` §11.
//!
//! The crate holds no file I/O (`architecture.md` §1): callers read profiles and corpora and hand the
//! values in. It holds no RNG either (`physics.md` §1) — the rack seed is an input.

pub mod ball;
pub mod constants;
pub mod frame;
pub mod profile;
pub mod rack;
pub mod state_hash;

pub use ball::{BallState, MotionMode};
pub use frame::{Slot, slot_position_mm};
pub use profile::{MuB, Profile};
pub use rack::Arrangement;
