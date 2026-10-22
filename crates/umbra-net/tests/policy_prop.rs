//! AT-12 (property): after every tick, the receiver-side extrapolation of the last sent basis is
//! within the position and orientation thresholds of the truth, or an update was sent this tick
//! (inside `min_interval` the check is relaxed); gaps between sends are at least `min_interval` and
//! at most the larger of heartbeat and `min_interval`. Requirements: REQ-N-2, ADR 0032.
//!
//! Written in step 4 of M6, run from WI-6. Mutation proven in WI-6: compare against the
//! last sent state instead of the extrapolated one, and this property fails.

#![forbid(unsafe_code)]
#![expect(clippy::cast_precision_loss, reason = "tick counts are small")]

mod common;

use proptest::prelude::*;
use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{Ecef, KinematicState, Quaternion, Radians, Seconds, Vec3, WorldEuler};
use umbra_net::{PublishPolicy, PublishTracker};

use common::{eid, publish_policy, tick};

const DT: f64 = 1.0 / 30.0;

fn distance(a: Ecef, b: Ecef) -> f64 {
    let (dx, dy, dz) = (a.0.x - b.0.x, a.0.y - b.0.y, a.0.z - b.0.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

proptest! {
    #[test]
    fn receiver_error_stays_within_the_thresholds_or_an_update_is_sent(
        steps in proptest::collection::vec(
            (-3.0_f64..3.0, -3.0_f64..3.0, -3.0_f64..3.0, -0.05_f64..0.05),
            1..300,
        ),
        model in prop_oneof![
            Just(DeadReckoning::Static),
            Just(DeadReckoning::Velocity),
            Just(DeadReckoning::Acceleration),
        ],
        threshold in 0.2_f64..5.0,
        min_ticks in prop_oneof![Just(0_u64), Just(2), Just(5)],
        heartbeat_ticks in prop_oneof![Just(2_u64), Just(15), Just(150)],
    ) {
        let policy = PublishPolicy {
            position_threshold_m: threshold,
            model,
            min_interval: Seconds(min_ticks as f64 / 30.0),
            heartbeat: Seconds(heartbeat_ticks as f64 / 30.0),
            ..publish_policy()
        };
        let angle_threshold = policy.orientation_threshold.0;
        let mut tracker = PublishTracker::new(policy);
        let mut true_state = common::kin(0.0);
        let mut yaw = 0.0_f64;
        let mut last: Option<(u64, RemoteState)> = None;

        for (n, (ax, ay, az, dyaw)) in steps.iter().enumerate() {
            let n = n as u64;
            // Velocity changes by a bounded step each tick; position integrates the velocity.
            let v = Vec3::new(
                true_state.velocity.x + ax,
                true_state.velocity.y + ay,
                true_state.velocity.z + az,
            );
            yaw += dyaw;
            true_state = KinematicState {
                position: Ecef(Vec3::new(
                    true_state.position.0.x + v.x * DT,
                    true_state.position.0.y + v.y * DT,
                    true_state.position.0.z + v.z * DT,
                )),
                velocity: v,
                orientation: Quaternion::from_world_euler(WorldEuler {
                    yaw: Radians(yaw),
                    pitch: Radians(0.0),
                    roll: Radians(0.0),
                }),
            };

            let sent_now = tracker.evaluate(tick(n), eid(1), &true_state);
            if let Some(sent) = sent_now {
                if let Some((previous, _)) = last {
                    let gap = n - previous;
                    prop_assert!(gap >= min_ticks, "tick {n}: sent {gap} ticks after the last, min {min_ticks}");
                }
                last = Some((n, sent));
            }

            let (sent_at, basis) = last.expect("first sight sends at tick 0");
            let silent = n - sent_at;
            prop_assert!(
                silent <= heartbeat_ticks.max(min_ticks),
                "tick {n}: silent for {silent} ticks, heartbeat {heartbeat_ticks}, min {min_ticks}"
            );
            if sent_at != n && silent >= min_ticks {
                let seen = extrapolate(&basis, Seconds(silent as f64 * DT));
                let error = distance(seen.position, true_state.position);
                prop_assert!(
                    error <= threshold,
                    "tick {n}: receiver error {error} m exceeds {threshold} m without an update"
                );
                let turned = seen.orientation.angle_between(true_state.orientation).0;
                prop_assert!(
                    turned <= angle_threshold,
                    "tick {n}: receiver orientation off by {turned} rad (limit {angle_threshold})"
                );
            }
        }
    }
}
