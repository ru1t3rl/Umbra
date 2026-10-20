//! AT-4 (engine handle, M5 WI-9): spawn and remove publish their events and refresh the entity
//! directory, entity filters and paging bounds, and the step edge cases.
//! Requirements: REQ-X-1, REQ-X-5, ADR 0024, ADR 0026 (M10).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;

use handle_support::guarded_step;

use handle_support::{
    capture, engine, load_stepped, long_skeleton, receive, short_skeleton, state_of,
    walking_skeleton,
};
use umbra_core::{Geodetic, Position, SimEvent, TemplateId};
use umbra_engine::bus::{FrameCause, RunState, SubscribeOptions};
use umbra_engine::handle::{
    Command, EntityQuery, GeoBox, HandleError, OwnershipFilter, StepOutcome,
};
use umbra_engine::{ComponentKind, SessionError, TemplateSpawn};

fn spawn_spec(name: &str) -> TemplateSpawn {
    let at = Geodetic::from_degrees(52.001, 4.002, 0.0).expect("valid position");
    let id = TemplateId::new("ground_vehicle.base").expect("a template id");
    let mut spec = TemplateSpawn::new(id, Position::Geodetic(at));
    spec.name = Some(name.to_owned());
    spec
}

#[test]
fn spawn_and_remove_publish_their_event_and_refresh_the_directory() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let sub = handle.subscribe(SubscribeOptions::new());
    handle.start().expect("start");
    drop(sub.drain());

    let spawned = handle.spawn_entity(spawn_spec("extra")).expect("spawn");
    let id = spawned.snapshot.id;
    let created = sub.drain().frames;
    handle.remove_entity(id).expect("remove");
    let removed = sub.drain().frames;

    let frame = created.last().expect("a frame before the reply");
    assert_eq!(frame.cause, FrameCause::Control);
    assert!(frame.directory.contains_key(&id));
    assert!(
        frame
            .events
            .iter()
            .any(|e| matches!(e, SimEvent::EntityCreated { entity, .. } if *entity == id))
    );
    let frame = removed.last().expect("a frame before the reply");
    assert!(!frame.directory.contains_key(&id));
    assert!(
        frame
            .events
            .iter()
            .any(|e| matches!(e, SimEvent::EntityRemoved { entity } if *entity == id))
    );
    assert!(matches!(
        handle.entity(id),
        Err(HandleError::Session(SessionError::EntityNotFound(_)))
    ));
}

#[test]
fn filters_narrow_items_and_total() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let all = handle.entities(EntityQuery::default()).expect("all");
    let first = &all.items[0];
    let g = first.snapshot.geodetic();
    let (lat, lon) = (g.lat().to_degrees(), g.lon().to_degrees());
    let total = |edit: &dyn Fn(&mut EntityQuery)| {
        let mut query = EntityQuery::default();
        edit(&mut query);
        handle.entities(query).expect("page").total
    };
    let near = GeoBox {
        min_lat_deg: lat - 1e-6,
        min_lon_deg: lon - 1e-6,
        max_lat_deg: lat + 1e-6,
        max_lon_deg: lon + 1e-6,
    };
    let across = GeoBox {
        min_lat_deg: -90.0,
        min_lon_deg: 170.0,
        max_lat_deg: 90.0,
        max_lon_deg: -170.0,
    };

    assert_eq!(all.total, 3);
    assert_eq!(total(&|q| q.ids = vec![first.snapshot.id]), 1);
    assert_eq!(total(&|q| q.bbox = Some(near)), 1);
    assert_eq!(
        total(&|q| q.bbox = Some(across)),
        0,
        "lon 4 is not near 180"
    );
    assert_eq!(total(&|q| q.ownership = Some(OwnershipFilter::Owned)), 3);
    assert_eq!(total(&|q| q.ownership = Some(OwnershipFilter::Remote)), 0);
    assert_eq!(
        total(&|q| q.template = Some(first.info.template.as_str().to_owned())),
        all.items
            .iter()
            .filter(|e| e.info.template == first.info.template)
            .count()
    );
    assert_eq!(total(&|q| q.template = Some("no.such".to_owned())), 0);
}

#[test]
fn a_limit_of_zero_is_held_to_one_and_a_huge_one_to_the_maximum() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let mut query = EntityQuery::default();
    query.limit = 0;

    let one = handle.entities(query.clone()).expect("page");
    query.limit = usize::MAX;
    let many = handle.entities(query).expect("page");

    assert_eq!(one.items.len(), 1);
    assert_eq!(one.next_after_id, Some(one.items[0].snapshot.id));
    assert_eq!(many.items.len(), 3);
}

#[test]
fn a_step_that_ends_on_the_duration_tick_is_completed_and_then_the_run_stops() {
    let (handle, _thread) = engine();
    let info = handle
        .load(
            short_skeleton(),
            None,
            Some(umbra_engine::ClockMode::Stepped),
        )
        .expect("load");
    handle.start().expect("start");

    let outcome = guarded_step(&handle, info.duration_ticks).expect("step");

    assert!(outcome.completed);
    assert_eq!(outcome.reached.tick(), info.duration_ticks);
    assert_eq!(state_of(&handle), RunState::Stopped);
}

