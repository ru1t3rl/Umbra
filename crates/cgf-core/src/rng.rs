//! STUB (WI-6): `SimRng`, derived streams, `Xoshiro256**` and `SplitMix64` (ADR 0006, REQ-K-4).
//!
//! Signatures only. Acceptance tests: `tests/rng.rs` (AT-5).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-6: signatures only, documented when implemented"
)]

use core::num::NonZeroU64;

use crate::id::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimRng {
    seed: u64,
}

impl SimRng {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }
    pub const fn seed(self) -> u64 {
        self.seed
    }
    pub fn stream(self, _key: StreamKey) -> RngStream {
        todo!("WI-6")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamKey {
    pub domain: u64,
    pub id: u64,
}

impl StreamKey {
    pub const ENTITY: u64 = 1;
    pub const SCENARIO: u64 = 2;
    pub fn entity(_id: EntityId) -> Self {
        todo!("WI-6")
    }
    pub fn scenario(_index: u64) -> Self {
        todo!("WI-6")
    }
}

/// Xoshiro256** state. Not `Copy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RngStream {
    s: [u64; 4],
}

impl RngStream {
    pub fn next_u64(&mut self) -> u64 {
        todo!("WI-6")
    }
    pub fn next_f64(&mut self) -> f64 {
        todo!("WI-6")
    }
    pub fn gen_range_u64(&mut self, _n: NonZeroU64) -> u64 {
        todo!("WI-6")
    }
    pub fn uniform_f64(&mut self, _lo: f64, _hi: f64) -> f64 {
        todo!("WI-6")
    }
    pub fn state(&self) -> [u64; 4] {
        self.s
    }
    /// Test-driven addition (not in the plan): build a stream from a raw state; `None` if all zero.
    pub fn from_state(_s: [u64; 4]) -> Option<Self> {
        todo!("WI-6")
    }
}

/// `SplitMix64` (seeding only; public for the reference-vector test).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u64(&mut self) -> u64 {
        let _ = self.state;
        todo!("WI-6")
    }
}
