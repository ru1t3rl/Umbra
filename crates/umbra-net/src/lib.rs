//! `umbra-net`: the protocol-neutral network layer of Umbra (ADR 0030).
//!
//! Responsibility (from M6): the network manager, publish policy, ghost lifecycle, identity map,
//! echo suppression and the adapter interface. It knows no protocol vocabulary; protocols live in
//! adapter crates, outside the engine side. Modules: `identity` and `typemap` (WI-5), `policy`
//! (WI-6), `ghosts`, `manager` and `loopback` (WI-7).
//!
//! Dependency rule (section 4): depends on `umbra-core` only among workspace crates, and on no
//! protocol crate (`cargo xtask check-deps`, `tests/protocol_neutral.rs`). It contains no `unsafe`
//! code.

#![forbid(unsafe_code)]

pub mod ghosts;
pub mod identity;
pub mod loopback;
pub mod manager;
pub mod policy;
pub mod replay;
pub mod tap;
pub mod typemap;

pub use ghosts::{GhostError, GhostSpec, GhostWorld};
pub use identity::{IdentityError, IdentityMap, Role};
pub use loopback::LoopbackHub;
pub use manager::{AdapterSlot, AdapterStatus, ManagerError, NetNotice, NetworkManager, OwnedMeta};
pub use policy::{AdapterSettings, GhostPolicy, PublishPolicy, PublishTracker};
pub use replay::ReplaySource;
pub use tap::InboundTap;
pub use typemap::{TypeMap, TypeMapWarning};

/// Name of this crate, as the registry and diagnostics spell it.
pub const NAME: &str = "umbra-net";
