//! WI-3: the canonical network model and the adapter contract suite (REQ-N-1, D-9, ADR 0030).
#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use umbra_core::net::contract::{
    check_after_stop, check_poll_max, check_poll_max_fed, check_start_once,
};
use umbra_core::net::{
    AdapterCounters, AdapterStart, InboundEvent, KeySpace, NetEntityType, NetError, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, OutboundEvent,
};
use umbra_core::{ControlAction, DEFAULT_TICK_RATE_HZ, SimTime, TickRate};

#[test]
fn key_space_excludes_base_and_includes_base_plus_count() {
    let ks = KeySpace {
        base: 100,
        count: 5,
    };
    assert!(!ks.contains(NetKey(99)));
    assert!(!ks.contains(NetKey(100)), "base is excluded");
    assert!(ks.contains(NetKey(101)));
    assert!(ks.contains(NetKey(105)), "base + count is included");
    assert!(!ks.contains(NetKey(106)));
}

#[test]
fn key_space_does_not_overflow() {
    let ks = KeySpace {
        base: u64::MAX - 2,
        count: u64::MAX,
    };
    assert!(!ks.contains(NetKey(u64::MAX - 2)));
    assert!(ks.contains(NetKey(u64::MAX)));
    assert!(!ks.contains(NetKey(0)));
    let empty = KeySpace { base: 7, count: 0 };
    assert!(!empty.contains(NetKey(7)));
    assert!(!empty.contains(NetKey(8)));
}

#[test]
fn net_entity_type_keeps_its_text() {
    let t = NetEntityType::new("1.2.225.1.1.1.0");
    assert_eq!(t.as_str(), "1.2.225.1.1.1.0");
    assert_eq!(t, NetEntityType::new("1.2.225.1.1.1.0"));
    assert_ne!(t, NetEntityType::new("other"));
}

/// One deliberate contract violation (or none) for the stub adapter.
#[derive(Clone, Copy, PartialEq)]
enum Flaw {
    None,
    PublishOkAfterStop,
    PollOkAfterStop,
    AppendsThenErrs,
    ClearsInto,
    IgnoresMax,
    UnderDelivers,
    StartsTwice,
    WrongStartError,
}

/// A loopback adapter: what is published is polled back, in order.
struct Stub {
    flaw: Flaw,
    stopped: bool,
    stops: Arc<AtomicUsize>,
    queue: VecDeque<InboundEvent>,
}

fn stopped_error() -> NetError {
    NetError::Faulted("stopped".to_owned())
}

impl NetworkAdapter for Stub {
    fn publish(&mut self, _: SimTime, out: &[OutboundEvent]) -> Result<(), NetError> {
        if self.stopped {
            return if self.flaw == Flaw::PublishOkAfterStop {
                Ok(())
            } else {
                Err(stopped_error())
            };
        }
        self.queue.extend(out.iter().cloned());
        Ok(())
    }
    fn poll(
        &mut self,
        _: SimTime,
        max: usize,
        into: &mut Vec<InboundEvent>,
    ) -> Result<(), NetError> {
        if self.stopped {
            match self.flaw {
                Flaw::PollOkAfterStop => return Ok(()),
                Flaw::AppendsThenErrs => {
                    into.push(NetMessage::SimulationControl(ControlAction::Start));
                    return Err(stopped_error());
                }
                _ => return Err(stopped_error()),
            }
        }
        if self.flaw == Flaw::ClearsInto {
            into.clear();
        }
        let max = match self.flaw {
            Flaw::IgnoresMax => max + 1,
            Flaw::UnderDelivers => max.saturating_sub(1),
            _ => max,
        };
        for _ in 0..max {
            match self.queue.pop_front() {
                Some(m) => into.push(m),
                None => break,
            }
        }
        Ok(())
    }
    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
        self.stopped = true;
    }
}

struct StubConfig {
    flaw: Flaw,
    started: AtomicBool,
    stops: Arc<AtomicUsize>,
}

