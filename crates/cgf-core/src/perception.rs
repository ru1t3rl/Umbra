//! Perception types: forces, contacts and the [`Sensor`] trait (REQ-B-5, ADR 0015, ASM-079).
//!
//! The kernel runs sensors for `Owned` entities in `EntityId` order against all other entities as
//! they were before the tick, so contacts do not depend on entity order.

use std::sync::Arc;

use crate::coords::Ecef;
use crate::geo::Vec3;
use crate::id::{ComponentId, EntityId};
use crate::motion::KinematicState;
use crate::ownership::Ownership;
use crate::terrain::TerrainProvider;
use crate::units::Meters;

/// The side an entity belongs to; grammar `[a-z][a-z0-9_]*` (ASM-079).
///
/// Ordering is byte-lexicographic, so it is a deterministic map key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Force(Arc<str>);

/// Why a string was rejected as a [`Force`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ForceError {
    /// The input was empty.
    #[error("force name is empty")]
    Empty,
    /// The first character is not `a-z`.
    #[error("force name {name:?} must start with a lowercase letter")]
    BadStart {
        /// The offending input.
        name: String,
    },
    /// A later character is not `a-z`, `0-9` or `_`.
    #[error("force name {name:?} has an invalid character at byte {index}")]
    BadChar {
        /// The offending input.
        name: String,
        /// Byte offset of the first invalid character.
        index: usize,
    },
}

impl Force {
    /// Validates `s` and creates the force.
    ///
    /// # Errors
    /// Returns the [`ForceError`] describing the first grammar violation.
    pub fn new(s: &str) -> Result<Self, ForceError> {
        let mut bytes = s.bytes();
        match bytes.next() {
            None => return Err(ForceError::Empty),
            Some(b'a'..=b'z') => {}
            Some(_) => return Err(ForceError::BadStart { name: s.to_owned() }),
        }
        let bad = s
            .bytes()
            .enumerate()
            .skip(1)
            .find(|(_, b)| !matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'));
        match bad {
            Some((index, _)) => Err(ForceError::BadChar {
                name: s.to_owned(),
                index,
            }),
            None => Ok(Self(Arc::from(s))),
        }
    }

    /// The force name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether two entities are hostile: both have a force and the forces differ (ASM-079).
#[must_use]
pub fn hostile(a: Option<&Force>, b: Option<&Force>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

/// A detected entity, as seen from one observer (perfect detection in v1, ADR 0015).
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    /// The detected entity.
    pub entity: EntityId,
    /// Its position, ECEF meters.
    pub position: Ecef,
    /// Its velocity, ECEF meters per second.
    pub velocity: Vec3,
    /// Its force, if any.
    pub force: Option<Force>,
    /// Distance from the observer.
    pub range: Meters,
}

/// The entity that senses.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Observer<'a> {
    /// Its ID.
    pub id: EntityId,
    /// Its pre-tick kinematic state.
    pub state: &'a KinematicState,
    /// Its force, if any.
    pub force: Option<&'a Force>,
}

impl<'a> Observer<'a> {
    /// Creates an observer.
    pub fn new(id: EntityId, state: &'a KinematicState, force: Option<&'a Force>) -> Self {
        Self { id, state, force }
    }
}

/// A candidate for detection.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Target<'a> {
    /// Its ID.
    pub id: EntityId,
    /// Its pre-tick kinematic state.
    pub state: &'a KinematicState,
    /// Its force, if any.
    pub force: Option<&'a Force>,
    /// Who simulates it (Owned and Remote entities are both perceived, REQ-K-5).
    pub ownership: &'a Ownership,
}

impl<'a> Target<'a> {
    /// Creates a target.
    pub fn new(
        id: EntityId,
        state: &'a KinematicState,
        force: Option<&'a Force>,
        ownership: &'a Ownership,
    ) -> Self {
        Self {
            id,
            state,
            force,
            ownership,
        }
    }
}

/// Detects other entities (ADR 0015). Pure: equal inputs give equal contacts.
///
/// ```
/// use std::sync::Arc;
/// use cgf_core::Sensor;
/// fn _accepts(_: Arc<dyn Sensor>, _: Box<dyn Sensor>) {}
/// ```
pub trait Sensor: Send + Sync {
    /// The sensor's namespaced ID (for example `builtin.visual`).
    fn id(&self) -> &ComponentId;

    /// Appends contacts among `candidates` (all other entities, id order, pre-tick state).
    fn sense(
        &self,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        terrain: &dyn TerrainProvider,
        out: &mut Vec<Contact>,
    );
}
