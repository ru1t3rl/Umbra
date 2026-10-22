//! AT-12 (unit level): the publish policy. Requirements: REQ-N-2, ADR 0032, ASM-231, ASM-239,
//! ASM-240, ASM-269.
//!
//! Written in step 4 of M6, run from WI-6. Every test drives the
//! tracker once per tick at 30 Hz with a hand-made true trajectory.

#![forbid(unsafe_code)]
#![expect(clippy::cast_precision_loss, reason = "tick counts are small")]
#![expect(clippy::cast_possible_truncation, reason = "tick counts are small")]

mod common;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState};
use umbra_core::{EntityId, KinematicState, Quaternion, Radians, Seconds, Vec3, WorldEuler};
use umbra_net::{PublishPolicy, PublishTracker};

use common::{at, eid, publish_policy, tick};

const DT: f64 = 1.0 / 30.0;

fn policy(model: DeadReckoning) -> PublishPolicy {
    PublishPolicy {
        model,
        ..publish_policy()
    }
}

/// Position at tick `n` for a velocity that is `before` for ticks `< turn_at` and `after` from
/// then on: the sum of `velocity(k) * dt` over the earlier ticks.
fn trajectory(n: u64, turn_at: u64, before: Vec3, after: Vec3) -> KinematicState {
    let steps_before = n.min(turn_at);
    let steps_after = n.saturating_sub(turn_at);
    let travelled = |v: Vec3, steps: u64| {
        let t = steps as f64 * DT;
        Vec3::new(v.x * t, v.y * t, v.z * t)
    };
    let (b, a) = (
        travelled(before, steps_before),
        travelled(after, steps_after),
    );
    let velocity = if n < turn_at { before } else { after };
    KinematicState {
        position: at(0.0 + b.y + a.y),
        velocity,
        orientation: Quaternion::IDENTITY,
    }
}

/// The ticks, out of `0..ticks`, at which the tracker asks to send, for states from `state_at`.
fn send_ticks(
    tracker: &mut PublishTracker,
    id: EntityId,
    ticks: u64,
    state_at: impl Fn(u64) -> KinematicState,
) -> Vec<u64> {
    (0..ticks)
        .filter(|&n| tracker.evaluate(tick(n), id, &state_at(n)).is_some())
        .collect()
}

fn still(_: u64) -> KinematicState {
    common::kin(0.0)
}

#[test]
fn first_sight_publishes_the_current_state_as_the_basis() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));
    let state = trajectory(0, 100, Vec3::new(0.0, 5.0, 0.0), Vec3::ZERO);

    let sent = tracker.evaluate(tick(0), eid(1), &state);

    assert_eq!(
        sent.map(|s| (s.state, s.model)),
        Some((state, DeadReckoning::Velocity))
    );
}

#[test]
fn constant_velocity_under_the_velocity_model_publishes_only_at_heartbeats() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));
    let v = Vec3::new(0.0, 10.0, 0.0);

    let sends = send_ticks(&mut tracker, eid(1), 301, |n| trajectory(n, 1_000, v, v));

    assert_eq!(sends, [0, 150, 300], "heartbeat 5 s at 30 Hz is 150 ticks");
}

#[test]
fn a_turn_publishes_on_the_tick_the_position_error_passes_the_threshold() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));
    let (before, after) = (Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -4.0, 0.0));

    let sends = send_ticks(&mut tracker, eid(1), 60, |n| {
        trajectory(n, 30, before, after)
    });

    // From tick 30 the error grows by 14 m/s * dt = 0.467 m per tick: 0.93 m at tick 32, 1.4 m at 33.
    assert_eq!(sends, [0, 33]);
}

