//! AT-3/WI-7: the `MotionModel` contract checks. Requirements: REQ-M-1, REQ-M-4, ASM-031.
//!
//! The checks must accept a well-behaved model and reject four deliberately broken stubs.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use umbra_core::motion::contract::{ContractViolation, check_motion_model};
use umbra_core::{
    Ecef, FlatTerrain, Intent, KinematicState, Meters, MotionModel, MotionModelId, Quaternion,
    Seconds, TerrainProvider, Vec3,
};

const DT: Seconds = Seconds(1.0 / 30.0);

#[expect(clippy::expect_used, reason = "test helper")]
fn model_id(s: &str) -> MotionModelId {
    MotionModelId::new(s).expect("valid id")
}

fn state(velocity: Vec3) -> KinematicState {
    KinematicState {
        position: Ecef(Vec3 {
            x: 6_378_137.0,
            y: 0.0,
            z: 0.0,
        }),
        velocity,
        orientation: Quaternion::IDENTITY,
    }
}

fn cases() -> Vec<(KinematicState, Vec<Intent>)> {
    vec![
        (state(Vec3::ZERO), vec![]),
        (state(Vec3::ZERO), vec![Intent::Hold]),
        (
            state(Vec3 {
                x: 0.0,
                y: 5.0,
                z: 0.0,
            }),
            vec![Intent::SetSpeed(umbra_core::MetersPerSecond(5.0))],
        ),
    ]
}

fn terrain() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

/// A model that leaves the state unchanged: deterministic, finite, unit, and stationary.
struct Identity(MotionModelId);
impl MotionModel for Identity {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        *s
    }
}

/// Broken: output depends on a call counter (hidden state across calls).
struct CountsCalls(MotionModelId, AtomicU64);
impl MotionModel for CountsCalls {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let n = self.1.fetch_add(1, Ordering::Relaxed);
        let mut out = *s;
        #[expect(clippy::cast_precision_loss, reason = "small test counter")]
        {
            out.velocity.y += n as f64;
        }
        out
    }
}

/// Broken: returns a non-finite value, deterministically (infinity compares equal to itself).
struct ReturnsInfinity(MotionModelId);
impl MotionModel for ReturnsInfinity {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *s;
        out.velocity.y = f64::INFINITY;
        out
    }
}

/// Broken: returns NaN.
struct ReturnsNan(MotionModelId);
impl MotionModel for ReturnsNan {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *s;
        out.position.0.x = f64::NAN;
        out
    }
}

/// Broken: the orientation quaternion is not unit length.
struct NonUnitQuaternion(MotionModelId);
impl MotionModel for NonUnitQuaternion {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *s;
        out.orientation = Quaternion {
            w: 2.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        out
    }
}

/// Broken: a stationary entity with no motion intent drifts.
struct DriftsWhenStationary(MotionModelId);
impl MotionModel for DriftsWhenStationary {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *s;
        out.position.0.z += 1.0;
        out
    }
}

/// REQ-M-1: a deterministic, finite, unit-quaternion, non-drifting model passes.
#[test]
fn given_well_behaved_model_when_checked_then_ok() {
    let m = Identity(model_id("test.stub.identity"));

    assert_eq!(check_motion_model(&m, &cases(), DT, &terrain()), Ok(()));
}

/// REQ-M-1: an empty case list is trivially fine.
#[test]
fn given_no_cases_when_checked_then_ok() {
    let m = Identity(model_id("test.stub.identity"));

    assert_eq!(check_motion_model(&m, &[], DT, &terrain()), Ok(()));
}

/// REQ-M-1: hidden state between calls is `NotDeterministic`.
#[test]
fn given_model_with_call_counter_when_checked_then_not_deterministic() {
    let m = CountsCalls(model_id("test.stub.counter"), AtomicU64::new(0));

    let got = check_motion_model(&m, &cases(), DT, &terrain());

    assert!(
        matches!(got, Err(ContractViolation::NotDeterministic { .. })),
        "{got:?}"
    );
}

