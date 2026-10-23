//! The state-changing commands, applied and recorded in one place (REQ-K-7, ADR 0053, ADR 0054,
//! ASM-626, ASM-735).
//!
//! The engine handle applies `spawn`, `remove` and `set_adapter_enabled` through [`CommandApplier`];
//! each one that succeeds is recorded with the tick boundary it was applied at, so what is recorded
//! is exactly what changed state. The replay (M7 WI-14) applies recorded commands through the same
//! functions, on an engine without a recorder, where the recording is a no-op.

use umbra_core::{EntityId, SimEvent};
use umbra_net::{AdapterStatus, ManagerError};
use umbra_record::{RecordedCommand, SpawnRecord};

use crate::engine::Engine;
use crate::session::{Session, SessionError, TemplateSpawn};

/// Applies the commands that change the state of a run between ticks.
pub(crate) struct CommandApplier;

impl CommandApplier {
    /// Spawns from a template; records the full spec and the id it received.
    pub(crate) fn spawn(
        session: &mut Session,
        spec: TemplateSpawn,
    ) -> Result<(EntityId, SimEvent), SessionError> {
        let recorded = SpawnRecord {
            template: spec.template.clone(),
            position: spec.position,
            heading: spec.heading,
            speed: spec.speed,
            name: spec.name.clone(),
            group: spec.group.clone(),
            force: spec.force.clone(),
        };
        let spawned = session.spawn(spec)?;
        session.engine().record_command(RecordedCommand::Spawn {
            spec: recorded,
            result: spawned.0,
        });
        Ok(spawned)
    }

    /// Removes an entity; records it.
    pub(crate) fn remove(session: &mut Session, id: EntityId) -> Result<SimEvent, SessionError> {
        let event = session.remove(id)?;
        session
            .engine()
            .record_command(RecordedCommand::Remove { entity: id });
        Ok(event)
    }

    /// Enables or disables the adapter called `name`; records it.
    pub(crate) fn set_adapter_enabled(
        engine: &mut Engine,
        name: &str,
        enabled: bool,
    ) -> Result<AdapterStatus, ManagerError> {
        let link = engine
            .network_mut()
            .ok_or_else(|| ManagerError::UnknownAdapter(name.to_owned()))?;
        let status = link.set_enabled(name, enabled)?;
        engine.record_command(RecordedCommand::SetAdapterEnabled {
            name: name.to_owned(),
            enabled,
        });
        Ok(status)
    }
}
