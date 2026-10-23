//! A network adapter that fails its `poll` or its `publish` (`params = { fail = "poll" | "publish" }`)
//! for the recorder's fault rows (M7 WI-13). The id is `test.net.fault`; keys 10..20.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

use std::sync::Arc;

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, Registry, SimTime};

struct FaultConfig {
    poll: bool,
}
struct FaultAdapter {
    poll: bool,
}

impl NetworkAdapterConfig for FaultConfig {
    fn key_space(&self) -> KeySpace {
        KeySpace {
            base: 10,
            count: 10,
        }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(FaultAdapter { poll: self.poll }))
    }
}

impl NetworkAdapter for FaultAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage]) -> Result<(), NetError> {
        if self.poll {
            Ok(())
        } else {
            Err(NetError::Faulted("send broke".into()))
        }
    }
    fn poll(&mut self, _: SimTime, _: usize, _: &mut Vec<NetMessage>) -> Result<(), NetError> {
        if self.poll {
            Err(NetError::Faulted("receive broke".into()))
        } else {
            Ok(())
        }
    }
    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }
    fn stop(&mut self) {}
}

struct FaultFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn NetworkAdapterConfig> for FaultFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        let poll = params
            .get("params")
            .and_then(|p| p.get("fail"))
            .and_then(ParamValue::as_str)
            == Some("poll");
        Ok(Arc::new(FaultConfig { poll }))
    }
}

/// `registry` plus the failing adapter.
pub fn with_fault_adapter(mut registry: Registry) -> Registry {
    registry
        .register_network_adapter(Arc::new(FaultFactory {
            id: ComponentId::new("test.net.fault").expect("id"),
            schema: ParamSchema::from_json_str(
                r#"{"type":"object","properties":{"fail":{"type":"string"}},"additionalProperties":false}"#,
            )
            .expect("schema"),
        }))
        .expect("register");
    registry
}
