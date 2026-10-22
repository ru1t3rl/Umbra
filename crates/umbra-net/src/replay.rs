//! The replay source: network adapters that return recorded inbound messages and open no sockets
//! (REQ-K-7, ADR 0054, ASM-628).
//!
//! Status: M7 WI-1 stub. Implemented in M7 WI-12.

use std::sync::Arc;

use umbra_core::Factory;
use umbra_core::net::{KeySpace, NetMessage, NetworkAdapterConfig};

/// Holds the recorded inbound messages and faults per tick and adapter.
#[derive(Debug)]
pub struct ReplaySource {
    _private: (),
}

impl ReplaySource {
    /// An empty source.
    ///
    /// # Panics
    /// Always, until M7 WI-12.
    #[expect(
        clippy::new_without_default,
        reason = "M7 WI-1 stub: WI-12 implements `new` and `Default`"
    )]
    #[must_use]
    pub fn new() -> Self {
        unimplemented!("M7 WI-12")
    }

    /// A factory for the adapter named `name` that replays from this source with the recorded
    /// `key_space`. Its adapters discard `publish`.
    #[must_use]
    pub fn factory(
        self: &Arc<Self>,
        name: &str,
        key_space: KeySpace,
    ) -> Arc<dyn Factory<dyn NetworkAdapterConfig>> {
        let _ = (name, key_space);
        unimplemented!("M7 WI-12")
    }

    /// Records that `adapter` delivers `messages` when polled for `tick`.
    pub fn push(&self, tick: u64, adapter: &str, messages: Vec<NetMessage>) {
        let _ = (tick, adapter, messages);
        unimplemented!("M7 WI-12")
    }

    /// Records that `adapter` faults during `call` at `tick`.
    pub fn fault_at(&self, tick: u64, adapter: &str, call: &str) {
        let _ = (tick, adapter, call);
        unimplemented!("M7 WI-12")
    }
}
