//! `builtin.cond.target_in_range`: the stored target is a contact within range (ASM-077).

use std::sync::Arc;

use cgf_core::{
    Access, BbType, BehaviorContext, Blackboard, BlackboardParam, BuildError, ComponentId,
    Condition, EntityView, ParamValue, Registry, RegistryError,
};

use super::{ConditionLeaf, check_keys, contact_in, info, key_name, positive};

const ID: &str = "builtin.cond.target_in_range";
const DESCRIPTION: &str = "Succeeds when the entity stored in the blackboard key target_key is a current contact within range_m meters. Fails when the key is unset or the entity is not (or no longer) a contact.";

struct TargetInRange {
    id: ComponentId,
    target_key: Box<str>,
    range_m: f64,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
    check_keys(params, &["target_key", "range_m"])?;
    Ok(Arc::new(TargetInRange {
        id: id.clone(),
        target_key: key_name(params, "target_key")?,
        range_m: positive(params, "range_m")?,
    }))
}

impl Condition for TargetInRange {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn check(
        &self,
        _: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
    ) -> bool {
        contact_in(entity, bb, &self.target_key).is_some_and(|c| c.range.0 <= self.range_m)
    }
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_condition(
        Arc::new(ConditionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.cond.target_in_range.json"),
            build,
        )),
        info(DESCRIPTION).with_blackboard_param(BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read,
        )),
    )
}
