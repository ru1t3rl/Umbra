//! `builtin.action.move_to`: approach a contact (ASM-077).

use std::sync::Arc;

use cgf_core::{
    Access, Action, ActionInstance, BbType, BehaviorContext, Blackboard, BlackboardParam,
    BuildError, ComponentId, EntityView, InstantiateError, Intent, IntentBuffer, MetersPerSecond,
    ParamValue, Registry, RegistryError, Status, TickRate,
};

use super::{ActionLeaf, check_keys, contact_in, info, key_name, non_negative, positive};

const ID: &str = "builtin.action.move_to";
const DESCRIPTION: &str = "Moves toward the entity stored in the blackboard key target_key at speed_mps (emits SetSpeed and MoveTo). Running while the target is farther than arrival_radius_m, Success once it is within that distance. Fails when the key is unset or the entity is not a current contact.";

struct MoveTo {
    id: ComponentId,
    target_key: Box<str>,
    speed_mps: f64,
    arrival_radius_m: f64,
}

struct MoveToInstance {
    target_key: Box<str>,
    speed_mps: f64,
    arrival_radius_m: f64,
}

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Action>, BuildError> {
    check_keys(params, &["target_key", "speed_mps", "arrival_radius_m"])?;
    Ok(Arc::new(MoveTo {
        id: id.clone(),
        target_key: key_name(params, "target_key")?,
        speed_mps: positive(params, "speed_mps")?,
        arrival_radius_m: non_negative(params, "arrival_radius_m")?,
    }))
}

impl Action for MoveTo {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(MoveToInstance {
            target_key: self.target_key.clone(),
            speed_mps: self.speed_mps,
            arrival_radius_m: self.arrival_radius_m,
        }))
    }
}

impl ActionInstance for MoveToInstance {
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
        if contact.range.0 <= self.arrival_radius_m {
            return Status::Success;
        }
        out.push(Intent::SetSpeed(MetersPerSecond(self.speed_mps)));
        out.push(Intent::MoveTo {
            destination: contact.position,
        });
        Status::Running
    }

    fn halt(&mut self) {}
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_action(
        Arc::new(ActionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.action.move_to.json"),
            build,
        )),
        info(DESCRIPTION).with_blackboard_param(BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read,
        )),
    )
}
