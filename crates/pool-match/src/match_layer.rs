//! The match layer above the rack-scoped machine (`rules.md` §8, `architecture.md` §1/§5): the race,
//! the alternating breaker, and the per-rack seed derivation the rack generator consumes.
//!
//! The machine owns one rack; everything that outlives a rack lives here. Every value it owns is
//! **derived**, not stored: rack *i*'s seed and breaker are functions of the match seed and the index
//! alone, so a session that restores a re-racked rack re-derives nothing (§2.7: "Re-racks reuse the
//! rack's seed and never re-derive it") and a replay recomputes both from the log's header.

use pool_rng::SplitMix64;
use pool_rules::Player;
use pool_sim::Profile;

use crate::log::{Difficulty, InputLog};

/// Racks a match is raced to (`rules.md` §8).
pub const RACE_TARGET: u32 = 5;

/// Rack `index`'s seed (`rules-break.md` §2.7): the output of a `SplitMix64` stream seeded with
/// `match_seed` after `index + 1` steps.
///
/// The stream is a value, not session state: rack *i*'s seed is a function of the match seed and the
/// index, so neither the session that derives it nor the replay that recomputes it has to re-run the
/// draws of the racks before it.
#[must_use]
pub fn rack_seed(match_seed: u64, index: u32) -> u64 {
    let mut stream = SplitMix64::new(match_seed);
    let mut seed = stream.next_u64();
    for _ in 0..index {
        seed = stream.next_u64();
    }
    seed
}

/// The match's first breaker: one `next_below(2)` draw on a stream seeded with `match_seed`
/// (`rules.md` §8: "first breaker drawn from the match seed").
///
/// The sentence names the seed, not the draw's shape; this is the reading taken — one uniform bit,
/// `0` is `P1` — and it is reported to the spec's owner rather than hidden here.
#[must_use]
pub fn first_breaker(match_seed: u64) -> Player {
    match SplitMix64::new(match_seed).next_below(2) {
        0 => Player::P1,
        _ => Player::P2,
    }
}

/// Rack `index`'s breaker: the drawn first breaker, alternating after it (`rules.md` §8).
#[must_use]
pub fn breaker_of(match_seed: u64, index: u32) -> Player {
    let first = first_breaker(match_seed);
    if index.is_multiple_of(2) {
        first
    } else {
        first.other()
    }
}

/// Everything a session needs to start a match (`architecture.md` §8's `Session::new`).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchConfig {
    /// The profile record the simulation runs (`physics.md` §5); its `id` is what the log records.
    pub profile: Profile,
    /// Rack-construction seed; the per-rack seeds derive from it (`rules-break.md` §2.7).
    pub match_seed: u64,
    /// Execution-noise stream seed; one perturbation per `from_policy` declaration, in log order.
    pub noise_seed: u64,
    /// The policy difficulty; inert while no declaration carries `from_policy`.
    pub difficulty: Difficulty,
    /// Racks needed to win the match (`rules.md` §8's race target).
    pub race_target: u32,
}

impl MatchConfig {
    /// The configuration a recorded log's header describes (`architecture.md` §6).
    ///
    /// The caller hands the profile record in: `pool-match` does no file I/O (`architecture.md` §1),
    /// so loading `config/profiles/<id>.json` is the binary's or the test's job.
    #[must_use]
    pub fn from_log(log: &InputLog, profile: Profile) -> Self {
        Self {
            profile,
            match_seed: log.match_seed,
            noise_seed: log.noise_seed,
            difficulty: log.difficulty.clone(),
            race_target: log.race_target,
        }
    }
}
