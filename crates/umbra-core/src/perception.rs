//! Perception types: forces, contacts and the [`Sensor`] trait (REQ-B-5, ADR 0015, ASM-079).
//!
//! The kernel runs sensors for `Owned` entities in `EntityId` order against all other entities as
//! they were before the tick, so contacts do not depend on entity order.

use std::fmt;
use std::sync::Arc;

use crate::coords::Ecef;
use crate::geo::Vec3;
use crate::id::{ComponentId, EntityId, MAX_ID_LEN, SegmentError, validate_segment};
use crate::motion::KinematicState;
use crate::ownership::Ownership;
use crate::rng::RngStream;
use crate::terrain::TerrainProvider;
use crate::time::SimTime;
use crate::units::Meters;

/// The side an entity belongs to; grammar `[a-z][a-z0-9_]*` (ASM-079).
///
/// Ordering is byte-lexicographic, so it is a deterministic map key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Force(Arc<str>);

/// Why a string was rejected as a [`Force`]. Errors echo at most a bounded prefix of the input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ForceError {
    /// The input was empty.
    #[error("force name is empty")]
    Empty,
    /// The input exceeds [`MAX_ID_LEN`] bytes; `prefix` is only a bounded prefix of it.
    #[error("force name {prefix:?} is {len} bytes; max is {max}", max = MAX_ID_LEN)]
    TooLong {
        /// A prefix of the offending input (at most 64 bytes).
        prefix: String,
        /// Real length in bytes.
        len: usize,
    },
    /// The first character is not `a-z`.
    #[error("force name must start with a lowercase letter (byte {index})")]
    InvalidStart {
        /// Byte offset of the offending character (always 0).
        index: usize,
    },
    /// A later character is not `a-z`, `0-9` or `_`.
    #[error("force name has an invalid character at byte {index}")]
    InvalidChar {
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
        validate_segment(s).map_err(|e| match e {
            SegmentError::Empty => ForceError::Empty,
            SegmentError::TooLong { prefix, len } => ForceError::TooLong { prefix, len },
            SegmentError::InvalidStart => ForceError::InvalidStart { index: 0 },
            SegmentError::InvalidChar { index } => ForceError::InvalidChar { index },
        })?;
        Ok(Self(Arc::from(s)))
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
#[non_exhaustive]
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

impl Contact {
    /// Creates a contact.
    pub fn new(
        entity: EntityId,
        position: Ecef,
        velocity: Vec3,
        force: Option<Force>,
        range: Meters,
    ) -> Self {
        Self {
            entity,
            position,
            velocity,
            force,
            range,
        }
    }
}

/// What a sensor may use besides the observer and candidates: time, the observer's own hashed RNG
/// stream (for perception noise later; `builtin.visual` draws nothing) and terrain for line of
/// sight.
#[non_exhaustive]
pub struct SenseContext<'a> {
    /// Simulation time of the current tick.
    pub now: SimTime,
    /// The observer's own hashed RNG stream (ADR 0006).
    pub rng: &'a mut RngStream,
    /// Terrain, for line of sight.
    pub terrain: &'a dyn TerrainProvider,
}

impl<'a> SenseContext<'a> {
    /// Creates a context.
    pub fn new(now: SimTime, rng: &'a mut RngStream, terrain: &'a dyn TerrainProvider) -> Self {
        Self { now, rng, terrain }
    }
}

/// Append-only view of the observer's contacts (same pattern as `IntentBuffer`).
pub struct ContactBuffer<'a>(&'a mut Vec<Contact>);

impl fmt::Debug for ContactBuffer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContactBuffer")
            .field("len", &self.0.len())
            .finish()
    }
}

impl<'a> ContactBuffer<'a> {
    /// Wraps a contact list.
    pub fn new(v: &'a mut Vec<Contact>) -> Self {
        Self(v)
    }

    /// Appends a contact.
    pub fn push(&mut self, c: Contact) {
        self.0.push(c);
    }

    /// Number of contacts in the list, including earlier ones.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
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
/// use umbra_core::Sensor;
/// fn _accepts(_: Arc<dyn Sensor>, _: Box<dyn Sensor>) {}
/// ```
pub trait Sensor: Send + Sync {
    /// The sensor's namespaced ID (for example `builtin.visual`).
    fn id(&self) -> &ComponentId;

    /// Appends contacts among `candidates` (all other entities, id order, pre-tick state).
    /// Deterministic: may use only `ctx` and its arguments.
    fn sense(
        &self,
        ctx: &mut SenseContext<'_>,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        out: &mut ContactBuffer<'_>,
    );
}
