//! Throwaway AI-pipeline spine prototype for wayfinder ticket #16.
//!
//! Nothing here ships. The spec section it feeds is `docs/spec/ai-constants.md`; the measurement
//! harness is `src/bin/measure.rs`, the ONNX serve path `src/bin/serve.rs`.

pub mod encode;
pub mod gen;
pub mod geom;
pub mod planner;
pub mod positions;
pub mod rng;
pub mod toy_sim;
