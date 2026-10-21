//! AT-12 (property): after every tick, the receiver-side extrapolation of the last sent basis is
//! within the position threshold of the true position, or an update was sent this tick; and no
//! gap between sends exceeds the heartbeat. Requirements: REQ-N-2, ADR 0032.
//!
//! Step 4 of M6: written before WI-6. Mutation to check when the policy exists: compare against the
//! last sent state instead of the extrapolated one, and this property fails.

#![forbid(unsafe_code)]
#![expect(clippy::cast_precision_loss, reason = "tick counts are small")]

mod common;

use proptest::prelude::*;
use umbra_core::dead_reckoning::{DeadReckoning, RemoteState, extrapolate};
use umbra_core::{Ecef, KinematicState, Quaternion, Seconds, Vec3};
use umbra_net::{PublishPolicy, PublishTracker};

use common::{eid, publish_policy, tick};

const DT: f64 = 1.0 / 30.0;

fn distance(a: Ecef, b: Ecef) -> f64 {
    let (dx, dy, dz) = (a.0.x - b.0.x, a.0.y - b.0.y, a.0.z - b.0.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

proptest! {
    #[test]
    #[ignore = "WI-6"]
    fn receiver_error_stays_within_the_threshold_or_an_update_is_sent(
        steps in proptest::collection::vec((-3.0_f64..3.0, -3.0_f64..3.0, -3.0_f64..3.0), 1..300),
        model in prop_oneof![Just(DeadReckoning::Static), Just(DeadReckoning::Velocity)],
        threshold in 0.2_f64..5.0,
    ) {
        let policy = PublishPolicy {
            position_threshold_m: threshold,
            model,
            ..publish_policy()
        };
        let mut tracker = PublishTracker::new(policy);
        let mut true_state = common::kin(0.0);
        let mut last: Option<(u64, RemoteState)> = None;

        for (n, (ax, ay, az)) in steps.iter().enumerate() {
            let n = n as u64;
            // Velocity changes by a bounded step each tick; position integrates the velocity.
            let v = Vec3::new(
                true_state.velocity.x + ax,
                true_state.velocity.y + ay,
                true_state.velocity.z + az,
            );
            true_state = KinematicState {
                position: Ecef(Vec3::new(
                    true_state.position.0.x + v.x * DT,
                    true_state.position.0.y + v.y * DT,
                    true_state.position.0.z + v.z * DT,
                )),
                velocity: v,
                orientation: Quaternion::IDENTITY,
            };

            if let Some(sent) = tracker.evaluate(tick(n), eid(1), &true_state) {
                last = Some((n, sent));
            }

            let (sent_at, basis) = last.expect("first sight sends at tick 0");
            let seen = extrapolate(&basis, Seconds((n - sent_at) as f64 * DT));
            let error = distance(seen.position, true_state.position);
            prop_assert!(
                sent_at == n || error <= threshold,
                "tick {n}: receiver error {error} m exceeds {threshold} m without an update"
            );
            prop_assert!(n - sent_at <= 150, "tick {n}: silent for more than the heartbeat");
        }
    }
}
