//! `builtin.cond.timer_elapsed`: simulation time since the start of the run (ASM-077, ASM-078).

use std::sync::Arc;

use cgf_core::{
    BehaviorContext, Blackboard, BuildError, ComponentId, Condition, EntityView, ParamValue,
    Registry, RegistryError, Seconds,
};

use super::{ConditionLeaf, check_keys, info, positive};

const ID: &str = "builtin.cond.timer_elapsed";
const DESCRIPTION: &str = "Succeeds once the simulation time since the start of the run is at least duration_s seconds, rounded up to whole ticks. It measures the run, not the time since the node last ran; use a cooldown or timeout node for relative time.";

struct TimerElapsed {
    id: ComponentId,
    duration_s: f64,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
    check_keys(params, &["duration_s"])?;
    Ok(Arc::new(TimerElapsed {
        id: id.clone(),
        duration_s: positive(params, "duration_s")?,
    }))
}

impl Condition for TimerElapsed {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    /// The rate comes with the tick's time, so no instance is needed. A duration too long for
    /// `u64` ticks never elapses.
    fn check(&self, ctx: &mut BehaviorContext<'_>, _: &EntityView<'_>, _: &mut Blackboard) -> bool {
        ctx.now
            .rate()
            .ticks_for(Seconds(self.duration_s))
            .is_some_and(|n| ctx.now.tick() >= n.get())
    }
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_condition(
        Arc::new(ConditionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.cond.timer_elapsed.json"),
            build,
        )),
        info(DESCRIPTION),
    )
}