impl StubConfig {
    fn new(flaw: Flaw) -> Self {
        Self {
            flaw,
            started: AtomicBool::new(false),
            stops: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl NetworkAdapterConfig for StubConfig {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 10 }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        if self.started.swap(true, Ordering::SeqCst) && self.flaw != Flaw::StartsTwice {
            return Err(if self.flaw == Flaw::WrongStartError {
                NetError::Start("again".to_owned())
            } else {
                NetError::AlreadyStarted
            });
        }
        Ok(Box::new(Stub {
            flaw: self.flaw,
            stopped: false,
            stops: Arc::clone(&self.stops),
            queue: if self.flaw == Flaw::IgnoresMax {
                // Inbound traffic is already waiting, so even the loose check can see the flaw.
                (0..3)
                    .map(|_| NetMessage::SimulationControl(ControlAction::Pause))
                    .collect()
            } else {
                VecDeque::new()
            },
        }))
    }
}

#[expect(clippy::unwrap_used, reason = "test code")]
fn feed(adapter: &mut dyn NetworkAdapter, msgs: &[NetMessage]) {
    let now = SimTime::start(TickRate::new(DEFAULT_TICK_RATE_HZ).unwrap());
    adapter.publish(now, msgs).unwrap();
}

mod concrete_config {
    use super::*;
    umbra_core::network_adapter_contract_tests!(StubConfig::new(Flaw::None));
}

mod concrete_config_fed {
    use super::*;
    umbra_core::network_adapter_contract_tests!(StubConfig::new(Flaw::None), feed = feed);
}

mod dyn_config {
    use super::*;
    umbra_core::network_adapter_contract_tests!(
        Arc::new(StubConfig::new(Flaw::None)) as Arc<dyn NetworkAdapterConfig>
    );
}

/// A configuration whose `start` always fails, so every generated contract test fails.
struct NeverStarts;

impl NetworkAdapterConfig for NeverStarts {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 1 }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Err(NetError::Start("never".to_owned()))
    }
}

/// The `attrs = [..]` form puts its attributes on every generated test (ASM-280): these tests
/// all fail inside, so they pass only because `#[should_panic]` was applied to each.
mod attributes_are_applied {
    use super::*;
    umbra_core::network_adapter_contract_tests!(attrs = [#[should_panic(expected = "start")]], NeverStarts);
}

mod attributes_are_applied_with_a_feed {
    use super::*;
    umbra_core::network_adapter_contract_tests!(
        attrs = [#[should_panic(expected = "start")]],
        NeverStarts,
        feed = feed
    );
}

/// Runs the four checks on a fresh config with `flaw`; `None` where the check passed.
fn failures(flaw: Flaw) -> [Option<String>; 4] {
    let cfg = || StubConfig::new(flaw);
    [
        check_start_once(&cfg()).err(),
        check_after_stop(&cfg()).err(),
        check_poll_max(&cfg()).err(),
        check_poll_max_fed(&cfg(), &feed).err(),
    ]
}

fn assert_fails_only(flaw: Flaw, indices: &[usize], text: &str) {
    let got = failures(flaw);
    for (i, r) in got.iter().enumerate() {
        match (indices.contains(&i), r) {
            (true, Some(msg)) => assert!(msg.contains(text), "check {i}: {msg}"),
            (true, None) => panic!("check {i} should fail"),
            (false, Some(msg)) => panic!("check {i} should pass: {msg}"),
            (false, None) => {}
        }
    }
}

#[test]
fn a_conforming_adapter_passes_every_check() {
    assert_eq!(failures(Flaw::None), [None, None, None, None]);
}

#[test]
fn starting_twice_fails_only_check_start_once() {
    assert_fails_only(Flaw::StartsTwice, &[0], "second start succeeded");
}

#[test]
fn a_wrong_second_start_error_fails_only_check_start_once() {
    assert_fails_only(Flaw::WrongStartError, &[0], "want AlreadyStarted");
}

#[test]
fn publish_ok_after_stop_fails_only_check_after_stop() {
    assert_fails_only(Flaw::PublishOkAfterStop, &[1], "publish after stop");
}

#[test]
fn poll_ok_after_stop_fails_only_check_after_stop() {
    assert_fails_only(Flaw::PollOkAfterStop, &[1], "poll after stop returned Ok");
}

#[test]
fn appending_then_erring_after_stop_fails_only_check_after_stop() {
    assert_fails_only(Flaw::AppendsThenErrs, &[1], "appended messages");
}

#[test]
fn clearing_into_fails_both_poll_checks() {
    assert_fails_only(Flaw::ClearsInto, &[2, 3], "appended");
}

#[test]
fn ignoring_max_fails_both_poll_checks_when_traffic_waits() {
    assert_fails_only(Flaw::IgnoresMax, &[2, 3], "appended");
}

#[test]
fn delivering_fewer_than_max_is_caught_by_the_fed_check_only() {
    assert_fails_only(Flaw::UnderDelivers, &[3], "appended");
}

#[test]
fn a_wrongly_successful_second_start_is_stopped_before_the_error() {
    let cfg = StubConfig::new(Flaw::StartsTwice);
    assert!(check_start_once(&cfg).is_err());
    assert_eq!(cfg.stops.load(Ordering::SeqCst), 2, "both adapters stopped");
}
