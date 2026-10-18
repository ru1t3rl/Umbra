//! `builtin.cond.chance`: a seeded coin flip (ASM-077, REQ-B-16).

use std::sync::Arc;

use umbra_core::{
    BehaviorContext, Blackboard, BuildError, ComponentId, Condition, EntityView, ParamValue,
    Registry, RegistryError,
};

use super::{ConditionLeaf, check_keys, info, probability};

const ID: &str = "builtin.cond.chance";

struct Chance {
    id: ComponentId,
    probability: f64,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
    check_keys(params, &["probability"])?;
    Ok(Arc::new(Chance {
        id: id.clone(),
        probability: probability(params, "probability")?,
    }))
}

impl Condition for Chance {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    /// One `next_f64()` in `[0, 1)` per call, whatever the probability, so the stream position
    /// does not depend on the parameters. `draw < p` makes 0 never and 1 always succeed.
    fn check(&self, ctx: &mut BehaviorContext<'_>, _: &EntityView<'_>, _: &mut Blackboard) -> bool {
        ctx.rng.next_f64() < self.probability
    }
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory = ConditionLeaf::new(
        ID,
        include_str!("../../schemas/builtin.cond.chance.json"),
        build,
    );
    let info = info(&factory);
    registry.register_condition(Arc::new(factory), info)
}
