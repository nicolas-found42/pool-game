//! The rules layer's vocabulary (`rules.md` §1–§3): the machine's states, its inputs, and the record
//! it emits. These types live here, below `pool-match`, because both the machine and the input log
//! speak them (`architecture.md` §1).

use serde::{Deserialize, Serialize};

/// A 2-vector in the table frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    /// x (mm, or a unit direction's x).
    pub x: f64,
    /// y (mm, or a unit direction's y).
    pub y: f64,
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
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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

/// The declaration's call: a ball plus pocket, a safety, or — on the break only — nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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

/// The cue-tip contact offset as fractions of the miscue envelope (`physics.md` §4): `1.0` is the
/// limit, `|(a, b)| <= 1`; `a > 0` is the shooter's right, `b > 0` above centre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spin {
    /// Right/left tip offset, envelope fraction.
    pub a: f64,
    /// Above/below-centre tip offset, envelope fraction.
    pub b: f64,
}

impl Spin {
    /// The envelope fraction's magnitude: `|(a, b)|`.
    #[must_use]
    pub fn magnitude(&self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }
}

/// One atomic shot declaration — the machine's `AwaitingShot` input, the AI's action-space skeleton,
/// and the UX's authored object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShotDeclaration {
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

/// The placement domain the incoming player is placing within (`rules.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementDomain {
    /// Ball in hand above the head string (rack start, break fouls).
    AboveHeadString,
    /// Ball in hand anywhere on the playing surface (standard fouls).
    Anywhere,
}
