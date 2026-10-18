//! Canonical, versioned state hash: FNV-1a 64 over a fixed byte encoding (ADR 0007, REQ-K-4).
//!
//! The hash detects accidental divergence between runs (determinism gate, replay); it is not a
//! security boundary. The encoding, not the hash function, carries the meaning:
//! - the stream starts with [`STATE_HASH_VERSION`] as a little-endian `u32`;
//! - integers are fixed-width little-endian;
//! - `f64` is written as its IEEE bits (little-endian) after canonicalising `-0.0` to `+0.0` and
//!   every NaN to `0x7FF8_0000_0000_0000`;
//! - byte runs and strings are prefixed with their length as a `u64`.
//!
//! [`StateHasher`] deliberately does not implement [`std::hash::Hasher`]: its provided `write_*`
//! methods use platform-width and non-canonical encodings.

use core::fmt;

use crate::geo::{Quaternion, Vec3};

/// Version of the canonical encoding. Any change to what or how the kernel hashes bumps it.
pub const STATE_HASH_VERSION: u32 = 2;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

/// A 64-bit state hash. Displays as 16 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateHash(pub u64);

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Incremental hasher over the canonical encoding (ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHasher {
    state: u64,
}

impl StateHasher {
    /// A hasher that has already consumed [`STATE_HASH_VERSION`].
    #[must_use]
    pub fn new() -> Self {
        let mut hasher = Self {
            state: FNV_OFFSET_BASIS,
        };
        hasher.write_u32(STATE_HASH_VERSION);
        hasher
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u64::from(byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    /// Writes one byte.
    pub fn write_u8(&mut self, v: u8) {
        self.write_raw(&[v]);
    }

    /// Writes a `u32`, little-endian.
    pub fn write_u32(&mut self, v: u32) {
        self.write_raw(&v.to_le_bytes());
    }

    /// Writes a `u64`, little-endian.
    pub fn write_u64(&mut self, v: u64) {
        self.write_raw(&v.to_le_bytes());
    }

    /// Writes an `f64` as little-endian IEEE bits; `-0.0` becomes `+0.0` and any NaN the
    /// canonical quiet NaN.
    pub fn write_f64(&mut self, v: f64) {
        let bits = if v.is_nan() {
            CANONICAL_NAN_BITS
        } else if v == 0.0 {
            0
        } else {
            v.to_bits()
        };
        self.write_u64(bits);
    }

    /// Writes a byte run prefixed with its length as a `u64`.
    pub fn write_bytes(&mut self, b: &[u8]) {
        // `usize` is at most 64 bits on every supported target; the cast cannot truncate.
        self.write_u64(b.len() as u64);
        self.write_raw(b);
    }

    /// Writes a string as the length-prefixed bytes of its UTF-8 encoding.
    pub fn write_str(&mut self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// Writes `x`, `y`, `z` as three `f64`.
    pub fn write_vec3(&mut self, v: Vec3) {
        self.write_f64(v.x);
        self.write_f64(v.y);
        self.write_f64(v.z);
    }

    /// Writes `w`, `x`, `y`, `z` as four `f64`.
    pub fn write_quaternion(&mut self, q: Quaternion) {
        self.write_f64(q.w);
        self.write_f64(q.x);
        self.write_f64(q.y);
        self.write_f64(q.z);
    }

    /// The hash of everything written so far; the hasher is unchanged.
    #[must_use]
    pub fn finish(&self) -> StateHash {
        StateHash(self.state)
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw FNV-1a 64 over `bytes` (no version prefix); for the published test vectors.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hasher = StateHasher {
        state: FNV_OFFSET_BASIS,
    };
    hasher.write_raw(bytes);
    hasher.state
}
