//! The network link under the engine handle (M6 WI-9): run control reaches the adapters in the
//! order the commands were issued (ASM-295), and ghosts show in entity queries and frames
//! (ASM-400).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;
mod net_support;

use handle_support::{frozen_config, guarded_step, start_engine, until_stopped};
use net_support::{Peer, fixture, registry_with, start};
use umbra_core::ControlAction;
use umbra_core::net::NetMessage;
use umbra_engine::bus::SubscribeOptions;
use umbra_engine::handle::{EngineHandle, EngineThread, EntityQuery, HandleError, OwnershipFilter};
use umbra_engine::{ClockMode, Ownership, SessionError, TemplateId, TemplateSpawn};
use umbra_net::LoopbackHub;

fn handle_over(hub: &std::sync::Arc<LoopbackHub>, dir: &str) -> (EngineHandle, EngineThread) {
    let mut config = frozen_config();
    config.base_registry = registry_with(&[("test.net.loopback", hub, 1000)]).clone();
    let (handle, thread) = start_engine(config);
    handle
        .load(fixture(dir), None, Some(ClockMode::Stepped))
        .expect("load");
    (handle, thread)
}

fn controls(hub: &LoopbackHub) -> Vec<ControlAction> {
    hub.captured()
        .into_iter()
        .filter_map(|(_, m)| match m {
            NetMessage::SimulationControl(a) => Some(a),
            _ => None,
        })
        .collect()
}

#[test]
fn commands_reach_the_adapters_in_the_order_they_were_issued() {
    let hub = LoopbackHub::with_capture(4096);
    let (handle, _thread) = handle_over(&hub, "control");

    handle.start().expect("start");
    guarded_step(&handle, 3).expect("step");
    handle.pause().expect("pause");
    handle.resume().expect("resume");
    handle.stop().expect("stop");

    assert_eq!(
        controls(&hub),
        vec![
            ControlAction::Start,
            ControlAction::Step(3),
            ControlAction::Pause,
            ControlAction::Resume,
            ControlAction::Stop
        ]
    );
}

#[test]
fn reaching_the_duration_publishes_exactly_one_stop() {
    let hub = LoopbackHub::with_capture(4096);
    let (handle, _thread) = handle_over(&hub, "control_short");
    let sub = handle.subscribe(SubscribeOptions::new());
    handle.start().expect("start");

    // The duration is 6 ticks; the step runs into it.
    let _ = guarded_step(&handle, 10);
    drop(until_stopped(&sub));

    assert_eq!(
        controls(&hub),
        vec![
            ControlAction::Start,
            ControlAction::Step(10),
            ControlAction::Stop
        ]
    );
}

/// A handle over engine A with a peer B ticked on the test thread.
fn handle_with_ghost() -> (EngineHandle, EngineThread, Peer) {
    let hub = LoopbackHub::new(4096);
    let (handle, thread) = handle_over(&hub, "a");
    let (mut b, _) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub, 2000)]),
        None,
    ));
    handle.start().expect("start");
    b.ticks(5);
    guarded_step(&handle, 5).expect("step");
    (handle, thread, b)
}

#[test]
fn ghosts_are_listed_with_template_and_peer_name() {
    let (handle, _thread, _b) = handle_with_ghost();
    let mut query = EntityQuery::default();
    query.ownership = Some(OwnershipFilter::Remote);

    let page = handle.entities(query).expect("entities");

    assert_eq!(page.items.len(), 1, "{:?}", page.items);
    let ghost = &page.items[0];
    assert_eq!(ghost.info.template.as_str(), "tank.mbt_generic");
    assert_eq!(ghost.info.name.as_deref(), Some("b-tank"));
    assert_eq!(ghost.info.group, None);
    assert!(matches!(ghost.snapshot.ownership, Ownership::Remote { .. }));
    let one = handle.entity(ghost.snapshot.id).expect("entity");
    assert_eq!(one.info, ghost.info);
}

#[test]
fn a_stream_frame_carries_the_ghost() {
    let (handle, _thread, mut b) = handle_with_ghost();
    let sub = handle.subscribe(SubscribeOptions::new());
    b.tick();

    guarded_step(&handle, 1).expect("step");

    let frames = sub.drain().frames;
    let frame = frames.last().expect("a frame");
    let ghost = frame
        .snapshot
        .as_ref()
        .expect("snapshot")
        .entities
        .iter()
        .find(|e| !e.ownership.is_owned())
        .expect("the ghost is in the snapshot")
        .id;
    assert!(frame.directory.contains_key(&ghost), "directory entry");
}

#[test]
fn a_ghost_cannot_be_removed_and_its_name_does_not_block_a_spawn() {
    let (handle, _thread, _b) = handle_with_ghost();
    let mut query = EntityQuery::default();
    query.ownership = Some(OwnershipFilter::Remote);
    let ghost = handle.entities(query).expect("entities").items[0]
        .snapshot
        .id;

    let removed = handle.remove_entity(ghost);

    assert!(
        matches!(removed, Err(HandleError::Session(SessionError::EntityRemote(id))) if id == ghost),
        "{removed:?}"
    );
    let mut spec = TemplateSpawn::new(
        TemplateId::new("tank.mbt_generic").expect("id"),
        umbra_engine::Position::Geodetic(
            umbra_engine::Geodetic::from_degrees(52.001, 4.002, 0.0).expect("position"),
        ),
    );
    spec.name = Some("b-tank".to_owned());
    handle
        .spawn_entity(spec)
        .expect("a ghost's name is not a duplicate");
}
