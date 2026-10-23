//! AT-12 (M7): the engine applies `Spawn` and `Despawn` intents right after `Kernel::tick`.
//! Requirements: REQ-B-2, REQ-C-2, REQ-C-5, REQ-K-5, D-46, ADR 0052, ASM-600..606, ASM-644,
//! ASM-645.
//!
//! The emitting behavior is `test.behavior.launcher` (`m7_support`), the scenarios are
//! `fixtures/m7/spawn_*`. Written in M7 WI-1 against the `Spawner` stub: every row that needs the
//! application was ignored until M7 WI-4 and is live since.
//!
//! The total-entity cap and the duplicate `Despawn` are also unit-tested in `spawner.rs` (the cap
//! cannot be lowered from outside the crate). Not covered by tests (ASM-686): the cap warning line.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod m7_support;
mod net_support;
mod plugin_support;

use m7_support::{
    capture_warnings, fixture, launcher_registry, plugin_lock, start, stepper, with_launcher,
};
use net_support::{Peer, registry_with};
use umbra_core::{EntityId, LocalFrame, Ownership, SimEvent, TemplateId};
use umbra_engine::handle::{EngineHandle, HandleConfig};
use umbra_engine::{ClockMode, MAX_SPAWNS_PER_TICK};
use umbra_net::LoopbackHub;

fn id(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn template(s: &str) -> TemplateId {
    TemplateId::new(s).expect("valid template id")
}

fn created(events: &[SimEvent]) -> Vec<(EntityId, Option<TemplateId>)> {
    events
        .iter()
        .filter_map(|e| match e {
            SimEvent::EntityCreated { entity, template } => Some((*entity, template.clone())),
            _ => None,
        })
        .collect()
}

fn removed(events: &[SimEvent]) -> Vec<EntityId> {
    events
        .iter()
        .filter_map(|e| match e {
            SimEvent::EntityRemoved { entity } => Some(*entity),
            _ => None,
        })
        .collect()
}

/// Fixtures are valid: every `m7` scenario loads and instantiates on the launcher registry.
#[test]
fn given_the_m7_fixtures_when_loaded_then_each_scenario_instantiates() {
    for dir in [
        "spawn_basic",
        "spawn_order",
        "spawn_leaver",
        "spawn_selfish",
        "spawn_flood",
        "spawn_missing",
        "spawn_missing_twice",
        "spawn_double_despawn",
        "handle_run",
    ] {
        let hub = LoopbackHub::new(16);
        let registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));

        let run = start(&registry, &fixture(dir));

        assert!(!run.entities.is_empty(), "{dir}");
    }
}

/// ADR 0052, ASM-600: a spawn emitted in tick 2 is applied at the end of tick 2: the entity exists
/// in tick 2's snapshot with the next id, and its `EntityCreated` is in tick 2's events.
#[test]
fn given_a_spawn_intent_when_the_tick_ends_then_the_entity_exists_with_the_next_id() {
    let mut s = stepper("spawn_basic");

    s.ticks(3);

    assert_eq!(s.counts, [1, 2, 2]);
    assert_eq!(created(&s.events[1]), [(id(2), Some(template("m7.child")))]);
    assert!(created(&s.events[0]).is_empty() && created(&s.events[2]).is_empty());
}

/// ADR 0052, ASM-601: the spawned entity takes the spawner's force, the intent's position and
/// heading, speed 0, no route, and is Owned.
#[test]
fn given_a_spawn_intent_then_the_entity_inherits_force_and_takes_position_and_heading() {
    let mut s = stepper("spawn_basic");
    s.ticks(2);
    let snapshot = s.engine.snapshot();
    let carrier = &snapshot.entities[0];
    let child = &snapshot.entities[1];

    assert_eq!(child.force, carrier.force);
    assert_eq!(child.ownership, Ownership::Owned);
    assert_eq!(child.route_next, None);
    assert!(child.state.velocity.norm() < 1e-9);
    assert!(child.state.position.distance(carrier.state.position).0 < 1e-6);
    let heading = LocalFrame::at(child.state.position.to_geodetic())
        .euler_from_orientation(child.state.orientation);
    assert!((heading.heading.0 - 0.25).abs() < 1e-6, "{heading:?}");
}

