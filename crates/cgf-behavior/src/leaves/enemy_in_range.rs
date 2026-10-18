//! `builtin.cond.enemy_in_range`: a hostile contact within range (ASM-077, ASM-079).

use std::sync::Arc;

use cgf_core::{
    Access, BbType, BbValue, BehaviorContext, Blackboard, BlackboardParam, BuildError, ComponentId,
    Condition, EntityView, ParamValue, Registry, RegistryError, hostile,
};

use super::{ConditionLeaf, check_keys, info, key_name, positive};

const ID: &str = "builtin.cond.enemy_in_range";

struct EnemyInRange {
    id: ComponentId,
    range_m: f64,
    store_as: Option<Box<str>>,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
    check_keys(params, &["range_m", "store_as"])?;
    let range_m = positive(params, "range_m")?;
    let store_as = if params.get("store_as").is_some() {
        Some(key_name(params, "store_as")?)
    } else {
        None
    };
    Ok(Arc::new(EnemyInRange {
        id: id.clone(),
        range_m,
        store_as,
    }))
}

impl Condition for EnemyInRange {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn check(
        &self,
        _: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
    ) -> bool {
        let nearest = entity
            .contacts
            .iter()
            .filter(|c| hostile(entity.force, c.force.as_ref()) && c.range.0 <= self.range_m)
            .min_by(|a, b| {
                a.range
                    .0
                    .total_cmp(&b.range.0)
                    .then(a.entity.cmp(&b.entity))
            });
        let Some(contact) = nearest else {
            return false;
        };
        match &self.store_as {
            Some(key) => bb.set(key, BbValue::Entity(contact.entity)).is_ok(),
            None => true,
        }
    }
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory = ConditionLeaf::new(
        ID,
        include_str!("../../schemas/builtin.cond.enemy_in_range.json"),
        build,
    );
    let info = info(&factory).with_blackboard_param(BlackboardParam::new(
        "store_as",
        BbType::Entity,
        Access::Write,
    ));
    registry.register_condition(Arc::new(factory), info)
}
