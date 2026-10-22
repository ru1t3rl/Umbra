//! `example-hover`: an example motion model plugin (M7 WI-17, REQ-SDK-3, REQ-P-4, ADR 0050, ADR 0056).
//!
//! Plugin `example.plugin.hover` 0.1.0 (ABI 0.2), built with `umbra-sdk` only and
//! `#![forbid(unsafe_code)]` like every plugin crate. One motion model, `example.motion.hover`, a
//! multirotor-style kinematic model that holds a height above ground through the terrain service:
//!
//! - params `height_agl_m` (number in [1, 500], default 20), `max_speed_mps` (number in (0, 100],
//!   default 15), `max_climb_mps` (number in (0, 50], default 5), `max_accel_mps2` (number in (0, 50],
//!   default 4);
//! - horizontal motion: `SetSpeed` sets the target speed (clamped to `[0, max_speed_mps]`, the speed
//!   changes by at most `max_accel_mps2 * dt` per step), `SetHeading` the true heading (instant),
//!   `MoveTo` heads for the destination, `Hold` brakes to 0; with no motion intent the current
//!   horizontal velocity is kept;
//! - vertical motion: the height above the ellipsoid moves toward `terrain.height_at(position) +
//!   height_agl_m` by at most `max_climb_mps * dt` per step (the terrain comes from the call, so the
//!   model follows slopes exactly as a built-in would); the velocity includes the vertical rate;
//! - orientation: yaw from the heading, no pitch or roll; always a unit quaternion;
//! - stateless: equal inputs give bit-identical outputs (REQ-M-4).
//!
//! Status: M7 WI-2 skeleton. The contract above is what the acceptance test
//! `crates/umbra-engine/tests/plugin_hover.rs` checks; WI-17 implements it.

#![forbid(unsafe_code)]

use umbra_sdk::{
    Error, Intent, KinematicState, MotionCtx, MotionModel as SdkMotionModel, export_plugin,
};

/// The capability `example.motion.hover`.
struct Hover {
    height_agl_m: f64,
    max_speed_mps: f64,
    max_climb_mps: f64,
    max_accel_mps2: f64,
}

impl SdkMotionModel for Hover {
    const ID: &'static str = "example.motion.hover";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const SCHEMA: &'static str = r#"{
  "title": "example.motion.hover parameters",
  "description": "Multirotor-style kinematic model: holds `height_agl_m` above the terrain (climb rate limited), horizontal speed and heading from the intents, with an acceleration limit.",
  "type": "object",
  "properties": {
    "height_agl_m": { "type": "number", "minimum": 1, "maximum": 500, "default": 20.0,
      "description": "Height to hold above the terrain, meters." },
    "max_speed_mps": { "type": "number", "exclusiveMinimum": 0, "maximum": 100, "default": 15.0,
      "description": "Horizontal speed limit, m/s." },
    "max_climb_mps": { "type": "number", "exclusiveMinimum": 0, "maximum": 50, "default": 5.0,
      "description": "Vertical rate limit, m/s." },
    "max_accel_mps2": { "type": "number", "exclusiveMinimum": 0, "maximum": 50, "default": 4.0,
      "description": "Horizontal acceleration and braking limit, m/s^2." }
  },
  "additionalProperties": false
}"#;

    fn configure(params: &str) -> Result<Self, Error> {
        let _ = params;
        unimplemented!("M7 WI-17")
    }

    fn step(
        &self,
        ctx: &MotionCtx<'_>,
        state: &KinematicState,
        intents: &[Intent<'_>],
        dt: f64,
    ) -> KinematicState {
        let _ = (
            ctx,
            state,
            intents,
            dt,
            self.height_agl_m,
            self.max_speed_mps,
            self.max_climb_mps,
            self.max_accel_mps2,
        );
        unimplemented!("M7 WI-17")
    }
}

export_plugin! {
    id: "example.plugin.hover",
    name: "Hover example",
    version: "0.1.0",
    motion_models: [Hover],
}
