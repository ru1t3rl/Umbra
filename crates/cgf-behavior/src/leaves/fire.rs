//! `builtin.action.fire`: fire a weapon at a contact (ASM-077, ADR 0016).

use std::sync::Arc;

use cgf_core::{
    Access, Action, ActionInstance, BbType, BehaviorContext, Blackboard, BlackboardParam,
    BuildError, ComponentId, EntityView, InstantiateError, Intent, IntentBuffer, ParamValue,
    Registry, RegistryError, Status, TickRate, WeaponId,
};

use super::{ActionLeaf, check_keys, contact_in, info, key_name, weapon};

const ID: &str = "builtin.action.fire";
const DESCRIPTION: &str = "Fires weapon at the entity stored in the blackboard key target_key (emits Fire) and succeeds in the same tick. Fails, without an intent, when the key is unset or the entity is not a current contact. Fire is only recorded in this version; it has no effect on the target.";

struct Fire {
    id: ComponentId,
    target_key: Box<str>,
    weapon: WeaponId,
}

struct FireInstance {
    target_key: Box<str>,
    weapon: WeaponId,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Action>, BuildError> {
    check_keys(params, &["target_key", "weapon"])?;
    Ok(Arc::new(Fire {
        id: id.clone(),
        target_key: key_name(params, "target_key")?,
        weapon: weapon(params, "weapon")?,
    }))
}

impl Action for Fire {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(FireInstance {
            target_key: self.target_key.clone(),
            weapon: self.weapon.clone(),
        }))
    }
}

impl ActionInstance for FireInstance {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        bb: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let Some(contact) = contact_in(entity, bb, &self.target_key) else {
            return Status::Failure;
        };
        out.push(Intent::Fire {
            target: contact.entity,
            weapon: self.weapon.clone(),
        });
        Status::Success
    }

    fn halt(&mut self) {}
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_action(
        Arc::new(ActionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.action.fire.json"),
            build,
        )),
        info(DESCRIPTION).with_blackboard_param(BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read,
        )),
    )
}
