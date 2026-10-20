//! AT-3 (snapshot bus, M5 WI-7): bounded per-subscriber queues that drop the oldest frame and never
//! make the publisher wait, event carry-over, strides, priming and the lock rules. Requirements:
//! REQ-K-6, REQ-X-7, ADR 0024 (design-review amendments M1, M2, M11, m1).
//!
//! `no_frame_built_without_subscribers` goes through the engine handle and `step` (WI-9). Events are tagged with a unique
//! number carried in `ControlAction::Step(n)` so tests can follow them across drops.
//!
//! "Due" (ASM-174): a subscription with stride `s` is due a tick frame at tick `t` when `t % s == 0`;
//! stride 1 is due at every tick.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod handle_support;

use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use umbra_engine::bus::{
    Bus, Drained, Frame, FrameCause, RunState, RunStatus, SubscribeOptions, Subscription,
};
use umbra_engine::{ClockMode, ControlAction, SimEvent};

use handle_support::{GUARD, config, finishes, release, short_skeleton, start_engine};

fn status() -> RunStatus {
    RunStatus::new(RunState::Running, ClockMode::Stepped, 1)
}

fn event(tag: u64) -> SimEvent {
    SimEvent::SimulationControl(ControlAction::Step(tag))
}

fn tag_of(event: &SimEvent) -> u64 {
    match event {
        SimEvent::SimulationControl(ControlAction::Step(tag)) => *tag,
        other => panic!("not a tagged event: {other:?}"),
    }
}

fn tags(events: &[SimEvent]) -> Vec<u64> {
    events.iter().map(tag_of).collect()
}

/// A tick frame with number `seq` carrying events tagged `events`.
fn frame(seq: u64, events: &[u64]) -> Frame {
    let mut frame = Frame::new(seq, FrameCause::Tick, status());
    frame.events = events.iter().copied().map(event).collect();
    frame
}

fn options(capacity: usize) -> SubscribeOptions {
    let mut options = SubscribeOptions::new();
    options.capacity = NonZeroUsize::new(capacity).expect("non-zero");
    options
}

fn strided(stride: u64, with_hash: bool) -> SubscribeOptions {
    let mut options = SubscribeOptions::new();
    options.min_tick_stride = NonZeroU64::new(stride).expect("non-zero");
    options.with_state_hash = with_hash;
    options
}

fn seqs(drained: &Drained) -> Vec<u64> {
    drained.frames.iter().map(|f| f.seq).collect()
}

fn new_bus() -> Arc<Bus> {
    Arc::new(Bus::new())
}

// ---- never blocking, bounded, newest kept ----

#[test]
fn publish_never_waits_for_a_full_subscriber() {
    let bus = new_bus();
    let sub = bus.subscribe(options(2));
    let publisher = Arc::clone(&bus);

    finishes(move || {
        for seq in 0..1000 {
            publisher.publish(frame(seq, &[]), None);
        }
    });

    let stats = sub.stats();
    assert_eq!(stats.pushed_total, 1000);
    assert_eq!(stats.dropped_total, 998);
}

#[test]
fn queue_never_exceeds_capacity() {
    let bus = new_bus();
    let sub = bus.subscribe(options(3));

    for seq in 0..50 {
        bus.publish(frame(seq, &[]), None);
    }

    assert!(sub.stats().max_queue_len <= 3, "{:?}", sub.stats());
    assert_eq!(sub.drain().frames.len(), 3);
}

#[test]
fn newest_frame_is_always_kept() {
    let bus = new_bus();
    let sub = bus.subscribe(options(4));

    for seq in 0..10 {
        bus.publish(frame(seq, &[]), None);
    }

    assert_eq!(
        seqs(&sub.drain()),
        vec![6, 7, 8, 9],
        "oldest first, newest kept"
    );
}

#[test]
fn dropped_count_is_exact() {
    let bus = new_bus();
    let sub = bus.subscribe(options(4));
    for seq in 0..10 {
        bus.publish(frame(seq, &[]), None);
    }

    let first = sub.drain();
    let second = sub.drain();
    for seq in 10..15 {
        bus.publish(frame(seq, &[]), None);
    }
    let third = sub.drain();

    assert_eq!(first.dropped, 6, "10 pushed into 4 slots");
    assert_eq!(second.dropped, 0, "the count restarts after a drain");
    assert_eq!(third.dropped, 1, "5 pushed into 4 slots");
    assert_eq!(sub.stats().dropped_total, 7);
}

