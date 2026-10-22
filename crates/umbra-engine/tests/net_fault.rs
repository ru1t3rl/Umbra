//! Fault policy of a network adapter at engine level (M6 WI-9). Requirements: REQ-N-1, ADR 0030,
//! ASM-252: under `abort` the run ends after the tick in which an adapter faulted (in `poll` or in
//! `publish`; `NetLink` compares the faulted count before and after each); under
//! `disable_capability` the adapter is disabled and the run goes on.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;
mod net_support;

use std::ops::ControlFlow;
use std::sync::Arc;

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{
    BuildError, ComponentId, ControlAction, Factory, ParamSchema, ParamValue, SimTime,
};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::bus::{RunState, StopReason};
use umbra_engine::handle::{EngineHandle, EngineThread};
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};
use umbra_engine::{EngineError, ManualWallClock, Pumped, RunUntil};
use umbra_net::ManagerError;

/// Which call fails.
#[derive(Clone, Copy, PartialEq)]
enum Fail {
    Poll,
    Publish,
    /// Any run-control message.
    Control,
    /// A `Stop` control message only.
    Stop,
}

struct FaultConfig(Fail);
struct FaultAdapter(Fail);

impl NetworkAdapterConfig for FaultConfig {
    fn key_space(&self) -> KeySpace {
        KeySpace {
            base: 10,
            count: 10,
        }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(FaultAdapter(self.0)))
    }
}

impl NetworkAdapter for FaultAdapter {
    fn publish(&mut self, _: SimTime, out: &[NetMessage]) -> Result<(), NetError> {
        let broken = match self.0 {
            Fail::Publish => true,
            Fail::Control => out
                .iter()
                .any(|m| matches!(m, NetMessage::SimulationControl(_))),
            Fail::Stop => out
                .iter()
                .any(|m| matches!(m, NetMessage::SimulationControl(ControlAction::Stop))),
            Fail::Poll => false,
        };
        if broken {
            Err(NetError::Faulted("send broke".into()))
        } else {
            Ok(())
        }
    }
    fn poll(&mut self, _: SimTime, _: usize, _: &mut Vec<NetMessage>) -> Result<(), NetError> {
        if self.0 == Fail::Poll {
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
        let fail = match params
            .get("params")
            .and_then(|p| p.get("fail"))
            .and_then(ParamValue::as_str)
        {
            Some("poll") => Fail::Poll,
            Some("control") => Fail::Control,
            Some("stop") => Fail::Stop,
            _ => Fail::Publish,
        };
        Ok(Arc::new(FaultConfig(fail)))
    }
}

fn registry() -> umbra_engine::Registry {
    let mut registry = builtin_registry().expect("registry");
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

fn engine(dir: &str) -> umbra_engine::Engine {
    let registry = registry();
    let scenario = load_scenario(&registry, &net_support::fixture(dir)).expect("load");
    instantiate(
        &registry,
        &scenario,
        RunOptions {
            wall: Box::new(ManualWallClock::new()),
            seed: None,
            clock: Some(umbra_engine::ClockMode::AsFastAsPossible),
        },
    )
    .expect("instantiate")
    .engine
}

fn abort_ends_the_run_after_the_faulting_tick(dir: &str) {
    let mut e = engine(dir);
    let mut seen = 0_u64;

    let result = e.run(RunUntil::Ticks(100), |_| {
        seen += 1;
        ControlFlow::Continue(())
    });

    let Err(EngineError::NetworkFault(fault)) = result else {
        panic!("expected a network fault, got {result:?}");
    };
    assert_eq!(fault.adapter, "bad");
    assert_eq!(
        seen, 1,
        "the callback saw the faulting tick, then the run stopped"
    );
    assert_eq!(e.now().tick(), 1);
}

#[test]
fn abort_policy_ends_the_run_after_a_publish_fault() {
    abort_ends_the_run_after_the_faulting_tick("fault_abort_publish");
}

#[test]
fn abort_policy_ends_the_run_after_a_poll_fault() {
    abort_ends_the_run_after_the_faulting_tick("fault_abort_poll");
}

#[test]
fn pump_reports_a_network_abort_with_the_tick_report() {
    let mut e = engine("fault_abort_publish");

    let pumped = e.pump(std::time::Duration::ZERO).expect("pump");

    let Pumped::NetAborted { report, fault } = pumped else {
        panic!("expected NetAborted, got {pumped:?}");
    };
    assert_eq!(report.time.tick(), 1);
    assert_eq!(fault.adapter, "bad");
}

#[test]
fn disable_capability_policy_disables_the_adapter_and_the_run_goes_on() {
    let mut e = engine("fault_disable_publish");

    let summary = e.run(RunUntil::Ticks(50), |_| ControlFlow::Continue(()));

    assert!(summary.is_ok(), "{summary:?}");
    assert_eq!(e.now().tick(), 50);
    let status = e.network().expect("link").status();
    assert!(status[0].faulted && !status[0].enabled, "{status:?}");
    assert_eq!(
        status[0]
            .fault_reason
            .as_deref()
            .map(|r| r.contains("send broke")),
        Some(true)
    );
    let again = e.network_mut().expect("link").set_enabled("bad", true);
    assert_eq!(again.err(), Some(ManagerError::Faulted("bad".into())));
}

#[test]
fn a_control_fault_under_abort_is_returned_to_the_caller() {
    let mut e = engine("fault_abort_start");

    let fault = e.publish_control(ControlAction::Start);

    assert_eq!(fault.map(|f| f.adapter), Some("bad".to_owned()));
    assert_eq!(
        e.publish_control(ControlAction::Pause),
        None,
        "reported once"
    );
}

fn handle_over(dir: &str) -> (EngineHandle, EngineThread) {
    let mut config = handle_support::frozen_config();
    config.base_registry = registry();
    let (handle, thread) = handle_support::start_engine(config);
    handle
        .load(
            net_support::fixture(dir),
            None,
            Some(umbra_engine::ClockMode::Stepped),
        )
        .expect("load");
    (handle, thread)
}

#[test]
fn a_fault_while_sending_start_stops_the_run_with_that_fault() {
    let (handle, _thread) = handle_over("fault_abort_start");

    let status = handle.start().expect("start is answered");

    assert_eq!(status.state, RunState::Stopped);
    assert!(
        matches!(&status.stop_reason, Some(StopReason::NetworkFault(f)) if f.adapter == "bad"),
        "{status:?}"
    );
}

#[test]
fn a_fault_while_sending_stop_is_the_final_status() {
    let (handle, _thread) = handle_over("fault_abort_stop");
    handle.start().expect("start");

    let status = handle.stop().expect("stop is answered");

    assert_eq!(status.state, RunState::Stopped);
    assert!(
        matches!(&status.stop_reason, Some(StopReason::NetworkFault(f)) if f.adapter == "bad"),
        "{status:?}"
    );
}

#[test]
fn a_publish_fault_in_a_tick_stops_the_handle_run_with_that_fault() {
    let (handle, _thread) = handle_over("fault_abort_publish");
    handle.start().expect("start");

    let _ = handle_support::guarded_step(&handle, 5);

    let status = handle.status().expect("status").run;
    assert_eq!(status.state, RunState::Stopped);
    assert!(
        matches!(&status.stop_reason, Some(StopReason::NetworkFault(_))),
        "{status:?}"
    );
}