#[test]
fn an_orientation_change_alone_publishes_once_it_passes_the_threshold() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));
    // 0.02 rad per tick: 0.04 rad at tick 2 is under 3 degrees (0.0524 rad), 0.06 rad at tick 3 is over.
    let yawing = |n: u64| KinematicState {
        orientation: Quaternion::from_world_euler(WorldEuler {
            yaw: Radians(0.02 * n as f64),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
        ..common::kin(0.0)
    };

    let sends = send_ticks(&mut tracker, eid(1), 5, yawing);

    assert_eq!(sends, [0, 3]);
}

#[test]
fn min_interval_suppresses_updates_that_are_due_sooner() {
    let mut p = policy(DeadReckoning::Static);
    p.min_interval = Seconds(1.0);
    let mut tracker = PublishTracker::new(p);
    let fast = Vec3::new(0.0, 100.0, 0.0);

    let sends = send_ticks(&mut tracker, eid(1), 61, |n| {
        trajectory(n, 1_000, fast, fast)
    });

    assert_eq!(
        sends,
        [0, 30, 60],
        "a static basis is outrun every tick, but at most 1 per second"
    );
}

#[test]
fn the_heartbeat_fires_after_exactly_ceil_heartbeat_times_rate_ticks() {
    // 0.05 s * 30 Hz = 1.5 ticks, so every 2 ticks; 5 s * 30 Hz = 150 ticks.
    for (heartbeat_s, every) in [(0.05, 2_u64), (5.0, 150), (0.1, 3)] {
        let mut p = policy(DeadReckoning::Static);
        p.heartbeat = Seconds(heartbeat_s);
        let mut tracker = PublishTracker::new(p);

        let sends = send_ticks(&mut tracker, eid(1), 301, still);

        let expected: Vec<u64> = (0..=300).step_by(every as usize).collect();
        assert_eq!(sends, expected, "heartbeat {heartbeat_s} s");
    }
}

#[test]
fn removal_is_reported_once_and_the_entity_is_forgotten() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));
    tracker.evaluate(tick(0), eid(1), &still(0));

    let (first, second) = (tracker.removed(eid(1)), tracker.removed(eid(1)));
    let republished = tracker.evaluate(tick(1), eid(1), &still(1));

    assert_eq!((first, second), (true, false));
    assert!(
        republished.is_some(),
        "a forgotten entity is a first sight again"
    );
}

#[test]
fn removing_an_entity_that_was_never_published_reports_nothing() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Velocity));

    assert!(!tracker.removed(eid(7)));
}

#[test]
fn the_acceleration_model_sends_the_two_tick_finite_difference_of_velocity() {
    let mut p = policy(DeadReckoning::Acceleration);
    p.heartbeat = Seconds(0.1); // every 3 ticks, so tick 3 sends
    let mut tracker = PublishTracker::new(p);
    let accelerating = |n: u64| KinematicState {
        velocity: Vec3::new(0.0, 2.0 * n as f64 * DT, 0.0),
        ..common::kin(0.0)
    };

    let sent: Vec<Option<RemoteState>> = (0..4)
        .map(|n| tracker.evaluate(tick(n), eid(1), &accelerating(n)))
        .collect();

    let first = sent[0].expect("first sight sends");
    let heartbeat = sent[3].expect("tick 3 is a heartbeat");
    assert_eq!(first.acceleration, Vec3::ZERO, "no history yet");
    assert!(
        (heartbeat.acceleration.y - 2.0).abs() < 1e-9,
        "{:?}",
        heartbeat.acceleration
    );
    assert_eq!(heartbeat.model, DeadReckoning::Acceleration);
}

#[test]
fn entities_are_tracked_independently() {
    let mut tracker = PublishTracker::new(policy(DeadReckoning::Static));

    let a0 = tracker.evaluate(tick(0), eid(1), &still(0));
    let b0 = tracker.evaluate(tick(0), eid(2), &still(0));
    let a1 = tracker.evaluate(tick(1), eid(1), &still(1));
    let b1 = tracker.evaluate(tick(1), eid(2), &still(1));

    assert_eq!(
        [a0.is_some(), b0.is_some(), a1.is_some(), b1.is_some()],
        [true, true, false, false]
    );
}

#[test]
fn the_acceleration_estimate_spans_two_ticks_not_one() {
    let mut p = policy(DeadReckoning::Acceleration);
    p.heartbeat = Seconds(0.1); // tick 3 sends
    let mut tracker = PublishTracker::new(p);
    // Velocity jumps at the last tick only: (6 - 0) / (2 dt) = 90 m/s^2, a one-tick difference gives 180 (x and z scale the same).
    let jump = |n: u64| KinematicState {
        velocity: if n == 3 {
            Vec3::new(3.0, 6.0, -1.5)
        } else {
            Vec3::ZERO
        },
        ..common::kin(0.0)
    };

    let sent: Vec<Option<RemoteState>> = (0..4)
        .map(|n| tracker.evaluate(tick(n), eid(1), &jump(n)))
        .collect();

    let heartbeat = sent[3].expect("tick 3 is a heartbeat");
    let a = heartbeat.acceleration;
    assert!(
        (a.x - 45.0).abs() < 1e-9 && (a.y - 90.0).abs() < 1e-9 && (a.z + 22.5).abs() < 1e-9,
        "{a:?}"
    );
}