#[test]
fn drain_of_an_empty_queue_is_empty() {
    // extra: an untouched subscription hands out nothing and reports no drops.
    let bus = new_bus();
    let sub = bus.subscribe(options(4));

    let drained = sub.drain();

    assert!(drained.frames.is_empty() && drained.carried_events.is_empty());
    assert_eq!(drained.dropped, 0);
    assert!(!drained.events_truncated);
}

#[test]
fn frames_pushed_before_a_subscription_existed_are_not_delivered() {
    // extra: `Bus::subscribe` promises that older frames are not delivered.
    let bus = new_bus();
    bus.publish(frame(0, &[1]), None);
    let sub = bus.subscribe(options(4));
    bus.publish(frame(1, &[2]), None);

    let drained = sub.drain();

    assert_eq!(seqs(&drained), vec![1]);
}

#[test]
fn every_subscription_gets_every_frame_independently() {
    // extra: a slow subscriber does not cost a fast one a frame.
    let bus = new_bus();
    let slow = bus.subscribe(options(2));
    let fast = bus.subscribe(options(100));

    for seq in 0..10 {
        bus.publish(frame(seq, &[]), None);
        drop(fast.drain());
    }
    bus.publish(frame(10, &[]), None);

    assert_eq!(seqs(&fast.drain()), vec![10]);
    assert_eq!(seqs(&slow.drain()), vec![9, 10]);
    assert_eq!(fast.stats().dropped_total, 0);
}

// ---- carried events (M2) ----

#[test]
fn dropped_frames_events_are_carried_into_the_next_drain() {
    let bus = new_bus();
    let sub = bus.subscribe(options(2));
    for seq in 0..5 {
        bus.publish(frame(seq, &[seq * 10, seq * 10 + 1]), None);
    }

    let drained = sub.drain();

    assert_eq!(seqs(&drained), vec![3, 4]);
    assert_eq!(
        tags(&drained.carried_events),
        vec![0, 1, 10, 11, 20, 21],
        "the events of frames 0..=2, oldest first"
    );
    assert!(!drained.events_truncated);
    let own: Vec<u64> = drained
        .frames
        .iter()
        .flat_map(|f| tags(&f.events))
        .collect();
    assert_eq!(
        own,
        vec![30, 31, 40, 41],
        "the kept frames keep their own events"
    );
}

#[test]
fn carried_events_are_handed_out_once() {
    // extra: a second drain does not repeat them.
    let bus = new_bus();
    let sub = bus.subscribe(options(1));
    for seq in 0..3 {
        bus.publish(frame(seq, &[seq]), None);
    }
    assert_eq!(tags(&sub.drain().carried_events), vec![0, 1]);

    let again = sub.drain();

    assert!(
        again.carried_events.is_empty(),
        "{:?}",
        again.carried_events
    );
    assert!(!again.events_truncated);
}

#[test]
fn carried_events_cap_sets_truncated() {
    let bus = new_bus();
    let mut capped = options(1);
    capped.max_carry_events = 3;
    let sub = bus.subscribe(capped);
    for seq in 0..5 {
        bus.publish(frame(seq, &[seq * 2, seq * 2 + 1]), None);
    }

    let drained = sub.drain();

    assert_eq!(drained.dropped, 4);
    assert!(
        drained.carried_events.len() <= 3,
        "8 events were dropped, at most 3 kept: {:?}",
        drained.carried_events
    );
    assert!(drained.events_truncated);
}

#[test]
fn carried_events_exactly_at_the_cap_are_not_truncated() {
    // extra: the cap is a limit, not a trigger.
    let bus = new_bus();
    let mut capped = options(1);
    capped.max_carry_events = 3;
    let sub = bus.subscribe(capped);
    bus.publish(frame(0, &[0, 1, 2]), None);
    bus.publish(frame(1, &[]), None);

    let drained = sub.drain();

    assert_eq!(tags(&drained.carried_events), vec![0, 1, 2]);
    assert!(!drained.events_truncated);
}

