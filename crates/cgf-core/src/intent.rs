//! Behavior intents, set v1 (REQ-B-2, ADR 0016, ASM-030, ASM-082).
//!
//! Behaviors express what an entity wants; motion models decide how (REQ-B-1, REQ-M-1). The enum
//! is `#[non_exhaustive]`: later versions add variants, so consumers must ignore unknown ones.

use crate::coords::Ecef;
use crate::id::{ComponentId, EntityId, TemplateId, WeaponId};
use crate::params::ParamValue;
use crate::units::{MetersPerSecond, Radians};

/// Version of the intent set. Adding a variant increments it; changing a variant is not allowed
/// within an ABI major (ADR 0016).
pub const INTENT_SET_VERSION: u32 = 1;

/// What an entity wants to do this tick.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Go to `destination` (ECEF position, D-15) and stop there.
    MoveTo {
        /// Target position, ECEF meters.
        destination: Ecef,
    },
    /// Cruise at this speed (clamped by the model's limits).
    SetSpeed(MetersPerSecond),
    /// Stop and keep the current heading; wins over every other intent.
    Hold,
    /// Follow the entity's initial route once; the kernel expands it in place (ASM-082).
    FollowRoute,
    /// Steer to this true heading when no later `MoveTo` overrides it.
    SetHeading(Radians),
    /// Fire `weapon` at `target`. Recorded only until events land (ADR 0016).
    Fire {
        /// The entity fired at.
        target: EntityId,
        /// The weapon used.
        weapon: WeaponId,
    },
    /// Spawn an entity from `template`. Recorded only until the spawn path lands (ADR 0016).
    Spawn {
        /// Template of the new entity.
        template: TemplateId,
        /// Initial position, ECEF meters.
        position: Ecef,
        /// Initial true heading.
        heading: Radians,
    },
    /// Remove this entity. Recorded only until the spawn path lands (ADR 0016).
    Despawn,
    /// A component-defined intent. Recorded only (ADR 0016).
    Custom {
        /// The component kind that understands the payload.
        kind: ComponentId,
        /// Opaque parameters.
        payload: ParamValue,
    },
}
