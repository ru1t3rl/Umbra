//! Kernel-internal systems. They run in the phase that owns them, over entities in id order,
//! and only on `Owned` entities (REQ-K-5). No public system trait exists in M1 (ASM-029).

pub(crate) mod behavior;
pub(crate) mod motion;
pub(crate) mod perception;
pub(crate) mod route;
