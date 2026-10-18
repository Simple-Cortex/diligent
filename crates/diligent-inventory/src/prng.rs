//! Seeded pseudo-random numbers for the random sample.
//!
//! Algorithm id [`PRNG_ID`]: `xorshift64*` (Vigna 2016) with the shifts 12, 25, 27 and
//! the multiplier `0x2545F4914F6CDD1D`. The 64-bit state is the first 8 bytes of
//! `sha256(seed_string)` read big-endian; a zero state is replaced by
//! `0x9E3779B97F4A7C15`. Bounded draws use rejection sampling, so they are unbiased.
//!
//! Reference vectors (state before the first call, then the first outputs):
//!
//! ```text
//! state 1                    -> 5180492295206395165, 12380297144915551517, 13389498078930870103
//! seed "diligent"            -> state 0xcf08fccd9d8bfd57
//!                               11815162611370927224, 3896702070645662550, 15025632453180051863
//! seed "run-2026-10-18-abcdef" -> state 0xe8f72f8f39049168
//!                               13982366010765060781, 2154164353909940071, 14987882293175992688
//! ```

use sha2::{Digest, Sha256};

/// Identifier recorded in `SAMPLE.json`.
pub const PRNG_ID: &str = "diligent-xorshift64star/1";

const MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;
const ZERO_FALLBACK: u64 = 0x9E37_79B9_7F4A_7C15;

/// The generator.
#[derive(Debug, Clone)]
pub struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    /// Starts from an explicit state (zero is replaced by a fixed constant).
    pub fn from_state(state: u64) -> Self {
        XorShift64Star {
            state: if state == 0 { ZERO_FALLBACK } else { state },
        }
    }

    /// Starts from a seed string.
    pub fn from_seed(seed: &str) -> Self {
        Self::from_state(seed_state(seed))
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(MULTIPLIER)
    }

    /// A uniform value in `0..n`. `n` must be non-zero.
    pub fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        if n <= 1 {
            return 0;
        }
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let x = self.next_u64();
            if x < zone {
                return x % n;
            }
        }
    }
}

/// The initial state derived from `seed`: first 8 bytes of its SHA-256, big-endian.
pub fn seed_state(seed: &str) -> u64 {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

/// Draws `k` distinct items from `items` uniformly (partial Fisher-Yates). The input order
/// must already be canonical; the result order is the draw order.
pub fn sample_without_replacement<T: Clone>(
    items: &[T],
    k: usize,
    rng: &mut XorShift64Star,
) -> Vec<T> {
    let mut pool: Vec<T> = items.to_vec();
    let k = k.min(pool.len());
    for i in 0..k {
        let remaining = (pool.len() - i) as u64;
        let j = i + usize::try_from(rng.below(remaining)).unwrap_or(0);
        pool.swap(i, j);
    }
    pool.truncate(k);
    pool
}