#[test]
fn the_velocity_and_static_models_send_zero_acceleration() {
    for model in [DeadReckoning::Velocity, DeadReckoning::Static] {
        let mut p = policy(model);
        p.heartbeat = Seconds(0.1);
        let mut tracker = PublishTracker::new(p);
        let accelerating = |n: u64| KinematicState {
            velocity: Vec3::new(0.0, 2.0 * n as f64 * DT, 0.0),
            ..common::kin(0.0)
        };

        let sent = (0..4)
            .filter_map(|n| tracker.evaluate(tick(n), eid(1), &accelerating(n)))
            .last()
            .expect("tick 3 is a heartbeat");

        assert_eq!(sent.acceleration, Vec3::ZERO, "{model:?}");
    }
}

/// Velocity `2 n dt` (acceleration 2) at tick `n`, for the history rows.
fn ramp(n: u64) -> KinematicState {
    KinematicState {
        velocity: Vec3::new(0.0, 2.0 * n as f64 * DT, 0.0),
        ..common::kin(0.0)
    }
}

fn accel_policy() -> PublishPolicy {
    let mut p = policy(DeadReckoning::Acceleration);
    p.heartbeat = Seconds(0.1); // 3 ticks
    p.position_threshold_m = 1e9; // the ramp keeps the position fixed: only heartbeats send
    p
}

#[test]
fn a_gap_in_ticks_restarts_the_acceleration_history() {
    let mut tracker = PublishTracker::new(accel_policy());
    for n in 0..3 {
        tracker.evaluate(tick(n), eid(1), &ramp(n));
    }

    // 1000 ticks later the heartbeat is due; the old samples are not two ticks back, so zero.
    let after_gap = tracker
        .evaluate(tick(1002), eid(1), &ramp(1002))
        .expect("heartbeat after a gap");
    assert_eq!(after_gap.acceleration, Vec3::ZERO);
    for n in 1003..1005 {
        assert!(tracker.evaluate(tick(n), eid(1), &ramp(n)).is_none());
    }
    // Three consecutive samples again: the estimate is back.
    let again = tracker
        .evaluate(tick(1005), eid(1), &ramp(1005))
        .expect("heartbeat");
    assert!(
        (again.acceleration.y - 2.0).abs() < 1e-6,
        "{:?}",
        again.acceleration
    );
}

#[test]
fn a_clock_going_backwards_restarts_the_acceleration_history() {
    let mut tracker = PublishTracker::new(accel_policy());
    for n in 5..8 {
        tracker.evaluate(tick(n), eid(1), &ramp(n));
    }

    let back = tracker
        .evaluate(tick(3), eid(1), &ramp(3))
        .expect("an entity whose clock went backwards is sent");

    assert_eq!(back.acceleration, Vec3::ZERO);
}

#[test]
fn the_heartbeat_shorter_than_min_interval_fires_at_min_interval() {
    let mut p = policy(DeadReckoning::Static);
    p.heartbeat = Seconds(0.05); // 2 ticks
    p.min_interval = Seconds(0.2); // 6 ticks

    let sends = send_ticks(&mut PublishTracker::new(p), eid(1), 25, still);

    assert_eq!(sends, [0, 6, 12, 18, 24]);
}

#[test]
fn the_position_threshold_is_strict() {
    for (offset, sends) in [(1.0, false), (1.5, true)] {
        let mut p = policy(DeadReckoning::Static);
        p.position_threshold_m = 1.0;
        let mut tracker = PublishTracker::new(p);
        tracker.evaluate(tick(0), eid(1), &still(0));

        let sent = tracker.evaluate(tick(1), eid(1), &common::kin(offset));

        assert_eq!(sent.is_some(), sends, "displacement {offset} m");
    }
}

#[test]
fn interval_ticks_follow_the_rate_of_the_time_given() {
    let mut p = policy(DeadReckoning::Static);
    p.heartbeat = Seconds(0.1); // 3 ticks at 30 Hz, 6 at 60 Hz
    let mut tracker = PublishTracker::new(p);
    let at_hz = |hz: u32, n: u64| {
        let rate = umbra_core::TickRate::new(hz).expect("non-zero rate");
        umbra_core::SimTime::at_tick(rate, n)
    };
    let sends = |tracker: &mut PublishTracker, id: u64, hz: u32| -> Vec<u64> {
        (0..13)
            .filter(|&n| tracker.evaluate(at_hz(hz, n), eid(id), &still(n)).is_some())
            .collect()
    };

    assert_eq!(sends(&mut tracker, 1, 30), [0, 3, 6, 9, 12]);
    assert_eq!(sends(&mut tracker, 2, 60), [0, 6, 12]);
}