/// ADR 0052, ASM-644: the spawned entity first acts in the next tick. A child that removes itself on
/// its first tick therefore exists after tick 2 (spawned) and is gone after tick 3.
#[test]
fn given_a_self_removing_child_when_spawned_then_it_acts_one_tick_later_and_is_removed() {
    let mut s = stepper("spawn_selfish");

    s.ticks(4);

    assert_eq!(s.counts, [1, 2, 1, 1]);
    assert_eq!(created(&s.events[1]).len(), 1);
    assert_eq!(removed(&s.events[2]), [id(2)]);
    assert!(removed(&s.events[1]).is_empty() && removed(&s.events[3]).is_empty());
    let counters = s.engine.lifecycle_counters();
    assert_eq!((counters.spawned, counters.despawned), (1, 1));
}

/// ADR 0052, ASM-602: spawns apply in (spawner id, emission order) and get monotonic new ids; each
/// child takes its own spawner's force.
#[test]
fn given_two_spawners_when_they_spawn_in_one_tick_then_ids_follow_spawner_then_emission_order() {
    let mut s = stepper("spawn_order");

    s.ticks(1);

    let made = created(&s.events[0]);
    assert_eq!(
        made,
        [
            (id(3), Some(template("m7.child_a"))),
            (id(4), Some(template("m7.child_a"))),
            (id(5), Some(template("m7.child_b"))),
            (id(6), Some(template("m7.child_b"))),
        ]
    );
    let snapshot = s.engine.snapshot();
    let forces: Vec<_> = snapshot.entities.iter().map(|e| e.force.clone()).collect();
    assert_eq!(forces[2], forces[0]);
    assert_eq!(forces[3], forces[0]);
    assert_eq!(forces[4], forces[1]);
    assert_ne!(forces[0], forces[1]);
}

/// ADR 0052, ASM-602: all spawns of a tick come before the despawns, so an entity that spawns and
/// despawns itself in one tick still spawns first, and the child keeps the (gone) spawner's force.
#[test]
fn given_a_spawner_that_despawns_itself_when_it_spawns_then_the_child_exists_and_the_spawn_came_first()
 {
    let mut s = stepper("spawn_leaver");

    s.ticks(1);

    assert_eq!(s.counts, [1], "the carrier is gone, its child exists");
    let events = &s.events[0];
    let created_at = events
        .iter()
        .position(|e| matches!(e, SimEvent::EntityCreated { .. }));
    let removed_at = events
        .iter()
        .position(|e| matches!(e, SimEvent::EntityRemoved { .. }));
    assert!(created_at < removed_at, "{events:?}");
    assert_eq!(removed(events), [id(1)]);
    let snapshot = s.engine.snapshot();
    assert_eq!(snapshot.entities[0].id, id(2));
    assert!(snapshot.entities[0].force.is_some());
}

/// ADR 0052: the spawner's events come after the kernel's events of the same tick.
#[test]
fn given_a_kernel_event_and_a_spawn_in_one_tick_then_the_spawn_event_comes_after_it() {
    let mut s = stepper("spawn_basic");

    s.ticks(2);

    let events = &s.events[1];
    let custom = events
        .iter()
        .position(|e| matches!(e, SimEvent::Custom { .. }));
    let create = events
        .iter()
        .position(|e| matches!(e, SimEvent::EntityCreated { .. }));
    assert!(custom.is_some() && create.is_some(), "{events:?}");
    assert!(custom < create, "{events:?}");
}

/// ASM-603: at most `MAX_SPAWNS_PER_TICK` spawns apply per tick; the rest are dropped in order and
/// counted.
#[test]
fn given_300_spawns_in_one_tick_when_applied_then_the_first_256_exist_and_44_are_counted_capped() {
    let mut s = stepper("spawn_flood");

    s.ticks(2);

    assert_eq!(MAX_SPAWNS_PER_TICK, 256);
    assert_eq!(s.counts, [1 + 256, 1 + 256]);
    let counters = s.engine.lifecycle_counters();
    assert_eq!((counters.spawned, counters.capped), (256, 44));
    let ids: Vec<_> = created(&s.events[0])
        .into_iter()
        .map(|(e, _)| e.get())
        .collect();
    assert_eq!(ids, (2..=257).collect::<Vec<_>>());
}