#[test]
fn truncation_is_reported_once_per_drain() {
    // extra: the flag belongs to one drain, like `dropped`.
    let bus = new_bus();
    let mut capped = options(1);
    capped.max_carry_events = 1;
    let sub = bus.subscribe(capped);
    for seq in 0..3 {
        bus.publish(frame(seq, &[seq * 2, seq * 2 + 1]), None);
    }
    assert!(sub.drain().events_truncated);

    assert!(!sub.drain().events_truncated);
}

// ---- registration and notification ----

#[test]
fn dropped_subscription_unregisters() {
    let bus = new_bus();
    {
        let _sub = bus.subscribe(options(2));
        assert!(
            bus.wants_frame(1),
            "a subscription is due every tick at stride 1"
        );
    }

    assert!(!bus.wants_frame(1));
    bus.publish(frame(0, &[]), None);
}

fn counting(options: &mut SubscribeOptions) -> Arc<AtomicU64> {
    let calls = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&calls);
    options.notify = Some(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    calls
}

#[test]
fn notify_called_once_per_publish() {
    let bus = new_bus();
    let mut watched = options(2);
    let calls = counting(&mut watched);
    let _sub = bus.subscribe(watched);

    for seq in 0..5 {
        bus.publish(frame(seq, &[]), None);
        assert_eq!(calls.load(Ordering::SeqCst), seq + 1);
    }
}

#[test]
fn notify_is_not_called_for_a_frame_aimed_at_another_slot() {
    // extra: priming one slot does not wake the others.
    let bus = new_bus();
    let mut watched = options(2);
    let calls = counting(&mut watched);
    let _watcher = bus.subscribe(watched);
    let target = bus.subscribe(options(2));

    bus.publish(frame(0, &[]), Some(target.id()));

    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn no_lock_held_during_notify() {
    let bus = new_bus();
    let kept: Arc<Mutex<Vec<Subscription>>> = Arc::new(Mutex::new(Vec::new()));
    let weak: Weak<Bus> = Arc::downgrade(&bus);
    let again = Arc::clone(&kept);
    let mut options_ = options(2);
    options_.notify = Some(Arc::new(move || {
        // m1: the notify may use the bus (here: subscribe again and ask) without deadlocking.
        if let Some(bus) = weak.upgrade() {
            let sub = bus.subscribe(SubscribeOptions::new());
            let _due = bus.wants_frame(1);
            again.lock().expect("kept lock").push(sub);
        }
    }));
    let first = bus.subscribe(options_);
    let publisher = Arc::clone(&bus);

    finishes(move || publisher.publish(frame(0, &[]), None));

    assert_eq!(
        kept.lock().expect("kept lock").len(),
        1,
        "the notify ran once"
    );
    assert_eq!(first.drain().frames.len(), 1);
}

#[test]
fn notify_may_drain_its_own_subscription() {
    // m1: the slot mutex is released before `notify`, so a notify that drains (as the server's
    // consumer wake-up may) does not deadlock.
    let bus = new_bus();
    let cell: Arc<Mutex<Option<Arc<Subscription>>>> = Arc::new(Mutex::new(None));
    let seen = Arc::new(AtomicU64::new(0));
    let (cell_in, seen_in) = (Arc::clone(&cell), Arc::clone(&seen));
    let mut watched = options(2);
    watched.notify = Some(Arc::new(move || {
        if let Some(sub) = cell_in.lock().expect("cell lock").as_ref() {
            seen_in.fetch_add(sub.drain().frames.len() as u64, Ordering::SeqCst);
        }
    }));
    *cell.lock().expect("cell lock") = Some(Arc::new(bus.subscribe(watched)));
    let publisher = Arc::clone(&bus);

    finishes(move || publisher.publish(frame(0, &[]), None));

    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the notify drained the frame"
    );
    // Break the cycle cell -> subscription -> slot -> notify -> cell, so the subscription unregisters.
    cell.lock().expect("cell lock").take();
}

