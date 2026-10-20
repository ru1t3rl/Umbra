//! `umbra-net`: the protocol-neutral network layer of Umbra (ADR 0030).
//!
//! Responsibility (from M6): the network manager, publish policy, ghost lifecycle, identity map,
//! echo suppression and the adapter interface. It knows no protocol vocabulary; protocols live in
//! adapter crates, outside the engine side. This scaffold (M6 WI-1) holds no logic yet.
//!
//! Dependency rule (section 4): depends on `umbra-core` only among workspace crates, and on no
//! protocol crate (`cargo xtask check-deps`, `tests/protocol_neutral.rs`). It contains no `unsafe`
//! code.

#![forbid(unsafe_code)]

/// Name of this crate, as the registry and diagnostics spell it.
pub const NAME: &str = "umbra-net";