/// ASM-604: a spawn of an unknown template is dropped and counted; it is not a fault and the run
/// goes on.
#[test]
fn given_an_unknown_template_when_spawned_then_it_is_dropped_counted_and_the_run_continues() {
    let mut s = stepper("spawn_missing");

    s.ticks(5);

    assert_eq!(s.counts, [1; 5]);
    let counters = s.engine.lifecycle_counters();
    assert_eq!((counters.spawned, counters.rejected), (0, 1));
}

/// ASM-606: the entity directory (the control API's view) follows spawns and removals with the
/// template and no name.
#[test]
fn given_spawned_and_removed_entities_when_queried_through_the_handle_then_the_directory_follows() {
    let mut config = HandleConfig::new(launcher_registry());
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            core::time::Duration::from_micros(500),
        ))
    });
    let (handle, _thread) = EngineHandle::spawn(config).expect("spawn the engine thread");
    handle
        .load(fixture("spawn_selfish"), None, Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    handle.step(2).expect("step");
    let after_spawn = handle.entity(id(2)).expect("the spawned entity is known");
    handle.step(1).expect("step");
    let after_removal = handle.entity(id(2));

    assert_eq!(after_spawn.info.template, template("m7.selfish"));
    assert_eq!(after_spawn.info.name, None);
    assert!(after_removal.is_err(), "the removed entity is gone");
}

/// ADR 0052, ASM-645, REQ-N-2: a spawned entity is published after the tick that creates it (so a
/// peer mirrors it within a few ticks; the test looks at tick 10) and its removal reaches the peer
/// as `EntityRemoved`, over the loopback adapter.
#[test]
fn given_a_peer_when_an_entity_is_spawned_and_later_despawned_then_the_peer_gets_and_loses_a_ghost()
{
    let hub = LoopbackHub::new(4096);
    let a_registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));
    let b_registry = registry_with(&[("test.net.loopback", &hub, 2000)]);
    let (mut a, _) = Peer::new(start(&a_registry, &fixture("spawn_net")));
    let (mut b, _) = Peer::new(net_support::start("b", &b_registry, None));

    for _ in 0..10 {
        b.tick();
        a.tick();
    }
    let with_child = b.ghosts().len();
    for _ in 0..40 {
        b.tick();
        a.tick();
    }

    assert_eq!(
        with_child, 2,
        "the carrier and its child are mirrored on the peer"
    );
    assert_eq!(
        b.ghosts().len(),
        1,
        "the child's ghost is gone well before the ghost timeout"
    );
    let removals = b
        .all_events()
        .iter()
        .filter(|e| matches!(e, SimEvent::EntityRemoved { .. }))
        .count();
    assert_eq!(removals, 1);
}

/// ADR 0052, ASM-645, REQ-N-2: the spawn is applied before the tick's own publish. The carrier
/// spawns in its tick 2; the peer, ticking once after that tick, already mirrors the child. (Were the
/// spawn applied after the publish, the child would reach the peer one tick later.)
#[test]
fn given_a_spawn_in_tick_2_when_the_peer_ticks_next_then_the_child_arrived_in_that_tick_s_publish()
{
    let hub = LoopbackHub::new(4096);
    let a_registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));
    let b_registry = registry_with(&[("test.net.loopback", &hub, 2000)]);
    let (mut a, _) = Peer::new(start(&a_registry, &fixture("spawn_net")));
    let (mut b, _) = Peer::new(net_support::start("b", &b_registry, None));
    a.tick();
    b.tick();
    assert_eq!(b.ghosts().len(), 1, "only the carrier after tick 1");

    a.tick();
    b.tick();

    assert_eq!(a.owned().len(), 2, "the child exists after tick 2");
    assert_eq!(b.ghosts().len(), 2, "the child was in tick 2's publish");
}

