//! `umbra-net-dis`: the DIS network adapter plugin (REQ-N-6, ADR 0034).
//!
//! Built with `umbra-sdk` only, like any third-party plugin (REQ-SDK-3), and `#![forbid(unsafe_code)]`
//! (ASM-116). Layers (ADR 0034, mapping in `docs/net/dis.md`):
//!
//! - [`codec`]: pure, panic-free big-endian encoding and decoding of PDUs (M6 WI-13: header and
//!   Entity State; WI-14: the other families);
//! - [`version`]: [`DisVersion`](version::DisVersion) and the header strategy, the only place the
//!   v6/v7 differences live;
//! - `translate`, `config` and `transport` follow in WI-15.
//!
//! The `export_plugin!` below is a stub until WI-15: it declares the plugin and no capability.

#![forbid(unsafe_code)]

pub mod codec;
pub mod version;

umbra_sdk::export_plugin! {
    id: "umbra.plugin.dis",
    name: "Umbra DIS adapter",
    version: "0.1.0",
}
