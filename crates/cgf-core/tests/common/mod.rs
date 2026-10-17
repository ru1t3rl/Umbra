//! Stub factories shared by the M3 registry and catalog tests.
#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::sync::Arc;

use cgf_core::{
    Action, ActionInstance, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, Blackboard,
    BuildError, ComponentId, ComponentInfo, Condition, ContactBuffer, EntityView, Factory,
    InstantiateError, Observer, ParamSchema, ParamValue, Provenance, Registry, SenseContext,
    Sensor, Target, TickRate, Version,
};

pub fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("{e}"))
}

pub fn schema() -> ParamSchema {
    ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap_or_else(|e| panic!("{e}"))
}

pub fn info() -> ComponentInfo {
    ComponentInfo::new("stub", Provenance::Builtin, Version::new(1, 0, 0))
}

macro_rules! stub {
    ($name:ident, $product:ident, $body:expr) => {
        pub struct $name {
            id: ComponentId,
            schema: ParamSchema,
        }
        impl $name {
            pub fn arc(id: &str) -> Arc<dyn Factory<dyn $product>> {
                Arc::new(Self {
                    id: cid(id),
                    schema: schema(),
                })
            }
        }
        impl Factory<dyn $product> for $name {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn param_schema(&self) -> &ParamSchema {
                &self.schema
            }
            fn build(&self, _: &ParamValue) -> Result<Arc<dyn $product>, BuildError> {
                Ok(Arc::new($body(&self.id)))
            }
        }
    };
}

pub struct BehaviorProduct(BehaviorId);
impl Behavior for BehaviorProduct {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "stub".into(),
        })
    }
}
pub struct ConditionProduct(ComponentId);
impl ConditionProduct {
    fn new(id: &ComponentId) -> Self {
        Self(id.clone())
    }
}
impl Condition for ConditionProduct {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn check(&self, _: &mut BehaviorContext<'_>, _: &EntityView<'_>, _: &mut Blackboard) -> bool {
        false
    }
}
pub struct ActionProduct(ComponentId);
impl ActionProduct {
    fn new(id: &ComponentId) -> Self {
        Self(id.clone())
    }
}
impl Action for ActionProduct {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn ActionInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "stub".into(),
        })
    }
}
pub struct SensorProduct(ComponentId);
impl SensorProduct {
    fn new(id: &ComponentId) -> Self {
        Self(id.clone())
    }
}
impl Sensor for SensorProduct {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn sense(
        &self,
        _: &mut SenseContext<'_>,
        _: &Observer<'_>,
        _: &[Target<'_>],
        _: &mut ContactBuffer<'_>,
    ) {
    }
}

impl BehaviorProduct {
    fn new(id: &ComponentId) -> Self {
        Self(BehaviorId::new(id.as_str()).unwrap_or_else(|e| panic!("{e}")))
    }
}

stub!(BehaviorStub, Behavior, BehaviorProduct::new);
stub!(ConditionStub, Condition, ConditionProduct::new);
stub!(ActionStub, Action, ActionProduct::new);
stub!(SensorStub, Sensor, SensorProduct::new);

pub fn empty_registry_with_one_of_each() -> Registry {
    let mut r = Registry::new();
    r.register_behavior(BehaviorStub::arc("t.behavior"), info())
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_condition(ConditionStub::arc("t.cond"), info())
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_action(ActionStub::arc("t.action"), info())
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_sensor(SensorStub::arc("t.sensor"))
        .unwrap_or_else(|e| panic!("{e}"));
    r
}
