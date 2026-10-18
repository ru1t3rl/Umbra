//! `builtin.action.follow_route`: follow the initial route (ASM-077, ADR 0016).

use std::sync::Arc;

use umbra_core::{
    Action, ActionInstance, BehaviorContext, Blackboard, BuildError, ComponentId, EntityView,
    InstantiateError, Intent, IntentBuffer, ParamValue, Registry, RegistryError, Status, TickRate,
};

use super::{ActionLeaf, check_keys, info};

const ID: &str = "builtin.action.follow_route";

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
    let factory = ActionLeaf::new(
        ID,
        include_str!("../../schemas/builtin.action.follow_route.json"),
        build,
    );
    let info = info(&factory);
    registry.register_action(Arc::new(factory), info)
}
