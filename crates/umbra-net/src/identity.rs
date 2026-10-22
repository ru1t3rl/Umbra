//! The entity identity map: network keys to local entities and back (ADR 0031, REQ-N-4, REQ-N-5).

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
    /// How many keys were issued so far; never decreases, so a key is never reused.
    issued: u64,
    by_entity: BTreeMap<EntityId, (NetKey, Role)>,
    by_key: BTreeMap<NetKey, EntityId>,
}

impl IdentityMap {
    /// An empty map for an adapter that issues keys from `space` (ASM-274).
    #[must_use]
    pub fn new(space: KeySpace) -> Self {
        Self {
            space,
            issued: 0,
            by_entity: BTreeMap::new(),
            by_key: BTreeMap::new(),
        }
    }

    /// The key of owned entity `e`, issued as `base + 1, base + 2, ...` in call order on first use.
    /// A later call for the same entity returns the same key.
    ///
    /// # Errors
    /// [`IdentityError::KeysExhausted`] once `space.count` keys are issued,
    /// [`IdentityError::AlreadyBound`] if `e` is bound as a ghost.
    pub fn local_key(&mut self, e: EntityId) -> Result<NetKey, IdentityError> {
        if let Some(&(key, role)) = self.by_entity.get(&e) {
            return match role {
                Role::Owned => Ok(key),
                Role::Remote => Err(IdentityError::AlreadyBound),
            };
        }
        if self.issued >= self.space.count {
            return Err(IdentityError::KeysExhausted);
        }
        let key = self
            .space
            .base
            .checked_add(self.issued)
            .and_then(|k| k.checked_add(1))
            .map(NetKey)
            .ok_or(IdentityError::KeysExhausted)?;
        self.issued += 1;
        self.by_entity.insert(e, (key, Role::Owned));
        self.by_key.insert(key, e);
        Ok(key)
    }

    /// Binds a peer's key to ghost `e`. A key the stored own space contains is refused.
    ///
    /// # Errors
    /// [`IdentityError::OwnSpace`] for a key the own space could issue, [`IdentityError::AlreadyBound`]
    /// for a key or entity already mapped.
    pub fn bind_remote(&mut self, k: NetKey, e: EntityId) -> Result<(), IdentityError> {
        if self.space.contains(k) {
            return Err(IdentityError::OwnSpace(k));
        }
        if self.by_key.contains_key(&k) || self.by_entity.contains_key(&e) {
            return Err(IdentityError::AlreadyBound);
        }
        self.by_key.insert(k, e);
        self.by_entity.insert(e, (k, Role::Remote));
        Ok(())
    }

    /// Test hook: binds `k` to `e` as an Owned entity without the own-space check, to build the
    /// state ADR 0031 layer 3 guards against (ASM-277). Returns whether it was bound.
    #[cfg(test)]
    pub(crate) fn bind_owned_for_test(&mut self, k: NetKey, e: EntityId) -> bool {
        if self.by_key.contains_key(&k) || self.by_entity.contains_key(&e) {
            return false;
        }
        self.by_key.insert(k, e);
        self.by_entity.insert(e, (k, Role::Owned));
        true
    }

    /// The entity bound to `k` and its role.
    #[must_use]
    pub fn entity(&self, k: NetKey) -> Option<(EntityId, Role)> {
        let e = *self.by_key.get(&k)?;
        let (_, role) = self.by_entity.get(&e)?;
        Some((e, *role))
    }

    /// The key bound to `e`.
    #[must_use]
    pub fn key(&self, e: EntityId) -> Option<NetKey> {
        self.by_entity.get(&e).map(|&(k, _)| k)
    }

    /// Drops the binding of `e`; an owned entity's key is not reissued.
    pub fn forget(&mut self, e: EntityId) {
        if let Some((k, _)) = self.by_entity.remove(&e) {
            self.by_key.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_hook_binds_outside_the_own_space() {
        let e = EntityId::new(1).unwrap_or_else(|| unreachable!("non-zero"));
        let mut m = IdentityMap::new(KeySpace { base: 10, count: 2 });
        assert!(m.bind_owned_for_test(NetKey(99), e));
        assert_eq!(m.entity(NetKey(99)), Some((e, Role::Owned)));
        assert_eq!(m.key(e), Some(NetKey(99)));
        assert!(!m.bind_owned_for_test(NetKey(99), e));
        m.forget(e);
        assert_eq!(m.entity(NetKey(99)), None);
    }
}
