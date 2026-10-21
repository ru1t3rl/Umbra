//! The world seam the manager mirrors peers' entities into (ADR 0030, REQ-N-4).
//!
//! Signature stub of M6 step 4; the ghost lifecycle itself lives in the manager (WI-7).

use std::sync::Arc;

use umbra_core::dead_reckoning::RemoteState;
use umbra_core::{EntityId, Force, RemoteSource, TemplateId};

/// Everything needed to create one ghost.
#[derive(Debug, Clone)]
pub struct GhostSpec<'a> {
    /// The adapter the ghost comes from.
    pub source: RemoteSource,
    /// The mapped template.
    pub template: &'a TemplateId,
    /// The peer's force, if sent.
    pub force: Option<Force>,
    /// The peer's display name, if sent.
    pub name: Option<Arc<str>>,
    /// The first dead-reckoning basis.
    pub remote: RemoteState,
}

/// A ghost operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GhostError {
    /// The world refused the operation.
    #[error("ghost world error: {0}")]
    World(String),
}

/// Where ghosts live. The engine implements it over `Kernel::spawn_remote`, `update_remote` and
/// `remove`; tests use a map.
pub trait GhostWorld {
    /// Creates a ghost.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError>;

    /// Replaces a ghost's basis.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn update_ghost(&mut self, id: EntityId, remote: RemoteState) -> Result<(), GhostError>;

    /// Removes a ghost.
    ///
    /// # Errors
    /// [`GhostError`] if the world refuses.
    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError>;
}
