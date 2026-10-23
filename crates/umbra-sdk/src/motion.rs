//! The motion model kind (ABI 0.2, ADR 0050, REQ-P-4, REQ-M-1, REQ-M-4).
//!
//! A [`MotionModel`] is configured once from JSON params and stepped once per entity and tick, like a
//! built-in `umbra_core::MotionModel`: stateless across ticks, equal inputs give bit-identical
//! outputs. The terrain comes with the call ([`MotionCtx::terrain`]). A motion model has no
//! randomness (like the core trait) and must not use a clock of its own (REQ-K-4).

use crate::behavior::{Fault, Intent};
use crate::convert::TerrainSvc;
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
    terrain: TerrainSvc,
    fault: &'a Fault,
}

impl<'a> MotionCtx<'a> {
    /// The context of one `step` call.
    pub(crate) fn new(tick: u64, rate_hz: u32, terrain: TerrainSvc, fault: &'a Fault) -> Self {
        Self {
            tick,
            rate_hz,
            terrain,
            fault,
        }
    }
}

impl MotionCtx<'_> {
    /// Simulation tick number when the host knows it, else 0: the host's motion model is time-free, so
    /// the ABI 0.2 host passes 0 (ASM-720). A model must not depend on it (equal inputs must give
    /// bit-identical outputs).
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Tick rate in Hz: `1 / dt` rounded to the nearest whole hertz, at least 1 (ASM-720).
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The terrain of this call.
    #[must_use]
    pub fn terrain(&self) -> Terrain<'_> {
        Terrain::new(self.terrain, self.fault)
    }
}

/// A motion model plugin capability (listed under `motion_models:` of `export_plugin!`).
pub trait MotionModel: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset); the top-level `description` is the catalog text.
    const SCHEMA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;

    /// Builds the model from the JSON params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &str) -> Result<Self, Error>;

    /// Advances `state` by `dt` seconds under `intents` (this tick's intents of the entity, in
    /// emission order). The host refuses a result that is not finite or whose orientation is not a
    /// unit quaternion within 1e-9. If a terrain query was refused during the step, the result is not
    /// reported (the host faulted the capability).
    fn step(
        &self,
        ctx: &MotionCtx<'_>,
        state: &KinematicState,
        intents: &[Intent<'_>],
        dt: f64,
    ) -> KinematicState;
}
