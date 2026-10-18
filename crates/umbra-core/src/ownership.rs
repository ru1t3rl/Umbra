//! Owned versus remote entities (REQ-K-5, ASM-041).
//!
//! Behavior and motion systems run only for [`Ownership::Owned`] entities; remote entities are
//! updated from the network (M6).

use std::sync::Arc;

/// Who simulates an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    /// Simulated by this engine instance.
    Owned,
    /// Simulated elsewhere and mirrored here.
    Remote {
        /// Where the entity comes from.
        source: RemoteSource,
    },
}

/// Opaque label of a remote entity's origin (an adapter or exercise); its structure is decided
/// with the network adapters in M6 (ASM-041).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RemoteSource(Arc<str>);

impl RemoteSource {
    /// A source label.
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self(Arc::from(s))
    }

    /// The label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Ownership {
    /// True for [`Ownership::Owned`] only.
    #[must_use]
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned)
    }
}