#[test]
fn poisoned_registry_recovers() {
    // m1. A panicking notify runs outside every lock, so this cannot poison the registry itself
    // (the registry's `PoisonError::into_inner` path is reachable only from inside `bus.rs`; see
    // ASM-176). What it does pin from outside: a panic raised by a subscriber's callback while
    // publishing leaves the bus fully usable.
    let bus = new_bus();
    let outcome = {
        let mut hostile = options(2);
        hostile.notify = Some(Arc::new(|| panic!("a hostile notify")));
        let _hostile_sub = bus.subscribe(hostile);
        let publisher = Arc::clone(&bus);
        std::thread::spawn(move || publisher.publish(frame(0, &[]), None)).join()
    };
    assert!(
        outcome.is_err(),
        "the panic of the notify propagated to the publisher"
    );
    let healthy = bus.subscribe(options(2));
    bus.publish(frame(1, &[]), None);

    assert!(bus.wants_frame(1));
    assert_eq!(seqs(&healthy.drain()), vec![1]);
}

// ---- strides and the state hash (M11) ----

#[test]
fn wants_frame_follows_the_smallest_due_stride() {
    let bus = new_bus();
    assert!(!bus.wants_frame(4), "nobody subscribed");
    let due = |bus: &Bus| -> Vec<u64> { (1..=12).filter(|t| bus.wants_frame(*t)).collect() };
    let first = bus.subscribe(strided(4, false));
    let due_with_one = due(&bus);
    let _second = bus.subscribe(strided(6, false));
    let due_with_two = due(&bus);
    release(first);
    let due_without_first = due(&bus);

    assert_eq!(due_with_one, vec![4, 8, 12]);
    assert_eq!(due_with_two, vec![4, 6, 8, 12]);
    assert_eq!(due_without_first, vec![6, 12]);
}

#[test]
fn state_hash_only_when_requested() {
    let bus = new_bus();
    assert!(!bus.wants_state_hash(2), "nobody subscribed");
    let _plain = bus.subscribe(strided(1, false));
    assert!(!bus.wants_state_hash(2), "no subscription asked for it");
    let _hashing = bus.subscribe(strided(2, true));

    let wanted: Vec<u64> = (1..=6).filter(|t| bus.wants_state_hash(*t)).collect();

    assert_eq!(wanted, vec![2, 4, 6], "only when a due subscription asked");
}

#[test]
fn prime_targets_one_slot() {
    let bus = new_bus();
    let mut first_options = options(4);
    let first_calls = counting(&mut first_options);
    let first = bus.subscribe(first_options);
    let second = bus.subscribe(options(4));
    let mut control = Frame::new(7, FrameCause::Control, status());
    control.events = vec![event(1)];

    bus.publish(control, Some(first.id()));

    let got = first.drain();
    assert_eq!(seqs(&got), vec![7]);
    assert_eq!(got.frames[0].cause, FrameCause::Control);
    assert!(
        second.drain().frames.is_empty(),
        "the other slot got nothing"
    );
    assert_eq!(second.stats().pushed_total, 0);
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
}

// ---- no building without subscribers (M11) ----

#[test]
fn no_frame_built_without_subscribers_wants_nothing() {
    // extra (bus half of the row): with no subscription the bus asks for nothing, at any tick.
    let bus = new_bus();

    let asked: Vec<u64> = (0..100).filter(|t| bus.wants_frame(*t)).collect();
    let hashed: Vec<u64> = (0..100).filter(|t| bus.wants_state_hash(*t)).collect();

    assert!(asked.is_empty() && hashed.is_empty());
}

#[test]
fn no_frame_built_without_subscribers() {
    // The handle half of the row: ticks and a step with nobody subscribed build no frame; the first
    // subscriber costs exactly its priming frame.
    let (handle, _thread) = start_engine(config());
    handle_support::load_stepped(&handle, short_skeleton());
    handle.start().expect("start");
    handle_support::guarded_step(&handle, 5).expect("step");
    assert_eq!(handle.snapshot_builds(), 0);

    let sub = handle.subscribe(SubscribeOptions::new());
    let primed = sub.wait(GUARD);

    assert_eq!(primed.frames.len(), 1);
    assert_eq!(handle.snapshot_builds(), 1);
}

