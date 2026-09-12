//! The seeded randomness kit (`architecture.md` §5).
//!
//! One implementation, no `rand`: `SplitMix64` as the stream, [`SplitMix64::next_below`] as Lemire
//! multiply-shift with rejection, [`shuffle`] as Fisher–Yates descending, and [`gaussian_pair`] as a
//! Marsaglia-polar Gaussian whose single transcendental is the pinned `libm::ln` (`architecture.md` §3).
//!
//! The simulator itself consumes none of this (`physics.md` §1): rack construction and execution noise
//! are the consumers, and both take a seed as an input.

/// `SplitMix64`: the stream `architecture.md` §5 pins, with `rules-break.md` §2.6's published constants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

/// The `SplitMix64` increment (`rules-break.md` §2.6 step 1).
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX_A: u64 = 0xBF58_476D_1CE4_E5B9;
const MIX_B: u64 = 0x94D0_49BB_1331_11EB;

impl SplitMix64 {
    /// A stream seeded with `seed`; the first output is the seed advanced by γ once.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit output (`rules-break.md` §2.6 step 1).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(MIX_A);
        z = (z ^ (z >> 27)).wrapping_mul(MIX_B);
        z ^ (z >> 31)
    }

    /// An unbiased value in `0..n` (`rules-break.md` §2.6 step 2): Lemire multiply-shift with rejection.
    ///
    /// # Panics
    ///
    /// Panics if `n == 0`.
    // The 128-bit product's two halves are read as `u64` by definition of the algorithm: the low half
    // is the rejection test's input and the high half is the result.
    #[allow(clippy::cast_possible_truncation)]
    pub fn next_below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "next_below(0) has no value to return");
        let mut x = self.next_u64();
        let mut m = u128::from(x) * u128::from(n);
        let mut l = m as u64;
        if l < n {
            let t = n.wrapping_neg() % n; // (2^64 - n) mod n
            while l < t {
                x = self.next_u64();
                m = u128::from(x) * u128::from(n);
                l = m as u64;
            }
        }
        (m >> 64) as u64
    }

    /// Fisher–Yates descending over `items` (`rules-break.md` §2.6 step 5).
    // A draw below `i + 1` is at most a slice length, so the conversion back to `usize` is exact.
    #[allow(clippy::cast_possible_truncation)]
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        let len = items.len();
        for i in (1..len).rev() {
            let j = self.next_below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }

    /// One Marsaglia-polar Gaussian pair (`architecture.md` §5): `sqrt` plus the pinned `libm::ln`.
    pub fn gaussian_pair(&mut self) -> (f64, f64) {
        loop {
            // Two uniforms in [-1, 1) from 53-bit mantissas.
            let u = unit_signed(self.next_u64());
            let v = unit_signed(self.next_u64());
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                let scale = (-2.0 * libm::log(s) / s).sqrt();
                return (u * scale, v * scale);
            }
        }
    }
}

/// A value in `[-1, 1)` from the top 53 bits of `x`.
fn unit_signed(x: u64) -> f64 {
    let mantissa = x >> 11; // 53 bits
    let unit = (mantissa as f64) * (1.0 / 9_007_199_254_740_992.0); // 2^-53
    unit * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_matches_the_published_sequence() {
        // The reference sequence for seed 0 (SplitMix64's published test vectors).
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn next_below_stays_in_range_and_is_stable() {
        let mut rng = SplitMix64::new(42);
        let draws: Vec<u64> = (0..64).map(|_| rng.next_below(7)).collect();
        assert!(draws.iter().all(|&d| d < 7));
        let mut again = SplitMix64::new(42);
        let repeat: Vec<u64> = (0..64).map(|_| again.next_below(7)).collect();
        assert_eq!(draws, repeat);
    }

    #[test]
    fn shuffle_is_a_permutation_and_seed_stable() {
        let mut rng = SplitMix64::new(7);
        let mut items: Vec<u32> = (0..12).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..12).collect::<Vec<u32>>());

        let mut again = SplitMix64::new(7);
        let mut repeat: Vec<u32> = (0..12).collect();
        again.shuffle(&mut repeat);
        assert_eq!(items, repeat);
    }

    #[test]
    fn gaussian_pair_is_standard_normal_ish() {
        let mut rng = SplitMix64::new(1);
        let n = 20_000;
        let (mut sum, mut sum_sq) = (0.0, 0.0);
        for _ in 0..n {
            let (a, b) = rng.gaussian_pair();
            sum += a + b;
            sum_sq += a * a + b * b;
        }
        let count = 2.0 * f64::from(n);
        let mean = sum / count;
        let var = sum_sq / count - mean * mean;
        assert!(mean.abs() < 0.03, "mean {mean}");
        assert!((var - 1.0).abs() < 0.05, "var {var}");
    }
}
