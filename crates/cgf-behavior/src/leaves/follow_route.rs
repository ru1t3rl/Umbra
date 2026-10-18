//! `builtin.action.follow_route`: follow the initial route (ASM-077, ADR 0016).

use std::sync::Arc;

use cgf_core::{
    Action, ActionInstance, BehaviorContext, Blackboard, BuildError, ComponentId, EntityView,
    InstantiateError, Intent, IntentBuffer, ParamValue, Registry, RegistryError, Status, TickRate,
};

use super::{ActionLeaf, check_keys, info};

const ID: &str = "builtin.action.follow_route";
const DESCRIPTION: &str = "Follows the entity's initial route using the kernel's route following. Running until the last waypoint is reached, then Success. Fails when the entity has no route. Takes no parameters.";

struct FollowRoute {
    id: ComponentId,
}

struct FollowRouteInstance;

fn build(id: &ComponentId, params: &ParamValue) -> Result<Arc<dyn Action>, BuildError> {
    check_keys(params, &[])?;
    Ok(Arc::new(FollowRoute { id: id.clone() }))
}

impl Action for FollowRoute {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Ok(Box::new(FollowRouteInstance))
    }
}

impl ActionInstance for FollowRouteInstance {
    /// Emits `FollowRoute` on every tick with a route, also on the tick the route completes (the
    /// kernel expands a finished route to `Hold`, so the entity stays at the last waypoint).
    /// Without a route, or with an empty one, it fails and emits nothing.
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        _: &mut Blackboard,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        match entity.route {
            Some(route) if !route.waypoints.is_empty() => {
                out.push(Intent::FollowRoute);
                if route.complete {
                    Status::Success
                } else {
                    Status::Running
                }
            }
            _ => Status::Failure,
        }
    }

    fn halt(&mut self) {}
}

pub(super) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_action(
        Arc::new(ActionLeaf::new(
            ID,
            include_str!("../../schemas/builtin.action.follow_route.json"),
            build,
        )),
        info(DESCRIPTION),
    )
}
