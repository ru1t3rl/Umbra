//! The inbound tap: a read-only observer of what each adapter delivered, for the recorder
//! (REQ-K-7, ADR 0053, ADR 0054, ASM-627).
//!
//! The manager reports each enabled adapter's poll result in scenario order, exactly as polled and
//! before it processes the messages, and every adapter fault with the call that failed. The tap
//! cannot change what the manager does.

use umbra_core::net::NetMessage;

/// Receives the inbound side of the network layer, in tick order.
pub trait InboundTap: Send {
    /// An adapter's poll for `tick` (the tick about to run) returned `messages` (possibly none).
    fn polled(&mut self, tick: u64, adapter: &str, messages: &[NetMessage]);

    /// An adapter faulted during `call` (`poll`, `publish`, or `control` for run-control publishes) at `tick`.
    fn faulted(&mut self, tick: u64, adapter: &str, call: &str);
}
