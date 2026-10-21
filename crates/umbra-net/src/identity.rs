//! The entity identity map: network keys to local entities and back (ADR 0031, REQ-N-4, REQ-N-5).
//!
//! Signature stub of M6 step 4; the bodies arrive with WI-5.
#![expect(
    clippy::todo,
    dead_code,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-5"
)]

use std::collections::BTreeMap;

use umbra_core::EntityId;
use umbra_core::net::{KeySpace, NetKey};

/// Who simulates the entity a key is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// This engine simulates it; the key was issued from the adapter's own key space.
    Owned,
    /// A peer simulates it; this engine mirrors it as a ghost.
    Remote,
}

/// Why a key could not be issued or bound.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// Every key of the own space is in use (keys are never reused).
    #[error("the key space is exhausted")]
    KeysExhausted,
    /// The key to bind lies in the own key space (an echo).
    #[error("key {0:?} lies in the own key space")]
    OwnSpace(NetKey),
    /// The key, or the entity, is already bound.
    #[error("already bound")]
    AlreadyBound,
}

/// A bijection between network keys and local entities, per adapter.
#[derive(Debug)]
pub struct IdentityMap {
    space: KeySpace,
    by_entity: BTreeMap<EntityId, (NetKey, Role)>,
    by_key: BTreeMap<NetKey, EntityId>,
}

impl IdentityMap {
    /// An empty map for an adapter that issues keys from `space` (ASM-274).
    #[must_use]
    pub fn new(space: KeySpace) -> Self {
        Self {
            space,
            by_entity: BTreeMap::new(),
            by_key: BTreeMap::new(),
        }
    }

    /// The key of owned entity `e`, issued as `base + 1, base + 2, ...` in call order on first use.
    ///
    /// # Errors
    /// [`IdentityError::KeysExhausted`] once `space.count` keys are issued.
    pub fn local_key(&mut self, e: EntityId) -> Result<NetKey, IdentityError> {
        todo!("WI-5: local_key")
    }

    /// Binds a peer's key to ghost `e`. A key the stored own space contains is refused.
    ///
    /// # Errors
    /// [`IdentityError::OwnSpace`] for a key the own space could issue, [`IdentityError::AlreadyBound`]
    /// for a key or entity already mapped.
    pub fn bind_remote(&mut self, k: NetKey, e: EntityId) -> Result<(), IdentityError> {
        todo!("WI-5: bind_remote")
    }

    /// The entity bound to `k` and its role.
    #[must_use]
    pub fn entity(&self, k: NetKey) -> Option<(EntityId, Role)> {
        todo!("WI-5: entity")
    }

    /// The key bound to `e`.
    #[must_use]
    pub fn key(&self, e: EntityId) -> Option<NetKey> {
        todo!("WI-5: key")
    }

    /// Drops the binding of `e`; an owned entity's key is not reissued.
    pub fn forget(&mut self, e: EntityId) {
        todo!("WI-5: forget")
    }
}
