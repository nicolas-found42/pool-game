//! The input log (`architecture.md` §6): the recorded free-choice sequence of a match.
//!
//! **Principle: the input log is the free-choice sequence; everything else is derived.** Racks, seeds'
//! expansions, adjudications, event logs, race scores, and re-rack snapshots are recomputed on replay.
//!
//! The types here mirror `docs/spec/input-log.schema.json` exactly — four entry kinds, the header, and
//! the strike declaration — with the schema's `additionalProperties: false` and its ranges enforced at
//! the parse boundary. No wall-clock value ever appears in the log.

use serde::{Deserialize, Deserializer, Serialize};

/// The log's header plus its entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputLog {
    /// Bumped only when an entry's meaning changes; never for additions or key reordering.
    pub format_version: u32,
    /// Profile id; the record lives at `config/profiles/<id>.json`.
    pub profile: String,
    /// Rack-construction seed; per-rack seeds derive from it (`rules-break.md` §2.7).
    pub match_seed: u64,
    /// Execution-noise stream seed; one draw per `from_policy` declaration, in log order.
    pub noise_seed: u64,
    /// The match's policy difficulty and checkpoint; inert when both seats are human.
    pub difficulty: Difficulty,
    /// Racks needed to win the match.
    pub race_target: u32,
    /// The free-choice sequence, in order.
    pub entries: Vec<Entry>,
}

/// A policy seat's difficulty: the level and the artifact identity it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Difficulty {
    /// The difficulty level.
    pub level: DifficultyLevel,
    /// Policy artifact identity (for example the sha256 of the served `.onnx`).
    pub checkpoint: String,
}

/// The four difficulty levels (`ai.md` §7: one policy, four levels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DifficultyLevel {
    /// High execution noise.
    Beginner,
    /// Medium noise.
    Intermediate,
    /// Low noise.
    Advanced,
    /// Near-zero noise.
    Pro,
}

/// A 2-vector in the table frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    /// x (mm, or a unit direction's x).
    pub x: f64,
    /// y (mm, or a unit direction's y).
    pub y: f64,
}

/// The declaration's call: a ball plus pocket, a safety, or — on the break only — nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Call {
    /// A called ball and pocket.
    Ball {
        /// The called ball, 1..=15.
        ball: BallNumber,
        /// Pocket id per the rules section's vocabulary.
        pocket: String,
    },
    /// A safety: passes the turn at the end of the shot.
    Safety,
    /// The break: no ball is called.
    Break,
}

/// A ball number, 1..=15, validated at the parse boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BallNumber(pub u8);

impl Serialize for BallNumber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for BallNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        if (1..=15).contains(&value) {
            Ok(Self(value))
        } else {
            Err(serde::de::Error::custom(format!(
                "ball {value} is outside 1..=15"
            )))
        }
    }
}

/// The placement domain the incoming player is placing within.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementDomain {
    /// Ball in hand above the head string (after a break foul).
    AboveHeadString,
    /// Ball in hand anywhere on the playing surface (after a standard foul).
    Anywhere,
}

/// One free choice: the four kinds of `architecture.md` §6's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    /// A cue-ball placement within its domain.
    Placement {
        /// The domain the placement must satisfy.
        domain: PlacementDomain,
        /// Cue-ball centre, table frame, millimetres.
        pos: Vec2,
    },
    /// The WPA 1.6 ¶2 request to spot the legal object ball nearest the head string.
    SpotRequest,
    /// One atomic shot declaration.
    Declaration(Declaration),
    /// One option of the current `AwaitingChoice` tree.
    Option {
        /// The option id, defined by the rules corpus.
        option_id: String,
    },
}

/// The atomic shot declaration: the call, the aim, the speed, the spin, the elevation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    /// True when a policy seat committed this declaration; the only ones execution noise applies to.
    pub from_policy: bool,
    /// The call.
    pub call: Call,
    /// Unit horizontal aim direction in the table frame.
    pub aim: Vec2,
    /// Cue-ball launch speed (mm/s).
    pub speed: f64,
    /// Tip offset, as fractions of the miscue envelope.
    pub spin: Spin,
    /// Cue elevation (rad).
    pub elevation: f64,
}

/// The cue-tip contact offset in the plane perpendicular to the cue axis, as fractions of the miscue
/// envelope: `1.0` is the limit, so `|(a, b)| <= 1`; `a > 0` is the shooter's right, `b > 0` above
/// centre. Enforced at the input boundary with a 1e-3 mm tolerance on the tip offset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spin {
    /// Right/left tip offset, envelope fraction.
    pub a: f64,
    /// Above/below-centre tip offset, envelope fraction.
    pub b: f64,
}

impl Spin {
    /// The envelope fraction's magnitude: `|(a, b)|`, compared against 1.0 with the ruled tolerance.
    #[must_use]
    pub fn magnitude(&self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }
}

impl InputLog {
    /// Parse a log from JSON text and validate the header's ranges.
    ///
    /// The types enforce the schema's shape (`additionalProperties: false`, the tagged unions, the
    /// ball range); this adds the numeric bounds a JSON Schema states but Rust types cannot.
    pub fn parse(text: &str) -> Result<Self, LogError> {
        let log: Self = serde_json::from_str(text).map_err(|e| LogError::Parse(e.to_string()))?;
        log.validate()?;
        Ok(log)
    }

    /// The header's numeric bounds (`docs/spec/input-log.schema.json`).
    pub fn validate(&self) -> Result<(), LogError> {
        if self.format_version < 1 {
            return Err(LogError::Invalid(
                "format_version must be at least 1".into(),
            ));
        }
        if self.race_target < 1 {
            return Err(LogError::Invalid("race_target must be at least 1".into()));
        }
        if self.profile.is_empty() {
            return Err(LogError::Invalid("profile must not be empty".into()));
        }
        if self.difficulty.checkpoint.is_empty() {
            return Err(LogError::Invalid(
                "difficulty.checkpoint must not be empty".into(),
            ));
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if let Entry::Declaration(declaration) = entry {
                if declaration.speed < 0.0 {
                    return Err(LogError::Invalid(format!(
                        "entries[{index}]: speed is negative"
                    )));
                }
                if declaration.elevation < 0.0 {
                    return Err(LogError::Invalid(format!(
                        "entries[{index}]: elevation is negative"
                    )));
                }
                if !declaration.speed.is_finite()
                    || !declaration.elevation.is_finite()
                    || !declaration.spin.a.is_finite()
                    || !declaration.spin.b.is_finite()
                    || !declaration.aim.x.is_finite()
                    || !declaration.aim.y.is_finite()
                {
                    return Err(LogError::Invalid(format!(
                        "entries[{index}]: a value is not finite"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Serialize to the schema's JSON. Entries keep their order; no key is ever dropped.
    pub fn to_json(&self) -> Result<String, LogError> {
        serde_json::to_string_pretty(self).map_err(|e| LogError::Parse(e.to_string()))
    }
}

/// Why a log could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogError {
    /// The document does not match the schema's shape.
    Parse(String),
    /// The document's shape is right but a bound is violated.
    Invalid(String),
}
