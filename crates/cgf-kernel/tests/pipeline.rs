//! AT-9 (stand-in part) and AT-10 (pipeline part). Requirements: REQ-K-2, REQ-K-5, REQ-K-8,
//! REQ-X-3 (partial), ASM-029, ASM-036.

#![forbid(unsafe_code)]

mod common;

use core::time::Duration;

use cgf_core::{Intent, MetersPerSecond, Ownership, RemoteSource, Vec3};
use cgf_kernel::{ClockMode, EntityInit, ManualWallClock, Pace, Phase};
use common::{Behavior, MS_50, Stub, at, kernel, kernel_with, owned, rate, route};

/// REQ-K-2: the phases run in the spec's fixed order, and that order is the enum's order.
#[test]
fn phase_order_is_spec_order() {
    assert_eq!(
        Phase::ALL,
        [
            Phase::Ingest,
            Phase::Perception,
            Phase::Behavior,
            Phase::Motion,
            Phase::Events,
            Phase::Publish
        ]
    );
    let mut sorted = Phase::ALL;
    sorted.sort();
    assert_eq!(sorted, Phase::ALL);
}

/// REQ-C-1, ASM-036: a tick advances simulation time by exactly one and reports the post-tick time.
#[test]
fn tick_advances_sim_time_by_one() {
    let mut k = kernel(1);
    let start = k.now();

    let report = k.tick();

    assert_eq!(report.map(|r| r.time.tick()), Ok(start.tick() + 1));
    assert_eq!(k.now().tick(), start.tick() + 1);
    assert_eq!(k.now().rate(), rate());
}

/// REQ-K-2: Behavior output reaches Motion in the same tick (one motion call per tick, with the
/// route's intents).
#[test]
fn behavior_intents_reach_motion_in_same_tick() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.teleport", Behavior::Teleport);
    let init = EntityInit {
        route: Some(route(vec![at(100.0), at(200.0)], 7.0)),
        ..owned(at(0.0), &m)
    };
    assert!(k.spawn(init).is_ok());

    assert!(k.tick().is_ok());

    let calls = m.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        !calls[0].is_empty(),
        "intents were emitted before motion ran"
    );
}

/// REQ-K-2 (ASM-029): the route stand-in advances when within the arrival radius, emits
/// `[SetSpeed(cruise), MoveTo(next)]`, and emits `Hold` (no `MoveTo`) once the last waypoint is
/// reached. A teleporting stub model moves the entity onto each destination.
#[test]
fn route_stand_in_advances_within_arrival_radius_and_emits_hold_at_end() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.teleport", Behavior::Teleport);
    let (w0, w1, w2) = (at(0.0), at(100.0), at(200.0));
    // 1 m from the first waypoint: already within the 5 m arrival radius.
    let init = EntityInit {
        route: Some(route(vec![w0, w1, w2], 7.0)),
        ..owned(at(1.0), &m)
    };
    assert!(k.spawn(init).is_ok());
    let next_after_tick = |k: &mut cgf_kernel::Kernel| {
        assert!(k.tick().is_ok());
        k.snapshot().entities[0].route_next
    };

    let after_1 = next_after_tick(&mut k);
    let after_2 = next_after_tick(&mut k);
    let after_3 = next_after_tick(&mut k);
    let after_4 = next_after_tick(&mut k);

    assert_eq!(
        [after_1, after_2, after_3, after_4],
        [Some((1, 3)), Some((2, 3)), Some((2, 3)), Some((2, 3))]
    );
    let calls = m.calls();
    let cruise = Intent::SetSpeed(MetersPerSecond(7.0));
    assert_eq!(
        calls[0],
        [cruise.clone(), Intent::MoveTo { destination: w1 }]
    );
    assert_eq!(calls[1], [cruise, Intent::MoveTo { destination: w2 }]);
    for hold_tick in &calls[2..] {
        assert!(hold_tick.contains(&Intent::Hold), "{hold_tick:?}");
        assert!(
            !hold_tick.iter().any(|i| matches!(i, Intent::MoveTo { .. })),
            "{hold_tick:?}"
        );
    }
}

