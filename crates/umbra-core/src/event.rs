//! Canonical simulation events, set v1 (REQ-C-5, ADR 0028).
//!
//! One event model for the snapshot stream (M5) and the network adapters (M6, D-9). The enums are
//! `#[non_exhaustive]`: `Detonation`, `Collision` and `EntityStateUpdate` are added when they get a
//! producer, so consumers must ignore unknown kinds. Events are output only: they are never hashed
//! (ADR 0007) and never change simulation state.

use crate::id::{ComponentId, EntityId, TemplateId, WeaponId};
use crate::params::ParamValue;

/// Something that happened in the simulation, reported once.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SimEvent {
    /// An entity was created at run time (API spawns only; the initial load emits none).
    EntityCreated {
        /// The new entity.
        entity: EntityId,
        /// The template it was built from, if any.
        template: Option<TemplateId>,
    },
    /// An entity was removed. Its id is never reused (REQ-C-2).
    EntityRemoved {
        /// The removed entity.
        entity: EntityId,
    },
    /// An entity fired a weapon at another (from a `Fire` intent, ADR 0016).
    Fire {
        /// The entity that emitted the intent.
        shooter: EntityId,
        /// The entity fired at.
        target: EntityId,
        /// The weapon used.
        weapon: WeaponId,
    },
    /// A component-defined event (from a `Custom` intent, or injected from outside).
    Custom {
        /// The emitting entity, if there is one.
        source: Option<EntityId>,
        /// The component kind that understands the payload.
        kind: ComponentId,
        /// Opaque parameters.
        payload: ParamValue,
    },
    /// A run-control change.
    SimulationControl(ControlAction),
}

/// A run-control action reported by [`SimEvent::SimulationControl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlAction {
    /// The run started.
    Start,
    /// The run paused.
    Pause,
    /// The run resumed.
    Resume,
    /// The run stopped.
    Stop,
    /// This many ticks were requested in stepped mode.
    Step(u64),
}

impl SimEvent {
    /// The stable, lower-case kind name of this event: the vocabulary of the stream's event-kind
    /// filter (ASM-170). It never depends on the payload.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::EntityCreated { .. } => "entity_created",
            Self::EntityRemoved { .. } => "entity_removed",
            Self::Fire { .. } => "fire",
            Self::Custom { .. } => "custom",
            Self::SimulationControl(_) => "simulation_control",
        }
    }
}
