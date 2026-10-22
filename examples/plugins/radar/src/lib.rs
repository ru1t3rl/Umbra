//! `example-radar`: an example sensor plugin (M7 WI-17, REQ-SDK-3, REQ-P-4, REQ-B-5, ADR 0050,
//! ADR 0056).
//!
//! Plugin `example.plugin.radar` 0.1.0 (ABI 0.2), built with `umbra-sdk` only and
//! `#![forbid(unsafe_code)]`. One sensor, `example.sensor.radar`:
//!
//! - params `range_m` (number in (0, 100000], required), `min_elevation_deg` and `max_elevation_deg`
//!   (numbers in [-90, 90], defaults -90 and 90), `antenna_height_m` (number in [0, 1000], default 2:
//!   the eye height of `builtin.visual`), `detect_probability` (number in [0, 1], default 1);
//! - for each candidate in id order that is within `range_m` (3D, inclusive), whose elevation above the
//!   observer's local horizon (the angle between the line to the candidate and the plane normal to the
//!   geocentric vertical at the observer; the SDK has no geodesy, so the vertical is the radial
//!   direction) lies in `[min_elevation_deg, max_elevation_deg]` (edges inclusive) and for which
//!   `terrain.line_of_sight(from, to)` holds, with `from` and `to` the observer and candidate
//!   positions each raised by `antenna_height_m` along its geocentric vertical (a chord on the
//!   ellipsoid dips below the terrain without the raise), it draws ONE `next_f64()` from the
//!   observer's stream and reports a contact iff `draw < detect_probability`; a candidate rejected by
//!   a gate draws nothing (REQ-K-4: the draw sequence is a function of the geometry, in candidate
//!   order);
//! - the contact carries the candidate's id, position, velocity, force and the 3D range.
//!
//! Status: M7 WI-2 skeleton. The contract above is what `crates/umbra-engine/tests/plugin_radar.rs`
//! checks against a reference implementation; WI-17 implements it.

#![forbid(unsafe_code)]

use umbra_sdk::{Contacts, Error, SenseCtx, Sensor as SdkSensor, TargetView, export_plugin};

/// The capability `example.sensor.radar`.
struct Radar {
    range_m: f64,
    min_elevation_deg: f64,
    max_elevation_deg: f64,
    antenna_height_m: f64,
    detect_probability: f64,
}

impl SdkSensor for Radar {
    const ID: &'static str = "example.sensor.radar";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const SCHEMA: &'static str = r#"{
  "title": "example.sensor.radar parameters",
  "description": "Radar: contacts every candidate within `range_m` and the elevation gate that has line of sight, with probability `detect_probability` per candidate and tick (one draw from the observer's stream).",
  "type": "object",
  "properties": {
    "range_m": { "type": "number", "exclusiveMinimum": 0, "maximum": 100000,
      "description": "3D detection range, meters (inclusive)." },
    "min_elevation_deg": { "type": "number", "minimum": -90, "maximum": 90, "default": -90.0,
      "description": "Lowest elevation above the observer's horizon, degrees (inclusive)." },
    "max_elevation_deg": { "type": "number", "minimum": -90, "maximum": 90, "default": 90.0,
      "description": "Highest elevation above the observer's horizon, degrees (inclusive)." },
    "antenna_height_m": { "type": "number", "minimum": 0, "maximum": 1000, "default": 2.0,
      "description": "Both line-of-sight endpoints are raised by this height along the vertical, meters." },
    "detect_probability": { "type": "number", "minimum": 0, "maximum": 1, "default": 1.0,
      "description": "Probability that a candidate inside every gate is reported." }
  },
  "required": ["range_m"],
  "additionalProperties": false
}"#;

    fn configure(params: &str) -> Result<Self, Error> {
        let _ = params;
        unimplemented!("M7 WI-17")
    }

    fn sense(
        &self,
        ctx: &mut SenseCtx<'_>,
        observer: &TargetView<'_>,
        candidates: &[TargetView<'_>],
        out: &mut Contacts<'_>,
    ) -> Result<(), Error> {
        let _ = (
            ctx,
            observer,
            candidates,
            out,
            self.range_m,
            self.min_elevation_deg,
            self.max_elevation_deg,
            self.antenna_height_m,
            self.detect_probability,
        );
        unimplemented!("M7 WI-17")
    }
}

export_plugin! {
    id: "example.plugin.radar",
    name: "Radar example",
    version: "0.1.0",
    sensors: [Radar],
}