/// REQ-K-2 (ASM-029): far from the next waypoint the route keeps steering to it.
#[test]
fn route_stand_in_keeps_steering_while_outside_the_arrival_radius() {
    let mut k = kernel(1);
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    let init = EntityInit {
        route: Some(route(vec![at(100.0), at(200.0)], 7.0)),
        ..owned(at(0.0), &m)
    };
    assert!(k.spawn(init).is_ok());

    for _ in 0..3 {
        assert!(k.tick().is_ok());
    }

    assert_eq!(k.snapshot().entities[0].route_next, Some((0, 2)));
    for call in m.calls() {
        assert_eq!(
            call,
            [
                Intent::SetSpeed(MetersPerSecond(7.0)),
                Intent::MoveTo {
                    destination: at(100.0)
                }
            ]
        );
    }
}

/// REQ-K-5: motion and the route stand-in run only for owned entities; a remote entity with a
/// moving model and a route stays exactly as spawned.
#[test]
fn motion_and_route_run_only_for_owned() {
    let mut k = kernel(1);
    let owned_model = Stub::new(
        "test.stub.owned",
        Behavior::Drift(Vec3 {
            x: 0.0,
            y: 30.0,
            z: 0.0,
        }),
    );
    let remote_model = Stub::new(
        "test.stub.remote",
        Behavior::Drift(Vec3 {
            x: 0.0,
            y: 30.0,
            z: 0.0,
        }),
    );
    let with_route = |model: &std::sync::Arc<Stub>, ownership| EntityInit {
        ownership,
        route: Some(route(vec![at(1000.0)], 5.0)),
        ..owned(at(0.0), model)
    };
    assert!(k.spawn(with_route(&owned_model, Ownership::Owned)).is_ok());
    let remote = Ownership::Remote {
        source: RemoteSource::new("dis.test"),
    };
    assert!(k.spawn(with_route(&remote_model, remote)).is_ok());
    let before = k.snapshot();

    for _ in 0..3 {
        assert!(k.tick().is_ok());
    }

    let after = k.snapshot();
    assert_ne!(
        after.entities[0].state, before.entities[0].state,
        "owned entity moved"
    );
    assert_eq!(
        after.entities[1].state, before.entities[1].state,
        "remote entity unchanged"
    );
    assert_eq!(after.entities[1].route_next, before.entities[1].route_next);
    assert_eq!(owned_model.calls().len(), 3);
    assert_eq!(
        remote_model.calls().len(),
        0,
        "motion never ran for the remote entity"
    );
}

/// REQ-K-8: when real-time ticks take longer than their period, the report carries an overrun
/// and all six phase timings, and no tick is skipped.
#[test]
fn overrun_is_reported_with_per_phase_timings() {
    // Every read of the wall clock costs 50 ms, far above the 33 ms period.
    let wall = ManualWallClock::with_auto_advance(MS_50);
    let mut k = kernel_with(1, ClockMode::RealTime, wall);
    let m = Stub::new("test.stub.still", Behavior::Stationary);
    assert!(k.spawn(owned(at(0.0), &m)).is_ok());

    // Drive the kernel the way the engine does: poll, then tick or wait.
    let mut reports = Vec::new();
    while reports.len() < 5 {
        match k.clock_mut().poll() {
            Pace::Tick => reports.extend(k.tick().ok()),
            Pace::WaitUntil(deadline) => k.clock().wait_until(deadline),
            Pace::Idle => break,
        }
    }

    assert_eq!(reports.len(), 5);
    assert_eq!(k.now().tick(), 5, "no tick was skipped");
    for (i, r) in reports.iter().enumerate() {
        assert_eq!(r.time.tick(), u64::try_from(i + 1).unwrap_or(0));
        assert!(r.overrun.is_some(), "tick {i} overran");
        assert_eq!(r.phase_wall.len(), 6);
        assert!(
            r.phase_wall.iter().all(|d| *d > Duration::ZERO),
            "tick {i}: {:?}",
            r.phase_wall
        );
    }
}

/// REQ-K-8: a tick that keeps up reports no overrun.
#[test]
fn tick_on_schedule_reports_no_overrun() {
    let mut k = kernel(1);

    let report = k.tick();

    assert_eq!(report.map(|r| r.overrun), Ok(None));
}
