//! The rack's slot geometry (`rules-break.md` §2.4): a closed form, never a coordinate table.

use crate::constants::{BALL_DIAMETER_MM, FOOT_SPOT_X_MM, RACK_ROW_STEP_MM};

/// A rack slot: `row` 1..=5 (apex first, at the foot spot), `index` 0..row within the row, ascending
/// from the y-negative end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    /// Row number, 1 (apex) through 5 (back row).
    pub row: u8,
    /// In-row index, `0..row`, ascending from the y-negative end.
    pub index: u8,
}

impl Slot {
    /// A slot from its parts; `row` is 1..=5 and `index < row`.
    ///
    /// # Panics
    ///
    /// Panics if `row` is outside `1..=5`, or `index` is not below `row`.
    #[must_use]
    pub const fn new(row: u8, index: u8) -> Self {
        assert!(row >= 1 && row <= 5, "row out of range");
        assert!(index < row, "index out of range for the row");
        Self { row, index }
    }

    /// The canonical slot index, 0..15: rows in order, in-row index ascending.
    #[must_use]
    pub const fn canonical_index(self) -> usize {
        let base = match self.row {
            1 => 0,
            2 => 1,
            3 => 3,
            4 => 6,
            5 => 10,
            _ => unreachable!(),
        };
        base + self.index as usize
    }

    /// The slot at a canonical index, 0..15.
    #[must_use]
    pub const fn from_canonical_index(index: usize) -> Self {
        let (row, base) = match index {
            0 => (1, 0),
            1..=2 => (2, 1),
            3..=5 => (3, 3),
            6..=9 => (4, 6),
            _ => (5, 10),
        };
        // `index - base` is at most 4 by the ranges above.
        Self {
            row,
            index: (index - base) as u8,
        }
    }

    /// The `r.i` spelling the corpora use (for example `5.4`).
    #[must_use]
    pub fn label(self) -> String {
        format!("{}.{}", self.row, self.index)
    }

    /// Parse the corpora's `r.i` spelling.
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        let (row, index) = label.split_once('.')?;
        let row: u8 = row.parse().ok()?;
        let index: u8 = index.parse().ok()?;
        if row == 0 || row > 5 || index >= row {
            return None;
        }
        Some(Self { row, index })
    }

    /// The slot's centre (mm), from the closed form of `rules-break.md` §2.4.
    #[must_use]
    pub fn position_mm(self) -> [f64; 2] {
        let x = FOOT_SPOT_X_MM + f64::from(self.row - 1) * RACK_ROW_STEP_MM;
        let offset = f64::from(self.index) - f64::from(self.row - 1) / 2.0;
        [x, offset * BALL_DIAMETER_MM]
    }

    /// The slots adjacent to this one in the exact-contact lattice: in-row neighbours and the two
    /// neighbours in each adjacent row. Each pair is exactly `2R` apart (`rules-break.md` §2.10).
    #[must_use]
    pub fn neighbours(self) -> Vec<Self> {
        let mut out = Vec::with_capacity(6);
        let (row, index) = (self.row, self.index);
        // In-row neighbours.
        if index > 0 {
            out.push(Self {
                row,
                index: index - 1,
            });
        }
        if index + 1 < row {
            out.push(Self {
                row,
                index: index + 1,
            });
        }
        // The next row down-table (row + 1): its slots `index` and `index + 1` both touch this one,
        // because row + 1 has one more slot than this row.
        if row < 5 {
            out.push(Self {
                row: row + 1,
                index,
            });
            out.push(Self {
                row: row + 1,
                index: index + 1,
            });
        }
        // The row above (row - 1): its slots `index - 1` and `index` touch this one.
        if row > 1 {
            if index > 0 {
                out.push(Self {
                    row: row - 1,
                    index: index - 1,
                });
            }
            if index < row - 1 {
                out.push(Self {
                    row: row - 1,
                    index,
                });
            }
        }
        out
    }
}

/// The centre of a slot (mm) — the free-function spelling of [`Slot::position_mm`].
#[must_use]
pub fn slot_position_mm(slot: Slot) -> [f64; 2] {
    slot.position_mm()
}
