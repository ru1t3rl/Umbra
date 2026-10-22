//! `umbra-net-dis`: the DIS network adapter plugin (REQ-N-6, ADR 0034).
//!
//! Built with `umbra-sdk` only, like any third-party plugin (REQ-SDK-3), and `#![forbid(unsafe_code)]`
//! (ASM-116). Layers (ADR 0034, mapping in `docs/net/dis.md`):
//!
//! - [`codec`]: pure, panic-free big-endian encoding and decoding of PDUs;
//! - [`version`]: [`DisVersion`](version::DisVersion) and the header strategy, the only place the
//!   v6/v7 differences live;
//! - [`translate`]: the SDK message mirror to and from PDUs (key packing, entity type tokens, force,
//!   weapon and dead reckoning tables, marking, relative timestamps);
//! - [`config`]: the params schema and their validation;
//! - [`transport`]: the UDP socket, two I/O threads and two bounded queues;
//! - [`adapter`]: the SDK traits over the layers above.
//!
//! The plugin exports one capability, `umbra.net.dis`. Nothing in it is global: any number of
//! instances (several scenario names, several images) run side by side (ASM-410).

#![forbid(unsafe_code)]

pub mod adapter;
pub mod codec;
pub mod config;
pub mod translate;
pub mod transport;
pub mod version;

pub use adapter::{DisAdapter, DisConfig, DisFactory};

umbra_sdk::export_plugin! {
    id: "umbra.plugin.dis",
    name: "Umbra DIS adapter",
    version: "0.1.0",
    network_adapters: [DisFactory],
}
