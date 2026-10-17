//! Shared helpers for the kernel integration tests: stub motion models and kernel builders.
//!
//! The kernel may not depend on `cgf-motion` (ASM-005), so tests use these stubs. Each test
//! binary uses a subset of the helpers.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;
use std::sync::{Arc, Mutex};

use cgf_core::{
    Ecef, FlatTerrain, Intent, KinematicState, Meters, MetersPerSecond, MotionModel, MotionModelId,
    Ownership, Quaternion, Seconds, TerrainProvider, TickRate, Vec3,
};
use cgf_kernel::{ClockMode, EntityInit, Kernel, KernelConfig, ManualWallClock, RouteInit};

pub const A: f64 = 6_378_137.0;

pub fn rate() -> TickRate {
    TickRate::new(30).expect("non-zero rate")
}

/// An AFAP kernel on flat terrain at 0 m with a manual wall clock.
pub fn kernel(seed: u64) -> Kernel {
    kernel_with(seed, ClockMode::AsFastAsPossible, ManualWallClock::new())
}

pub fn kernel_with(seed: u64, clock: ClockMode, wall: ManualWallClock) -> Kernel {
    let config = KernelConfig {
        tick_rate: rate(),
        seed,
        clock,
    };
    Kernel::new(
        config,
        Box::new(FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(wall),
    )
}

/// A point on the equator at longitude offset `y_m` meters east of (0 N, 0 E), on the surface.
pub fn at(y_m: f64) -> Ecef {
    Ecef(Vec3 {
        x: A,
        y: y_m,
        z: 0.0,
    })
}

/// Stationary state (zero velocity, identity orientation) at `position`.
pub fn state_at(position: Ecef) -> KinematicState {
    KinematicState {
        position,
        velocity: Vec3::ZERO,
        orientation: Quaternion::IDENTITY,
    }
}

pub fn model_id(s: &str) -> MotionModelId {
    MotionModelId::new(s).expect("valid id")
}

/// What a [`Stub`] does on each step.
#[derive(Debug, Clone, Copy)]
pub enum Behavior {
    /// Returns the state unchanged.
    Stationary,
    /// Adds `velocity * dt` to the position (ECEF m/s) and keeps everything else.
    Drift(Vec3),
    /// Jumps to the destination of the last `MoveTo` intent (or stays without one).
    Teleport,
}

/// A recording stub motion model. Cloning the `Arc` shares the call log.
pub struct Stub {
    id: MotionModelId,
    behavior: Behavior,
    log: Mutex<Vec<Vec<Intent>>>,
}

impl Stub {
    pub fn new(id: &str, behavior: Behavior) -> Arc<Self> {
        Arc::new(Self {
            id: model_id(id),
            behavior,
            log: Mutex::new(Vec::new()),
        })
    }

    /// The intents received by each `step` call so far, in order.
    pub fn calls(&self) -> Vec<Vec<Intent>> {
        self.log.lock().expect("log lock").clone()
    }
}

impl MotionModel for Stub {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        _terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        self.log.lock().expect("log lock").push(intents.to_vec());
        let mut out = *state;
        match self.behavior {
            Behavior::Stationary => {}
            Behavior::Drift(v) => {
                out.position = Ecef(Vec3 {
                    x: state.position.0.x + v.x * dt.0,
                    y: state.position.0.y + v.y * dt.0,
                    z: state.position.0.z + v.z * dt.0,
                });
            }
            Behavior::Teleport => {
                let destination = intents.iter().rev().find_map(|i| match i {
                    Intent::MoveTo { destination } => Some(*destination),
                    _ => None,
                });
                if let Some(d) = destination {
                    out.position = d;
                }
            }
        }
        out
    }
}

/// An owned entity at `position` driven by `model`, with no route.
pub fn owned(position: Ecef, model: &Arc<Stub>) -> EntityInit {
    EntityInit {
        state: state_at(position),
        ownership: Ownership::Owned,
        motion: model.clone(),
        route: None,
    }
}

/// A route over `waypoints` with a 5 m arrival radius and the given cruise speed.
pub fn route(waypoints: Vec<Ecef>, cruise: f64) -> RouteInit {
    RouteInit {
        waypoints,
        arrival_radius: Meters(5.0),
        cruise_speed: MetersPerSecond(cruise),
    }
}

pub const MS_50: Duration = Duration::from_millis(50);
