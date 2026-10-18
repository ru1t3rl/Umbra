//! `builtin.cond.chance`: a seeded coin flip (ASM-077, REQ-B-16).

use std::sync::Arc;

use cgf_core::{
    BehaviorContext, Blackboard, BuildError, ComponentId, Condition, EntityView, ParamValue,
    Registry, RegistryError,
};

use super::{ConditionLeaf, check_keys, info, probability};

const ID: &str = "builtin.cond.chance";
const DESCRIPTION: &str = "Succeeds with the given probability. Every evaluation draws exactly one number from the entity's own random stream, so results depend on the scenario seed and repeat exactly with the same seed. Probability 0 never succeeds and 1 always does.";

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
    registry.register_condition(
        Arc::new(ConditionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.cond.chance.json"),
            build,
        )),
        info(DESCRIPTION),
    )
}