#[test]
fn a_step_of_no_ticks_completes_at_once() {
    let (handle, _thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");

    let outcome = guarded_step(&handle, 0).expect("step");

    assert!(outcome.completed);
    assert_eq!((outcome.applied_at.tick(), outcome.reached.tick()), (0, 0));
    assert!(!outcome.status.step_pending);
}

#[test]
fn a_zero_step_beside_a_pending_step_leaves_the_pending_one() {
    let (handle, _thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");
    let (reply, long) = capture::<StepOutcome>();
    handle
        .submit(Command::Step {
            count: 10_000_000,
            reply,
        })
        .expect("submit");

    let refused = guarded_step(&handle, 0);
    let pending = handle.status().expect("status").run.step_pending;
    handle.unload().expect("unload");

    assert!(matches!(refused, Err(HandleError::StepInProgress)));
    assert!(pending);
    assert!(matches!(
        receive(&long),
        Err(HandleError::StepCancelled { .. })
    ));
}

#[test]
fn pausing_drops_the_queued_ticks_so_resume_does_not_run_them() {
    let (handle, _thread) = engine();
    load_stepped(&handle, long_skeleton());
    handle.start().expect("start");
    let (reply, step) = capture::<StepOutcome>();
    handle
        .submit(Command::Step {
            count: 10_000_000,
            reply,
        })
        .expect("submit");

    handle.pause().expect("pause");
    let Err(HandleError::StepCancelled { reached }) = receive(&step) else {
        panic!("the step is cancelled");
    };
    handle.resume().expect("resume");
    let time = handle.status().expect("status").run.time;

    assert_eq!(time, Some(reached));
}

#[test]
fn components_are_listed_without_a_scenario_and_filter_by_kind() {
    let (handle, _thread) = engine();

    let all = handle.components(None).expect("components");
    let motion = handle
        .components(Some(ComponentKind::MotionModel))
        .expect("components");

    assert!(motion.len() < all.len());
    assert_ne!(motion.len(), 0);
    assert!(motion.iter().all(|e| e.kind == ComponentKind::MotionModel));
    assert!(
        motion
            .iter()
            .any(|e| e.id.as_str() == "builtin.ground.kinematic")
    );
    assert_ne!(handle.catalog().expect("catalog").entries().len(), 0);
}

#[test]
fn spawn_in_the_unknown_template_leaves_the_directory_alone() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let mut spec = spawn_spec("ghost");
    spec.template = TemplateId::new("no.such_template").expect("an id");

    let refused = handle.spawn_entity(spec);

    assert!(matches!(
        refused,
        Err(HandleError::Session(SessionError::TemplateNotFound(_)))
    ));
    assert_eq!(
        handle.entities(EntityQuery::default()).expect("page").total,
        3
    );
}

/// The `Fire` events of the first `ticks` ticks of `patrol_and_engage`, with their ticks.
fn reference_fires(ticks: u64) -> Vec<(u64, SimEvent)> {
    let registry = umbra_engine::builtins::standard_registry().expect("registry");
    let scenario = umbra_engine::scenario::load_scenario(
        &registry,
        &handle_support::example("patrol_and_engage"),
    )
    .expect("load");
    let options = umbra_engine::scenario::RunOptions {
        wall: Box::new(umbra_engine::ManualWallClock::with_auto_advance(
            std::time::Duration::from_micros(500),
        )),
        seed: None,
        clock: None,
    };
    let mut reference = umbra_engine::scenario::instantiate(&registry, &scenario, options)
        .expect("instantiate")
        .engine;
    let mut fires = Vec::new();
    reference
        .run(umbra_engine::RunUntil::Ticks(ticks), |view| {
            let tick = view.report.time.tick();
            fires.extend(
                view.report
                    .events
                    .iter()
                    .filter(|e| matches!(e, SimEvent::Fire { .. }))
                    .map(|e| (tick, e.clone())),
            );
            std::ops::ControlFlow::Continue(())
        })
        .expect("run");
    fires
}

#[test]
fn a_strided_subscriber_gets_the_events_of_ticks_that_built_no_frame() {
    const TICKS: u64 = 1131;
    const STRIDE: u64 = 3;
    let reference = reference_fires(TICKS);
    assert!(
        reference.iter().any(|(tick, _)| tick % STRIDE != 0),
        "a Fire on a tick the stride skips: {:?}",
        reference.iter().map(|(t, _)| t).collect::<Vec<_>>()
    );
    let (handle, _thread) = engine();
    load_stepped(
        &handle,
        handle_support::examples().join("patrol_and_engage/scenario.toml"),
    );
    let mut options = SubscribeOptions::new();
    options.min_tick_stride = std::num::NonZeroU64::new(STRIDE).expect("non-zero");
    options.capacity = std::num::NonZeroUsize::new(100_000).expect("non-zero");
    let sub = handle.subscribe(options);
    handle.start().expect("start");

    handle_support::guarded_step(&handle, TICKS).expect("step");

    let drained = sub.drain();
    let got: Vec<SimEvent> = drained
        .carried_events
        .iter()
        .chain(drained.frames.iter().flat_map(|f| f.events.iter()))
        .filter(|e| matches!(e, SimEvent::Fire { .. }))
        .cloned()
        .collect();
    let want: Vec<SimEvent> = reference.into_iter().map(|(_, e)| e).collect();
    assert_eq!(drained.dropped, 0);
    assert_eq!(got, want);
}
