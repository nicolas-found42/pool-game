//! The frozen rack generator (`rules-break.md` §2.6): a pure `seed → arrangement` map, asserted never
//! repaired.

use pool_rng::SplitMix64;

use crate::ball::BallState;
use crate::constants::BALL_DIAMETER_MM;
use crate::frame::Slot;

/// The slots the generator assigns before the free shuffle, as canonical indices.
const SLOT_5_0: usize = 10;
const SLOT_5_4: usize = 14;
const SLOT_3_1: usize = 4;

/// The 12 free slots, in canonical slot order (`rules-break.md` §2.6 step 6).
const DEAL_ORDER: [usize; 12] = [0, 1, 2, 3, 5, 6, 7, 8, 9, 11, 12, 13];

/// The group-A balls (1–7) and group-B balls (9–15), each in ascending ball-number order.
const GROUP_A: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];
const GROUP_B: [u8; 7] = [9, 10, 11, 12, 13, 14, 15];

/// A frozen arrangement: ball numbers by canonical slot index (0..15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrangement {
    /// Ball number per canonical slot index; always 1..=15 (there is no empty slot).
    pub slots: [u8; 15],
}

impl Arrangement {
    /// The ball in a slot.
    #[must_use]
    pub const fn ball_at(&self, slot: Slot) -> u8 {
        self.slots[slot.canonical_index()]
    }

    /// The slot holding a ball number.
    ///
    /// # Panics
    ///
    /// Panics if no slot holds `ball` — an arrangement always holds balls 1..=15, so a miss means the
    /// caller asked about a ball outside the rack.
    #[must_use]
    pub fn slot_of(&self, ball: u8) -> Slot {
        let index = self
            .slots
            .iter()
            .position(|&b| b == ball)
            .unwrap_or_else(|| panic!("ball {ball} is not in this arrangement"));
        Slot::from_canonical_index(index)
    }

    /// The arrangement as `(slot label, ball)` pairs in canonical slot order.
    #[must_use]
    pub fn labelled(&self) -> Vec<(String, u8)> {
        (0..15)
            .map(|i| (Slot::from_canonical_index(i).label(), self.slots[i]))
            .collect()
    }

    /// The 16 ball states at rest in their rack positions: cue ball parked above the head string,
    /// object balls in their slots.
    #[must_use]
    pub fn rest_states(&self) -> [BallState; 16] {
        let mut states = [BallState::at_rest([0.0; 3]); 16];
        states[0] = BallState::at_rest([-800.0, 0.0, crate::constants::BALL_RADIUS_MM]);
        for index in 0..15 {
            let slot = Slot::from_canonical_index(index);
            let [x, y] = slot.position_mm();
            let ball = usize::from(self.slots[index]);
            states[ball] = BallState::at_rest([x, y, crate::constants::BALL_RADIUS_MM]);
        }
        states
    }
}

/// The frozen rack function: `seed → arrangement` (`rules-break.md` §2.6).
///
/// Draws, in order: one `next_below(2)` for the corner sides, two `next_below(7)` for the corner balls
/// (slot (5,0) then (5,4)), then the 12-ball Fisher–Yates descending shuffle.
#[must_use]
pub fn generate(seed: u64) -> Arrangement {
    let mut rng = SplitMix64::new(seed);
    let mut slots = [0_u8; 15];

    // Step 3: corner sides. 0 ⇒ group A takes (5,0); 1 ⇒ the reverse.
    let side = rng.next_below(2);
    let (left_group, right_group) = if side == 0 {
        (GROUP_A, GROUP_B)
    } else {
        (GROUP_B, GROUP_A)
    };

    // Step 4: corner balls, in slot order (5,0) then (5,4), each over its group in ascending order.
    let left = left_group[rng.next_below(7) as usize];
    slots[SLOT_5_0] = left;
    let right = right_group[rng.next_below(7) as usize];
    slots[SLOT_5_4] = right;

    // Step 5: the free shuffle — the 12 balls that are neither the 8 nor a corner ball, ascending.
    let mut free: Vec<u8> = (1..=15)
        .filter(|&b| b != 8 && b != left && b != right)
        .collect();
    rng.shuffle(&mut free);

    // Step 6: the 8 takes (3,1); the shuffled 12 are dealt in canonical slot order.
    slots[SLOT_3_1] = 8;
    for (slot, ball) in DEAL_ORDER.into_iter().zip(free) {
        slots[slot] = ball;
    }

    let arrangement = Arrangement { slots };
    debug_assert!(
        check_invariants(&arrangement).is_ok(),
        "generator produced an illegal rack"
    );
    arrangement
}

/// The rack invariants of `rules-break.md` §2.10, checked on an arrangement. A violation is a bug.
///
/// # Errors
///
/// Returns the violated invariant's description: the apex slot empty, ball 8 outside (3,1), the corner
/// slots not holding one ball per group, a slot empty or holding a ball number outside 1..=15, a ball
/// appearing twice, or a neighbour pair not exactly `2R` apart.
pub fn check_invariants(arrangement: &Arrangement) -> Result<(), String> {
    let slots = &arrangement.slots;

    // 1. the apex ball is in slot (1,0)
    if slots[0] == 0 {
        return Err("slot (1,0) is empty".to_string());
    }
    // 2. ball 8 is in slot (3,1)
    if slots[SLOT_3_1] != 8 {
        return Err(format!(
            "ball 8 is in slot {}, not (3,1)",
            arrangement.slot_of(8).label()
        ));
    }
    // 3. slots (5,0) and (5,4) hold one ball from each group
    let corner_a = slots[SLOT_5_0];
    let corner_b = slots[SLOT_5_4];
    let in_a = |b: u8| (1..=7).contains(&b);
    if !(in_a(corner_a) ^ in_a(corner_b)) {
        return Err(format!(
            "corner slots hold {corner_a} and {corner_b}, not one per group"
        ));
    }
    // 4. the 15 slots hold 15 distinct ball numbers
    let mut seen = [false; 16];
    for &ball in slots {
        if ball == 0 || ball > 15 {
            return Err(format!("slot holds invalid ball number {ball}"));
        }
        if seen[usize::from(ball)] {
            return Err(format!("ball {ball} appears twice"));
        }
        seen[usize::from(ball)] = true;
    }
    // 5. every adjacent pair of slots is exactly 2R apart
    let mut worst = 0.0_f64;
    for index in 0..15 {
        let slot = Slot::from_canonical_index(index);
        let [x0, y0] = slot.position_mm();
        for neighbour in slot.neighbours() {
            let [x1, y1] = neighbour.position_mm();
            let d = ((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0)).sqrt();
            worst = worst.max((d - BALL_DIAMETER_MM).abs());
        }
    }
    if worst > 1e-9 {
        return Err(format!("lattice spacing is off by {worst} mm"));
    }
    Ok(())
}
