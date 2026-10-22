//! The sensor kind (ABI 0.2, ADR 0050, REQ-P-4, REQ-B-5).
//!
//! A [`Sensor`] runs once per observer and tick against all other entities as they were before the
//! tick (id order), like a built-in `umbra_core::Sensor`. Randomness only from the observer's own
//! stream ([`SenseCtx::rng`], REQ-K-4); terrain through [`SenseCtx::terrain`].
//!
//! Status: M7 WI-2 stub. The signatures follow `docs/plans/M7.md` (section "SDK"); the behavior is
//! implemented in M7 WI-10.

use std::marker::PhantomData;

use crate::behavior::{Contact, Rng};
use crate::error::Error;
use crate::motion::KinematicState;
use crate::services::Terrain;

/// Who simulates an entity (`umbra_core::Ownership` reduced to the two cases a plugin can see).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// Simulated by this engine.
    Owned,
    /// A ghost simulated by a peer.
    Remote,
}

/// The observer or a candidate of one sense call, pre-tick.
#[derive(Debug, Clone, Copy)]
pub struct TargetView<'a> {
    /// Entity id (non-zero).
    pub id: u64,
    /// Pre-tick kinematic state.
    pub state: KinematicState,
    /// Force, if any.
    pub force: Option<&'a str>,
    /// Who simulates it.
    pub ownership: Ownership,
}

/// Time, randomness and terrain of one sense call.
#[derive(Debug)]
pub struct SenseCtx<'a> {
    tick: u64,
    rate_hz: u32,
    call: PhantomData<&'a ()>,
}

impl SenseCtx<'_> {
    /// Simulation tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        let _ = (self.tick, self.rate_hz, &self.call);
        unimplemented!("M7 WI-10")
    }

    /// Tick rate in Hz.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        unimplemented!("M7 WI-10")
    }

    /// The observer's own RNG stream through the host.
    pub fn rng(&mut self) -> Rng<'_> {
        unimplemented!("M7 WI-10")
    }

    /// The terrain of this call.
    #[must_use]
    pub fn terrain(&self) -> Terrain<'_> {
        unimplemented!("M7 WI-10")
    }
}

/// The observer's contacts of one sense call. A contact must name one of the candidates (the host
/// refuses and faults otherwise); the kernel sorts and deduplicates by id as it does for built-ins.
#[derive(Debug)]
pub struct Contacts<'a> {
    call: PhantomData<&'a ()>,
}

impl Contacts<'_> {
    /// Appends a contact (copied into the host at once).
    ///
    /// # Errors
    /// The host refused the contact (it names no candidate, or the host's cap per call is reached).
    pub fn push(&mut self, contact: &Contact<'_>) -> Result<(), Error> {
        let _ = (&self.call, contact);
        unimplemented!("M7 WI-10")
    }
}

/// A sensor plugin capability (listed under `sensors:` of `export_plugin!`).
pub trait Sensor: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset).
    const SCHEMA: &'static str;

    /// Builds the sensor from the JSON params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &str) -> Result<Self, Error>;

    /// Appends contacts among `candidates` (all other entities, id order, pre-tick state).
    ///
    /// # Errors
    /// The sensor failed (or a service call was refused); the host faults the capability.
    fn sense(
        &self,
        ctx: &mut SenseCtx<'_>,
        observer: &TargetView<'_>,
        candidates: &[TargetView<'_>],
        out: &mut Contacts<'_>,
    ) -> Result<(), Error>;
}