/// ADR 0052, ASM-645: the despawn is applied before the tick's own publish. The child removes itself
/// on its 30th tick (tick 32 of the run); the peer's next tick already ingests `EntityRemoved`.
#[test]
fn given_a_despawn_in_tick_32_when_the_peer_ticks_next_then_the_removal_arrived_in_that_tick_s_publish()
 {
    let hub = LoopbackHub::new(4096);
    let a_registry = with_launcher(registry_with(&[("test.net.loopback", &hub, 1000)]));
    let b_registry = registry_with(&[("test.net.loopback", &hub, 2000)]);
    let (mut a, _) = Peer::new(start(&a_registry, &fixture("spawn_net")));
    let (mut b, _) = Peer::new(net_support::start("b", &b_registry, None));
    for _ in 0..31 {
        a.tick();
        b.tick();
    }
    assert_eq!(b.ghosts().len(), 2, "the child lives through tick 31");
    let removals = |p: &Peer| {
        p.all_events()
            .iter()
            .filter(|e| matches!(e, SimEvent::EntityRemoved { .. }))
            .count()
    };
    assert_eq!(removals(&b), 0);

    a.tick();
    b.tick();

    assert_eq!(a.owned().len(), 1, "the child is gone after tick 32");
    assert_eq!(b.ghosts().len(), 1, "the removal was in tick 32's publish");
    assert_eq!(removals(&b), 1);
}

/// ASM-685: an entity that emits `Despawn` twice in one tick is removed once: one `EntityRemoved`,
/// one count, no fault, and the run goes on.
#[test]
fn given_a_duplicate_despawn_in_one_tick_then_one_removal_one_count_and_no_fault() {
    let mut s = stepper("spawn_double_despawn");

    s.ticks(3);

    assert_eq!(removed(&s.events[0]), [id(1)], "once, not twice");
    assert_eq!(s.counts, [1, 1, 1], "the child is left");
    let counters = s.engine.lifecycle_counters();
    assert_eq!((counters.spawned, counters.despawned), (1, 1));
    assert_eq!(counters.rejected, 0);
}

/// ASM-604, ASM-690: two rejected spawns of one template in one tick count twice and warn once,
/// naming the spawner and the template; a later rejection of the same template stays silent.
#[test]
fn given_two_rejected_spawns_of_one_template_then_two_counts_and_one_warning() {
    let mut s = stepper("spawn_missing_twice");

    let warnings = capture_warnings(|| s.ticks(3));

    let counters = s.engine.lifecycle_counters();
    assert_eq!((counters.spawned, counters.rejected), (0, 2));
    let spawn_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("spawn dropped"))
        .collect();
    assert_eq!(spawn_warnings.len(), 1, "{warnings:?}");
    assert!(
        spawn_warnings[0].contains("m7.missing") && spawn_warnings[0].contains("spawner=1"),
        "{warnings:?}"
    );
    assert!(!spawn_warnings[0].contains("entity 0"), "{warnings:?}");
}

