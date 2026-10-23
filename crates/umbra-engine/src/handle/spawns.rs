//! `spawn` and `remove` (REQ-X-5, ADR 0026): applied between ticks, each followed by a control
//! frame that carries the `EntityCreated` or `EntityRemoved` event, then the reply.

use umbra_core::{EntityId, SimEvent};

use super::commands::need;
use super::thread::{Run, Worker};
use super::{Applied, EntityState, HandleError, Reply};
use crate::bus::RunState;
use crate::commands::CommandApplier;
use crate::session::TemplateSpawn;

/// The states in which entities may be spawned and removed.
const EDITABLE: [RunState; 3] = [RunState::Ready, RunState::Running, RunState::Paused];

impl Worker {
    /// The event is published before the entity is read back, so a spawn that happened is never
    /// reported as an error with its event lost.
    pub(super) fn spawn_entity(&mut self, spec: TemplateSpawn, reply: Reply<EntityState>) {
        let (id, event) = match self.spawn_now(spec) {
            Ok(spawned) => spawned,
            Err(error) => return reply.complete(Err(error)),
        };
        self.publish_control(vec![event]);
        let state = self.run_mut().and_then(|run| run.entity_state(id));
        reply.complete(state);
    }

    pub(super) fn remove_entity(&mut self, id: EntityId, reply: Reply<Applied>) {
        match self.remove_now(id) {
            Ok((applied, event)) => {
                self.publish_control(vec![event]);
                reply.complete(Ok(applied));
            }
            Err(error) => reply.complete(Err(error)),
        }
    }

    fn spawn_now(&mut self, spec: TemplateSpawn) -> Result<(EntityId, SimEvent), HandleError> {
        let run = self.run_mut()?;
        need(run, &EDITABLE, "spawn")?;
        let spawned = CommandApplier::spawn(&mut run.session, spec)?;
        refresh_directory(run);
        Ok(spawned)
    }

    fn remove_now(&mut self, id: EntityId) -> Result<(Applied, SimEvent), HandleError> {
        let run = self.run_mut()?;
        need(run, &EDITABLE, "remove")?;
        let event = CommandApplier::remove(&mut run.session, id)?;
        refresh_directory(run);
        let applied = Applied {
            applied_at: run.session.engine().now(),
        };
        Ok((applied, event))
    }
}

/// Rebuilds the directory frames share: the entity set changed.
fn refresh_directory(run: &mut Run) {
    run.refresh_directory();
}
