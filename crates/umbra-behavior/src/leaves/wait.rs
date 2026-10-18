//! `builtin.action.wait`: hold position for a time (ASM-077, ASM-078, ASM-092).

use std::sync::Arc;

use umbra_core::{
    Action, ActionInstance, BehaviorContext, Blackboard, BuildError, ComponentId, EntityView,
    InstantiateError, Intent, IntentBuffer, ParamValue, Registry, RegistryError, Seconds, Status,
    TickRate,
};

use super::{ActionLeaf, check_keys, info, positive};

const ID: &str = "builtin.action.wait";

struct Wait {
    id: ComponentId,
    duration_s: f64,
}

struct WaitInstance {
    /// Ticks to hold (at least 1).
    ticks: u64,
    /// Ticks held so far in this activation.
    held: u64,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Action>, BuildError> {
    check_keys(params, &["duration_s"])?;
    Ok(Arc::new(Wait {
        id: id.clone(),
        duration_s: positive(params, "duration_s")?,
    }))
}

impl Action for Wait {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        let ticks =
            rate.ticks_for(Seconds(self.duration_s))
                .ok_or_else(|| InstantiateError::Duration {
                    what: "duration_s".to_owned(),
                    hz: rate.hz(),
                })?;
        Ok(Box::new(WaitInstance {
            ticks: ticks.get(),
            held: 0,
        }))
    }
}

impl ActionInstance for WaitInstance {
    /// Running (with `Hold`) on the first `ticks` ticks, Success on the next, which also resets
    /// the count.
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        _: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        if self.held < self.ticks {
            self.held += 1;
            out.push(Intent::Hold);
            Status::Running
        } else {
            self.held = 0;
            Status::Success
        }
    }

    fn halt(&mut self) {
        self.held = 0;
    }
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory = ActionLeaf::new(
        ID,
        include_str!("../../schemas/builtin.action.wait.json"),
        build,
    );
    let info = info(&factory);
    registry.register_action(Arc::new(factory), info)
}
