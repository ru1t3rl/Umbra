//! M6 WI-11 (left for it by WI-9; ADR 0030 "Shutdown", ADR 0033 "Threads", REQ-P-8): dropping an
//! engine drops its `NetLink` before its plugin host, so every adapter is stopped while the host
//! still holds its plugin images. The order is the `Engine` field order (`network` before
//! `plugins`); this test pins it with an adapter that, when stopped, records whether the engine's
//! plugin host is still alive.
//!
//! A plugin adapter keeps its own image alive in any case (the bridge's handles hold it,
//! `umbra-plugin-host/tests/network_bridge.rs::stop_and_destroy_run_exactly_once_on_drop_before_the_image_is_released`);
//! this order is the engine's part.
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod net_support;

use core::time::Duration;
use std::sync::{Arc, Mutex, Weak};

use umbra_core::net::{
    AdapterCounters, AdapterStart, InboundEvent, KeySpace, NetError, NetworkAdapter,
    NetworkAdapterConfig, OutboundEvent,
};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, SimTime};
use umbra_engine::ManualWallClock;
use umbra_engine::builtins::builtin_registry;
use umbra_engine::plugins::{HostOptions, PluginHost};
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};

/// What the probing adapter saw: the engine's host (set after attaching) and, per `stop`, whether
/// that host was still alive.
#[derive(Default)]
struct Probe {
    host: Mutex<Option<Weak<PluginHost>>>,
    stops: Mutex<Vec<bool>>,
}

struct ProbeFactory {
    id: ComponentId,
    schema: ParamSchema,
    probe: Arc<Probe>,
}

struct ProbeConfig {
    probe: Arc<Probe>,
}

struct ProbeAdapter {
    probe: Arc<Probe>,
    stopped: bool,
}

impl Factory<dyn NetworkAdapterConfig> for ProbeFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, _: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        Ok(Arc::new(ProbeConfig {
            probe: Arc::clone(&self.probe),
        }))
    }
}

impl NetworkAdapterConfig for ProbeConfig {
    fn key_space(&self) -> KeySpace {
        KeySpace {
            base: 1000,
            count: 100,
        }
    }

    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(ProbeAdapter {
            probe: Arc::clone(&self.probe),
            stopped: false,
        }))
    }
}

impl NetworkAdapter for ProbeAdapter {
    fn publish(&mut self, _: SimTime, _: &[OutboundEvent]) -> Result<(), NetError> {
        Ok(())
    }

    fn poll(&mut self, _: SimTime, _: usize, _: &mut Vec<InboundEvent>) -> Result<(), NetError> {
        Ok(())
    }

    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }

    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let alive = self
            .probe
            .host
            .lock()
            .expect("probe")
            .as_ref()
            .is_some_and(|h| h.upgrade().is_some());
        self.probe.stops.lock().expect("probe").push(alive);
    }
}

#[test]
fn the_network_link_drops_before_the_plugin_host() {
    let probe = Arc::new(Probe::default());
    let mut registry = builtin_registry().expect("the built-in registry builds");
    registry
        .register_network_adapter(Arc::new(ProbeFactory {
            // The adapter id `net_pair/a` names.
            id: ComponentId::new("test.net.loopback").expect("id"),
            schema: ParamSchema::from_json_str(r#"{"type":"object"}"#).expect("schema"),
            probe: Arc::clone(&probe),
        }))
        .expect("register the probe");
    let scenario = load_scenario(&registry, &net_support::fixture("a")).expect("load net_pair/a");
    let run = instantiate(
        &registry,
        &scenario,
        RunOptions {
            wall: Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
                500,
            ))),
            seed: None,
            clock: None,
        },
    )
    .expect("instantiate");
    let mut engine = run.engine;
    let host = Arc::new(PluginHost::new(HostOptions::default()));
    *probe.host.lock().expect("probe") = Some(Arc::downgrade(&host));
    engine.attach_plugins(Arc::clone(&host)).expect("attach");
    let weak = Arc::downgrade(&host);
    drop(host);
    engine
        .step(3, |_| std::ops::ControlFlow::Continue(()))
        .expect("three ticks");
    assert!(weak.upgrade().is_some(), "the engine holds the only host");
    assert_eq!(
        probe.stops.lock().expect("probe").len(),
        0,
        "not stopped while running"
    );

    drop(engine);

    assert_eq!(
        *probe.stops.lock().expect("probe"),
        [true],
        "the adapter was stopped once, while the engine's plugin host was still alive"
    );
    assert!(
        weak.upgrade().is_none(),
        "and the host is gone with the engine"
    );
}