/// A spawn from a plugin-less launcher whose child template runs a plugin behavior builds inside the
/// plugin build scope (ASM-605, ASM-645): `create_instance` is logged at the spawn, the spawn is
/// applied (not rejected), and `destroy_instance` at the child's despawn. The child's graph runs the
/// plugin's `lifecycle` behavior and a launcher that despawns it, in a `parallel`.
#[test]
fn given_a_child_with_a_plugin_behavior_when_spawned_and_despawned_then_it_builds_in_scope_and_is_destroyed()
 {
    use plugin_support::{Tpl, write_scenario};
    use std::sync::{Arc, Mutex};
    use umbra_core::PluginId;
    use umbra_engine::{ManualWallClock, PluginLog, Session, SessionOptions};
    use umbra_plugin_host::LogLevel;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);
    impl PluginLog for Recorder {
        fn log(&self, _: &PluginId, _: LogLevel, message: &str) {
            self.0.lock().expect("log lock").push(message.to_owned());
        }
    }
    let _lock = plugin_lock();
    let plugins = plugin_support::fixture(&[]);
    let graph = "schema_version = 1\nid = \"test.graph.life_then_leave\"\nversion = \"1.0.0\"\n\
                 root = \"p\"\n\n\
                 [nodes.p]\ntype = \"parallel\"\nchildren = [\"life\", \"leave\"]\n\n\
                 [nodes.life]\ntype = \"subgraph\"\nref = \"test.behavior.lifecycle\"\n\n\
                 [nodes.leave]\ntype = \"subgraph\"\nref = \"test.behavior.launcher\"\n\
                 params = { despawn_at = 2 }\n"
        .to_owned();
    let scenario = write_scenario(
        "spawn-plugin-scope",
        2.0,
        &format!("plugin_paths = [{:?}]", plugins.to_str().expect("utf-8")),
        &[
            Tpl {
                id: "test.entity.carrier",
                behavior: "test.behavior.launcher",
                params: "{ spawn_at = [1], template = \"test.entity.child\" }",
            },
            Tpl {
                id: "test.entity.child",
                behavior: "test.graph.life_then_leave",
                params: "",
            },
        ],
        &[("life_then_leave", graph)],
        &[("test.entity.carrier", "carrier")],
    );
    let log = Arc::new(Recorder::default());
    let mut options = SessionOptions::new(Box::new(ManualWallClock::with_auto_advance(
        core::time::Duration::from_micros(500),
    )));
    let hook: Arc<dyn PluginLog> = log.clone();
    options.plugin_log = Some(hook);
    let mut session = Session::load(&launcher_registry(), &scenario, options)
        .unwrap_or_else(|e| panic!("load: {e}"));
    let count = |what: &str| {
        log.0
            .lock()
            .expect("log lock")
            .iter()
            .filter(|m| *m == what)
            .count()
    };
    let step = |session: &mut Session| {
        session
            .engine_mut()
            .step(1, |_| core::ops::ControlFlow::Continue(()))
            .expect("step");
    };
    assert_eq!(count("create_instance"), 0, "the carrier is no plugin");
    session.engine_mut().resume();

    step(&mut session);
    let counters = session.engine().lifecycle_counters();
    assert_eq!((counters.spawned, counters.rejected), (1, 0));
    assert_eq!(
        count("create_instance"),
        1,
        "built in the scope at the spawn"
    );
    assert_eq!(count("destroy_instance"), 0);

    step(&mut session);
    assert_eq!(
        count("destroy_instance"),
        0,
        "the child lives through its first tick"
    );
    step(&mut session);
    assert_eq!(session.engine().lifecycle_counters().despawned, 1);
    assert_eq!(count("destroy_instance"), 1, "destroyed at the despawn");
}

/// ADR 0052, ASM-688: the directory a stream frame carries follows spawns and despawns too (it is
/// cached between frames and rebuilt when the engine's directory changes): in every frame the
/// directory holds exactly the Owned entities of the snapshot.
#[test]
fn given_a_subscription_when_entities_are_spawned_and_removed_then_every_frame_directory_matches_its_snapshot()
 {
    use umbra_engine::bus::SubscribeOptions;

    let mut config = HandleConfig::new(launcher_registry());
    config.wall = std::sync::Arc::new(|| -> Box<dyn umbra_engine::WallClock> {
        Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            core::time::Duration::from_micros(500),
        ))
    });
    let (handle, _thread) = EngineHandle::spawn(config).expect("spawn the engine thread");
    let mut options = SubscribeOptions::new();
    options.capacity = core::num::NonZeroUsize::new(100_000).expect("non-zero");
    let sub = handle.subscribe(options);
    handle
        .load(fixture("spawn_selfish"), None, Some(ClockMode::Stepped))
        .expect("load");
    handle.start().expect("start");

    handle.step(4).expect("step");

    let mut frames = Vec::new();
    while frames
        .last()
        .and_then(|f: &std::sync::Arc<umbra_engine::bus::Frame>| f.snapshot.as_ref())
        .is_none_or(|s| s.time.tick() < 4)
    {
        let drained = sub.wait(core::time::Duration::from_secs(30)).frames;
        assert!(!drained.is_empty(), "timed out waiting for frames");
        frames.extend(drained);
    }
    let mut sizes = Vec::new();
    for frame in &frames {
        let Some(snapshot) = &frame.snapshot else {
            continue;
        };
        let owned: Vec<_> = snapshot
            .entities
            .iter()
            .filter(|e| e.ownership.is_owned())
            .map(|e| e.id)
            .collect();
        let known: Vec<_> = frame.directory.keys().copied().collect();
        assert_eq!(known, owned, "tick {}", snapshot.time.tick());
        sizes.push(owned.len());
    }
    assert!(sizes.contains(&2), "a frame saw the child: {sizes:?}");
    assert_eq!(sizes.last(), Some(&1), "and the last one saw it gone");
}
