//! The sensor kind (ABI 0.2, ADR 0050, REQ-P-4, REQ-B-5).
//!
//! A [`Sensor`] runs once per observer and tick against all other entities as they were before the
//! tick (id order), like a built-in `umbra_core::Sensor`. Randomness only from the observer's own
//! stream ([`SenseCtx::rng`], REQ-K-4); terrain through [`SenseCtx::terrain`].

use crate::abi::{UMBRA_OK, UmbraToken};
use crate::behavior::{Contact, Fault, Rng};
use crate::convert::{ContactSink, TerrainSvc};
use crate::error::Error;
use crate::host::HostCopy;
use crate::motion::KinematicState;
use crate::services::{Terrain, refused};

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
    host: Option<HostCopy>,
    rng: UmbraToken,
    terrain: TerrainSvc,
    fault: &'a Fault,
}

impl<'a> SenseCtx<'a> {
    /// The context of one `sense` call.
    pub(crate) fn new(
        tick: u64,
        rate_hz: u32,
        host: Option<HostCopy>,
        rng: UmbraToken,
        terrain: TerrainSvc,
        fault: &'a Fault,
    ) -> Self {
        Self {
            tick,
            rate_hz,
            host,
            rng,
            terrain,
            fault,
        }
    }
}

impl SenseCtx<'_> {
    /// Simulation tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Tick rate in Hz.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The observer's own RNG stream through the host.
    pub fn rng(&mut self) -> Rng<'_> {
        Rng::new(self.host, self.rng, self.fault)
    }

    /// The terrain of this call.
    #[must_use]
    pub fn terrain(&self) -> Terrain<'_> {
        Terrain::new(self.terrain, self.fault)
    }
}

/// The observer's contacts of one sense call. A contact must name one of the candidates (the host
/// refuses and faults otherwise); the kernel sorts and deduplicates by id as it does for built-ins.
#[derive(Debug)]
pub struct Contacts<'a> {
    sink: ContactSink,
    fault: &'a Fault,
}

impl<'a> Contacts<'a> {
    /// The contact output of one `sense` call.
    pub(crate) fn new(sink: ContactSink, fault: &'a Fault) -> Self {
        Self { sink, fault }
    }
}

impl Contacts<'_> {
    /// Appends a contact (copied into the host at once).
    ///
    /// # Errors
    /// The host refused the contact (it names no candidate, or the host's cap per call is reached);
    /// the capability is then faulted and this `sense` call returns the host's code whether or not the
    /// sensor looks at the error.
    pub fn push(&mut self, contact: &Contact<'_>) -> Result<(), Error> {
        let code = self.sink.push(contact);
        if code == UMBRA_OK {
            return Ok(());
        }
        Err(refused(
            self.fault,
            code,
            &format!("contact push of entity {} refused", contact.entity),
        ))
    }
}

/// A sensor plugin capability (listed under `sensors:` of `export_plugin!`).
pub trait Sensor: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset).
    const SCHEMA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;

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