/// REQ-M-1: an infinite output is `NonFinite`.
#[test]
fn given_model_returning_infinity_when_checked_then_non_finite() {
    let m = ReturnsInfinity(model_id("test.stub.infinity"));

    let got = check_motion_model(&m, &cases(), DT, &terrain());

    assert!(
        matches!(got, Err(ContractViolation::NonFinite { .. })),
        "{got:?}"
    );
}

/// REQ-M-1: a NaN output is rejected (`NonFinite`; a determinism comparison that trips on
/// `NaN != NaN` first would be a checker bug, so only `NonFinite` is accepted).
#[test]
fn given_model_returning_nan_when_checked_then_non_finite() {
    let m = ReturnsNan(model_id("test.stub.nan"));

    let got = check_motion_model(&m, &cases(), DT, &terrain());

    assert!(
        matches!(got, Err(ContractViolation::NonFinite { .. })),
        "{got:?}"
    );
}

/// REQ-M-1: a non-unit orientation quaternion is `NotUnitQuaternion`.
#[test]
fn given_model_with_non_unit_quaternion_when_checked_then_not_unit_quaternion() {
    let m = NonUnitQuaternion(model_id("test.stub.nonunit"));

    let got = check_motion_model(&m, &cases(), DT, &terrain());

    assert!(
        matches!(got, Err(ContractViolation::NotUnitQuaternion { .. })),
        "{got:?}"
    );
}

/// REQ-M-1: a stationary entity with no intents (or `Hold`) that moves is `StationaryMoved`.
#[test]
fn given_model_that_drifts_when_stationary_when_checked_then_stationary_moved() {
    let m = DriftsWhenStationary(model_id("test.stub.drift"));

    let got = check_motion_model(&m, &cases(), DT, &terrain());

    assert!(
        matches!(got, Err(ContractViolation::StationaryMoved { .. })),
        "{got:?}"
    );
}

/// REQ-M-1, REQ-M-4: `MotionModel` and `TerrainProvider` are object safe and shareable; one
/// instance can serve many entities (`Arc<dyn MotionModel>`, ASM-031).
#[test]
fn given_models_when_boxed_and_shared_then_trait_objects_work() {
    let boxed: Box<dyn MotionModel> = Box::new(Identity(model_id("test.stub.identity")));
    let shared: Arc<dyn MotionModel> = Arc::new(Identity(model_id("test.stub.shared")));
    let other = Arc::clone(&shared);
    let terrain: Arc<dyn TerrainProvider> = Arc::new(terrain());

    assert_eq!(boxed.id().as_str(), "test.stub.identity");
    assert_eq!(other.id().as_str(), "test.stub.shared");
    assert_eq!(
        check_motion_model(&*shared, &cases(), DT, &*terrain),
        Ok(())
    );
}

/// Drifts the position by a fixed amount along z every step.
struct Drift(MotionModelId, f64);
impl MotionModel for Drift {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *s;
        out.position.0.z += self.1;
        out
    }
}

/// ASM-045: the stationary tolerance is 1 mm.
#[test]
fn given_drift_around_one_millimetre_when_stationary_then_tolerance_is_1mm() {
    let stationary = vec![(state(Vec3::ZERO), vec![Intent::Hold])];

    let small = Drift(model_id("test.stub.small"), 5e-4);
    let large = Drift(model_id("test.stub.large"), 2e-3);

    assert_eq!(
        check_motion_model(&small, &stationary, DT, &terrain()),
        Ok(())
    );
    assert!(matches!(
        check_motion_model(&large, &stationary, DT, &terrain()),
        Err(ContractViolation::StationaryMoved { case: 0 })
    ));
}

/// ASM-045: with a `SetSpeed` intent the entity is not "stationary", so it may move.
#[test]
fn given_set_speed_intent_when_model_moves_then_stationary_rule_does_not_apply() {
    let cases = vec![(
        state(Vec3::ZERO),
        vec![Intent::SetSpeed(umbra_core::MetersPerSecond(3.0))],
    )];
    let m = Drift(model_id("test.stub.moves"), 1.0);

    assert_eq!(check_motion_model(&m, &cases, DT, &terrain()), Ok(()));
}
