//! ASM-236 and ADR 0031 at engine level (ASM-457): one log warning per unmapped entity type and
//! adapter, one per adapter whose key space ran out, with two adapters at once (ASM-410). The
//! state hashes do not depend on logging.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod net_support;

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};
use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{AdapterStart, KeySpace, NetEntityState, NetKey, NetMessage, NetworkAdapter};
use umbra_core::{Ecef, Force, Radians, Registry, SimTime, Vec3, WorldEuler};
use umbra_engine::builtins::builtin_registry;
use umbra_net::LoopbackHub;

use net_support::{Peer, start};

/// Collects the message of every WARN event.
#[derive(Clone, Default)]
struct Warnings(Arc<Mutex<Vec<String>>>);

struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl Subscriber for Warnings {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        *m.level() == Level::WARN
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, e: &Event<'_>) {
        let mut m = Message(String::new());
        e.record(&mut m);
        self.0.lock().expect("lock").push(m.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

/// `left` and `right` on two hubs; `keys` keys each. Returns a raw peer on each hub.
fn registry(keys: u64) -> (Registry, [Box<dyn NetworkAdapter>; 2]) {
    let mut registry = builtin_registry().expect("builtin registry");
    let mut peers = Vec::new();
    for (id, base) in [("test.net.left", 1000), ("test.net.right", 2000)] {
        let hub = LoopbackHub::new(1024);
        let cid = umbra_core::ComponentId::new(id).expect("id");
        registry
            .register_network_adapter(hub.factory(cid.clone(), KeySpace { base, count: keys }))
            .expect("register");
        let input = serde_json::json!({"name": "peer", "params": {}, "templates": {}});
        let peer = hub
            .factory(
                cid,
                KeySpace {
                    base: base + 500,
                    count: 10,
                },
            )
            .build(&input)
            .expect("peer config")
            .start(&AdapterStart {
                now: SimTime::at_tick(net_support_rate(), 0),
            })
            .expect("peer start");
        peers.push(peer);
    }
    let [l, r] = <[_; 2]>::try_from(peers).unwrap_or_else(|_| unreachable!("two peers"));
    (registry, [l, r])
}

fn net_support_rate() -> umbra_core::TickRate {
    umbra_core::TickRate::new(30).expect("rate")
}

fn state(key: u64, entity_type: &str) -> NetMessage {
    NetMessage::EntityState(NetEntityState {
        key: NetKey(key),
        entity_type: Some(umbra_core::net::NetEntityType::new(entity_type)),
        force: Force::new("red").ok(),
        name: None,
        position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
        velocity: Vec3::ZERO,
        acceleration: Vec3::ZERO,
        angular_velocity: Vec3::ZERO,
        orientation: WorldEuler {
            yaw: Radians(0.0),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        },
        dead_reckoning: DeadReckoning::Static,
    })
}

/// Drives the scenario for six ticks with the peers sending `sends`; the last state hash.
fn drive(scenario: &str, keys: u64, sends: &[(usize, &str)]) -> umbra_core::StateHash {
    let (registry, mut peers) = registry(keys);
    let (mut engine, _) = Peer::new(start(scenario, &registry, None));
    for round in 0..6_u64 {
        for (n, (side, ty)) in sends.iter().enumerate() {
            let msg = state(5000 + 10 * (*side as u64) + n as u64 % 3, ty);
            let now = SimTime::at_tick(net_support_rate(), round);
            peers[*side].publish(now, &[msg]).expect("send");
        }
        engine.tick();
    }
    engine.hashes.last().copied().expect("a tick ran")
}

/// The WARN messages of a run and its final hash.
fn run(scenario: &str, keys: u64, sends: &[(usize, &str)]) -> (Vec<String>, umbra_core::StateHash) {
    let warnings = Warnings::default();
    let hash = tracing::subscriber::with_default(warnings.clone(), || drive(scenario, keys, sends));
    let got = warnings.0.lock().expect("lock").clone();
    (got, hash)
}

fn count(warnings: &[String], needle: &str) -> usize {
    warnings.iter().filter(|w| w.contains(needle)).count()
}

#[test]
fn an_unmapped_type_is_warned_once_per_adapter_and_type() {
    let sends = [
        (0, "zebra"),
        (0, "zebra"),
        (1, "zebra"),
        (1, "yak"),
        (0, "tank"),
    ];

    let (warnings, hash) = run("c", 100, &sends);

    assert_eq!(count(&warnings, "no template maps"), 3, "{warnings:?}");
    assert_eq!(
        count(
            &warnings,
            "adapter left: no template maps entity type \"zebra\""
        ),
        1
    );
    assert_eq!(
        count(
            &warnings,
            "adapter right: no template maps entity type \"zebra\""
        ),
        1
    );
    assert_eq!(
        count(
            &warnings,
            "adapter right: no template maps entity type \"yak\""
        ),
        1
    );
    assert_eq!(count(&warnings, "key space"), 0, "{warnings:?}");
    // Logging is not state: a run with no subscriber ends in the same hash.
    assert_eq!(hash, drive("c", 100, &sends));
}

#[test]
fn key_exhaustion_is_warned_once_per_adapter() {
    let (warnings, _) = run("exhaust", 1, &[]);

    assert_eq!(count(&warnings, "key space exhausted"), 2, "{warnings:?}");
    assert_eq!(count(&warnings, "adapter left: key space exhausted"), 1);
    assert_eq!(count(&warnings, "adapter right: key space exhausted"), 1);
}

#[test]
fn nothing_is_warned_when_nothing_is_unmapped_or_exhausted() {
    let (warnings, _) = run("c", 100, &[(0, "tank")]);

    assert_eq!(warnings, Vec::<String>::new());
}
