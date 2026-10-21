//! The in-process loopback adapter: endpoints of one hub deliver to each other (REQ-N-1, ADR 0030).
//!
//! Signature stub of M6 step 4; the bodies arrive with WI-7. Semantics (plan, "Loopback semantics"):
//! every endpoint created from one hub delivers each published message to every other endpoint's
//! bounded queue (drop and count when full); delivery is synchronous inside `publish`; the `params`
//! keys `key_space` (`[base, count]`) and `echo` (bool, also deliver to self) are read at build.
#![expect(
    clippy::todo,
    dead_code,
    clippy::needless_pass_by_value,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-7"
)]

use std::sync::Arc;

use umbra_core::net::{KeySpace, NetMessage, NetworkAdapterConfig};
use umbra_core::{ComponentId, Factory};

/// A set of loopback endpoints that hear each other.
#[derive(Debug)]
pub struct LoopbackHub {
    capacity: usize,
}

impl LoopbackHub {
    /// A hub whose endpoints queue at most `capacity` inbound messages each. It keeps no capture
    /// list (a long run would grow it without bound, ASM-279).
    #[must_use]
    pub fn new(capacity: usize) -> Arc<Self> {
        todo!("WI-7: LoopbackHub::new")
    }

    /// Like [`LoopbackHub::new`], and records every published message for [`LoopbackHub::captured`].
    #[must_use]
    pub fn with_capture(capacity: usize) -> Arc<Self> {
        todo!("WI-7: LoopbackHub::with_capture")
    }

    /// A factory registered under `id`; every `build` creates one endpoint of this hub, with
    /// `key_space` unless the params give one.
    #[must_use]
    pub fn factory(
        self: &Arc<Self>,
        id: ComponentId,
        key_space: KeySpace,
    ) -> Arc<dyn Factory<dyn NetworkAdapterConfig>> {
        todo!("WI-7: LoopbackHub::factory")
    }

    /// Every message published on the hub so far, with the key-space base of the publishing
    /// endpoint (its identity; factory ids are shared), in publish order. Empty unless the hub
    /// was made with [`LoopbackHub::with_capture`] (debugging aid, ASM-238, ASM-279).
    #[must_use]
    pub fn captured(&self) -> Vec<(u64, NetMessage)> {
        todo!("WI-7: LoopbackHub::captured")
    }

    /// How many endpoints have been started (the validate dry run keeps this at 0).
    #[must_use]
    pub fn starts(&self) -> usize {
        todo!("WI-7: LoopbackHub::starts")
    }
}
