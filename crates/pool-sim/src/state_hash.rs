//! `state_hash`: FNV-1a 64 over the canonical byte stream of a position (`architecture.md` §11).
//!
//! Stable, dependency-free, and compares bits — never decimal text. The stream is, per ball in id
//! order (cue, 1–15): `f64::to_bits()` of position, velocity, and angular velocity, then the motion
//! mode and the pocketed/off-table flags.

use crate::ball::BallState;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hash one position: 16 ball states, cue first.
#[must_use]
pub fn state_hash(states: &[BallState; 16]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for ball in states {
        for value in ball
            .pos_mm
            .iter()
            .chain(ball.vel_mm_s.iter())
            .chain(ball.spin_rad_s.iter())
        {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        let mode = match ball.mode {
            crate::ball::MotionMode::Airborne => 0_u8,
            crate::ball::MotionMode::Sliding => 1,
            crate::ball::MotionMode::Rolling => 2,
            crate::ball::MotionMode::Spinning => 3,
            crate::ball::MotionMode::Stationary => 4,
        };
        for byte in [mode, u8::from(ball.pocketed), u8::from(ball.off_table)] {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// The hash rendered as the 16 lowercase hex digits the goldens carry.
#[must_use]
pub fn state_hash_hex(states: &[BallState; 16]) -> String {
    format!("{:016x}", state_hash(states))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rack;

    #[test]
    fn hash_is_stable_and_position_sensitive() {
        let arrangement = rack::generate(0);
        let states = arrangement.rest_states();
        let first = state_hash(&states);
        assert_eq!(first, state_hash(&states));

        let mut moved = states;
        moved[3].pos_mm[0] += 1e-9;
        assert_ne!(
            first,
            state_hash(&moved),
            "a 1e-9 mm move must change the hash"
        );
    }
}
