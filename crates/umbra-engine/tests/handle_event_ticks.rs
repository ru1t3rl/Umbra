//! Event ticks (M5 WI-15, ASM-205): every event a subscriber receives comes with the tick it
//! happened in, also when its frame was dropped by the bus (carried) or never built (held by the
//! stride). The reference is a plain engine run of `patrol_and_engage`.
//! Requirements: REQ-X-6, ADR 0024 (M2), ADR 0027.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;

use std::num::{NonZeroU64, NonZeroUsize};

use core::time::Duration;

use handle_support::{engine, examples, guarded_step, load_stepped};
use umbra_core::SimEvent;
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{Drained, SubscribeOptions};
use umbra_engine::scenario::{RunOptions, instantiate, load_scenario};
use umbra_engine::{ManualWallClock, RunUntil};

/// The ticks of the `Fire` events of the first 1200 ticks of a plain run.
fn reference_fire_ticks() -> Vec<u64> {
    let registry = standard_registry().expect("registry");
    let scenario =
        load_scenario(&registry, &handle_support::example("patrol_and_engage")).expect("load");
    let options = RunOptions {
        wall: Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
            500,
        ))),
        seed: None,
        clock: None,
    };
    let mut engine = instantiate(&registry, &scenario, options)
        .expect("instantiate")
        .engine;
    let mut ticks = Vec::new();
    engine
        .run(RunUntil::Ticks(1200), |view| {
            let tick = view.report.time.tick();
            ticks.extend(
                view.report
                    .events
                    .iter()
                    .filter(|e| matches!(e, SimEvent::Fire { .. }))
                    .map(|_| tick),
            );
            std::ops::ControlFlow::Continue(())
        })
        .expect("run");
    ticks
}

/// The ticks of the `Fire` events of everything drained, carried events first.
fn fire_ticks(drained: &Drained) -> Vec<u64> {
    assert_eq!(
        drained.carried_events.len(),
        drained.carried_ticks.len(),
        "a tick per carried event"
    );
    let carried = drained
        .carried_events
        .iter()
        .zip(&drained.carried_ticks)
        .map(|(event, tick)| (event, *tick));
    let queued = drained.frames.iter().flat_map(|frame| {
        assert_eq!(
            frame.events.len(),
            frame.event_ticks.len(),
            "a tick per event"
        );
        frame.events.iter().zip(frame.event_ticks.iter().copied())
    });
    carried
        .chain(queued)
        .filter(|(event, _)| matches!(event, SimEvent::Fire { .. }))
        .map(|(_, tick)| tick)
        .collect()
}

fn run_with(options: SubscribeOptions) -> Drained {
    let (handle, _thread) = engine();
    load_stepped(&handle, examples().join("patrol_and_engage/scenario.toml"));
    let sub = handle.subscribe(options);
    handle.start().expect("start");
    guarded_step(&handle, 1200).expect("step");
    sub.drain()
}

#[test]
fn a_carried_event_keeps_the_tick_it_happened_in() {
    let mut options = SubscribeOptions::new();
    options.capacity = NonZeroUsize::MIN;
    let drained = run_with(options);
    assert!(drained.dropped > 0, "the bus dropped frames");
    let want = reference_fire_ticks();
    assert!(
        want.len() > 3,
        "the reference fires several times: {want:?}"
    );
    assert_eq!(fire_ticks(&drained), want);
}

#[test]
fn a_held_event_keeps_the_tick_it_happened_in() {
    // Stride 7: most fire ticks build no frame, so their events are held for the next built one.
    let mut options = SubscribeOptions::new();
    options.min_tick_stride = NonZeroU64::new(7).expect("non-zero");
    options.capacity = NonZeroUsize::new(1000).expect("non-zero");
    let drained = run_with(options);
    let want = reference_fire_ticks();
    assert!(
        want.iter().any(|tick| tick % 7 != 0),
        "some fires fall on ticks that build no frame: {want:?}"
    );
    assert_eq!(fire_ticks(&drained), want);
}

#[test]
fn is_closed_is_false_while_running_and_true_after_the_thread_ends_and_wakes_subscribers() {
    let (handle, thread) = engine();
    let rang = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut options = SubscribeOptions::new();
    let counter = std::sync::Arc::clone(&rang);
    options.notify = Some(std::sync::Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let _sub = handle.subscribe(options);
    assert!(!handle.is_closed());
    // Let the priming frame ring once, so the count below is only the exit's ring.
    for _ in 0..50_000_000_u32 {
        if rang.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        rang.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "priming never rang"
    );
    let before = rang.load(std::sync::atomic::Ordering::SeqCst);
    thread.shutdown().expect("shutdown");
    assert!(handle.is_closed());
    assert!(
        rang.load(std::sync::atomic::Ordering::SeqCst) > before,
        "the exit rings every subscription"
    );
}
