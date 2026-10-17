//! STUB (WI-5): canonical state hash, FNV-1a 64 (ADR 0007, REQ-K-4).
//!
//! Signatures only. Acceptance tests: `tests/hash.rs` (AT-6).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-5: signatures only, documented when implemented"
)]

use core::fmt;

use crate::geo::{Quaternion, Vec3};

pub const STATE_HASH_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateHash(pub u64);

impl fmt::Display for StateHash {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("WI-5")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHasher {
    _private: (),
}

impl StateHasher {
    pub fn new() -> Self {
        todo!("WI-5")
    }
    pub fn write_u8(&mut self, _v: u8) {
        todo!("WI-5")
    }
    pub fn write_u32(&mut self, _v: u32) {
        todo!("WI-5")
    }
    pub fn write_u64(&mut self, _v: u64) {
        todo!("WI-5")
    }
    pub fn write_f64(&mut self, _v: f64) {
        todo!("WI-5")
    }
    pub fn write_bytes(&mut self, _b: &[u8]) {
        todo!("WI-5")
    }
    pub fn write_str(&mut self, _s: &str) {
        todo!("WI-5")
    }
    pub fn write_vec3(&mut self, _v: Vec3) {
        todo!("WI-5")
    }
    pub fn write_quaternion(&mut self, _q: Quaternion) {
        todo!("WI-5")
    }
    pub fn finish(&self) -> StateHash {
        todo!("WI-5")
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw FNV-1a 64 over bytes.
pub fn fnv1a64(_bytes: &[u8]) -> u64 {
    todo!("WI-5")
}
