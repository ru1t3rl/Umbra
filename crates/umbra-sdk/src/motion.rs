//! The motion model kind (ABI 0.2, ADR 0050, REQ-P-4, REQ-M-1, REQ-M-4).
//!
//! A [`MotionModel`] is configured once from JSON params and stepped once per entity and tick, like a
//! built-in `umbra_core::MotionModel`: stateless across ticks, equal inputs give bit-identical
//! outputs. The terrain comes with the call ([`MotionCtx::terrain`]).
//!
//! Status: M7 WI-2 stub. The signatures follow `docs/plans/M7.md` (section "SDK"); the behavior is
//! implemented in M7 WI-10.

use std::marker::PhantomData;

use crate::behavior::Intent;
use crate::error::Error;
use crate::services::{Terrain, Vec3};

/// Kinematic state of one entity: ECEF position and velocity, and the orientation quaternion
/// `[w, x, y, z]` rotating body FRD axes to ECEF (the mirror of `umbra_core::KinematicState`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicState {
    /// Position, ECEF meters.
    pub position: Vec3,
    /// Velocity, ECEF meters per second.
    pub velocity: Vec3,
    /// Unit quaternion `[w, x, y, z]`. The host refuses a result that is not unit within 1e-9.
    pub orientation: [f64; 4],
}

/// Time and terrain of one motion step.
#[derive(Debug)]
pub struct MotionCtx<'a> {
    tick: u64,
    rate_hz: u32,
    call: PhantomData<&'a ()>,
}

impl MotionCtx<'_> {
    /// Simulation tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        let _ = (self.tick, self.rate_hz, &self.call);
        unimplemented!("M7 WI-10")
    }

    /// Tick rate in Hz.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        unimplemented!("M7 WI-10")
    }

    /// The terrain of this call.
    #[must_use]
    pub fn terrain(&self) -> Terrain<'_> {
        unimplemented!("M7 WI-10")
    }
}

/// A motion model plugin capability (listed under `motion_models:` of `export_plugin!`).
pub trait MotionModel: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset); the top-level `description` is the catalog text.
    const SCHEMA: &'static str;

    /// Builds the model from the JSON params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &str) -> Result<Self, Error>;

    /// Advances `state` by `dt` seconds under `intents`. The host refuses a result that is not
    /// finite or whose orientation is not a unit quaternion within 1e-9.
    fn step(
        &self,
        ctx: &MotionCtx<'_>,
        state: &KinematicState,
        intents: &[Intent<'_>],
        dt: f64,
    ) -> KinematicState;
}