#[test]
fn no_frame_built_without_subscribers_while_running() {
    // The handle half of the row without `step` (WI-8): a run to its duration with nobody
    // subscribed builds no frame; the first subscriber then costs exactly its priming frame.
    let (handle, _thread) = start_engine(config());
    handle
        .load(short_skeleton(), None, Some(ClockMode::AsFastAsPossible))
        .expect("load");
    handle.start().expect("start");
    while handle.status().expect("status").run.state != RunState::Stopped {
        std::thread::yield_now();
    }
    assert_eq!(handle.snapshot_builds(), 0);

    let sub = handle.subscribe(SubscribeOptions::new());
    let primed = sub.wait(GUARD);

    assert_eq!(primed.frames.len(), 1);
    assert_eq!(handle.snapshot_builds(), 1);
}

#[test]
fn due_and_interest_agree_with_the_single_questions() {
    // ASM-174 / WI-8: `due` is `(wants_frame, wants_state_hash)` from one registry read, and
    // `interest` is `(any subscription, any asks for hashes)` whatever the stride.
    let bus = new_bus();
    assert_eq!(bus.interest(), (false, false));
    assert_eq!(bus.due(4), (false, false));
    let plain = bus.subscribe(SubscribeOptions::new());
    assert_eq!(bus.interest(), (true, false));
    let mut strided = SubscribeOptions::new();
    strided.min_tick_stride = NonZeroU64::new(3).expect("non-zero");
    strided.with_state_hash = true;
    let hashed = bus.subscribe(strided);
    assert_eq!(bus.interest(), (true, true));

    for tick in 0..12 {
        assert_eq!(
            bus.due(tick),
            (bus.wants_frame(tick), bus.wants_state_hash(tick)),
            "tick {tick}"
        );
    }
    assert_eq!(bus.due(1), (true, false), "only the plain one is due");
    assert_eq!(bus.due(3), (true, true));
    drop(plain);
    drop(hashed);
    assert_eq!(bus.interest(), (false, false));
}

// ---- the invariants over random sequences ----

#[derive(Debug, Clone)]
enum Op {
    Publish(u8),
    Drain,
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(
        prop_oneof![3 => (0_u8..4).prop_map(Op::Publish), 1 => Just(Op::Drain)],
        1..80,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    /// The capacity bound, the kept newest frame, the exact drop count and the conservation of
    /// events (up to the carry cap) hold for any publish and drain sequence.
    #[test]
        fn random_publish_and_drain_sequences_keep_the_invariants(
        capacity in 1_usize..6,
        carry_cap in prop_oneof![Just(4096_usize), 0_usize..8],
        script in ops(),
    ) {
        let bus = new_bus();
        let mut wanted = options(capacity);
        wanted.max_carry_events = carry_cap;
        let sub = bus.subscribe(wanted);
        let (mut next_seq, mut next_tag) = (0_u64, 0_u64);
        let mut published: Vec<(u64, Vec<u64>)> = Vec::new();

        let check = |sub: &Subscription, published: &mut Vec<(u64, Vec<u64>)>| -> Result<(), TestCaseError> {
            let drained = sub.drain();
            prop_assert!(drained.frames.len() <= capacity);
            prop_assert!(sub.stats().max_queue_len <= capacity);
            let kept = published.len().min(capacity);
            prop_assert_eq!(drained.frames.len(), kept);
            prop_assert_eq!(drained.dropped, (published.len() - kept) as u64);
            if let Some((last, _)) = published.last() {
                prop_assert_eq!(drained.frames.last().map(|f| f.seq), Some(*last), "newest kept");
            }
            let got: Vec<u64> = tags(&drained.carried_events)
                .into_iter()
                .chain(drained.frames.iter().flat_map(|f| tags(&f.events)))
                .collect();
            let sent: Vec<u64> = published.iter().flat_map(|(_, e)| e.clone()).collect();
            if drained.events_truncated {
                prop_assert!(drained.carried_events.len() <= carry_cap);
                prop_assert!(got.len() < sent.len());
            } else {
                prop_assert_eq!(got, sent, "no event lost below the cap, order kept");
            }
            published.clear();
            Ok(())
        };

        for op in script {
            match op {
                Op::Publish(count) => {
                    let events: Vec<u64> = (0..u64::from(count)).map(|i| next_tag + i).collect();
                    next_tag += u64::from(count);
                    bus.publish(frame(next_seq, &events), None);
                    published.push((next_seq, events));
                    next_seq += 1;
                }
                Op::Drain => check(&sub, &mut published)?,
            }
        }
        check(&sub, &mut published)?;
    }
}
