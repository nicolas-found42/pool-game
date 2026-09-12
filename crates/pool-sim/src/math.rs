//! Minimal 3-vector arithmetic for the core (`physics.md` §2).
//!
//! Only `+ − × ÷` and `sqrt` appear here — the numeric discipline of `architecture.md` §3 — so the
//! same source produces the same bits on every conformant target.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A vector in the table frame (mm, mm/s, or rad/s depending on the quantity).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct V3 {
    /// x, along the long axis (head → foot positive).
    pub x: f64,
    /// y, across the table.
    pub y: f64,
    /// z, up.
    pub z: f64,
}

/// The free-function spelling of [`V3`]'s literal, for tables and constants.
#[must_use]
pub const fn v3(x: f64, y: f64, z: f64) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    /// The zero vector.
    pub const ZERO: Self = v3(0.0, 0.0, 0.0);

    /// The dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// The cross product, `self × other`.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        v3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// The Euclidean length.
    #[must_use]
    pub fn len(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The unit vector along `self`; the zero vector maps to itself.
    #[must_use]
    pub fn norm(self) -> Self {
        let length = self.len();
        if length == 0.0 {
            Self::ZERO
        } else {
            self / length
        }
    }

    /// The horizontal projection (`z` dropped).
    #[must_use]
    pub fn xy(self) -> Self {
        v3(self.x, self.y, 0.0)
    }

    /// Whether all three components are finite; a NaN or an infinity is a bug, never a state.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Component-wise maximum of the absolute values; the sleep test's norm (`physics.md` §5).
    #[must_use]
    pub fn abs_max(self) -> f64 {
        self.x.abs().max(self.y.abs()).max(self.z.abs())
    }
}

impl Add for V3 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        v3(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for V3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        v3(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f64> for V3 {
    type Output = Self;

    fn mul(self, scale: f64) -> Self {
        v3(self.x * scale, self.y * scale, self.z * scale)
    }
}

impl Div<f64> for V3 {
    type Output = Self;

    fn div(self, scale: f64) -> Self {
        v3(self.x / scale, self.y / scale, self.z / scale)
    }
}

impl Neg for V3 {
    type Output = Self;

    fn neg(self) -> Self {
        v3(-self.x, -self.y, -self.z)
    }
}

impl From<V3> for [f64; 3] {
    fn from(value: V3) -> Self {
        [value.x, value.y, value.z]
    }
}

impl From<[f64; 3]> for V3 {
    fn from(value: [f64; 3]) -> Self {
        v3(value[0], value[1], value[2])
    }
}

impl std::fmt::Display for V3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:.3}, {:.3}, {:.3})", self.x, self.y, self.z)
    }
}
