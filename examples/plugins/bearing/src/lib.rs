//! `example-bearing`: an example condition plugin (M7 WI-18, REQ-SDK-3, REQ-P-4, REQ-B-3, REQ-B-15,
//! ADR 0050, ADR 0056).
//!
//! Plugin `example.plugin.bearing` 0.1.0 (ABI 0.2), built with `umbra-sdk` only and
//! `#![forbid(unsafe_code)]`. One condition, `example.cond.bearing_within`:
//!
//! - params `target_key` (string, required: an `entity` key of the graph's `[blackboard]`, read only,
//!   declared in the metadata as a blackboard param) and `max_bearing_deg` (number in (0, 180],
//!   default 30);
//! - Success iff the entity in `target_key` is a current contact of the entity and the absolute angle
//!   between the entity's heading (its body forward axis projected on the local horizon) and the
//!   direction to the target (projected on the horizon) is at most `max_bearing_deg` (inclusive);
//!   Failure when the key is unset or the target is not a contact;
//! - it writes nothing.
//!
//! Status: M7 WI-2 skeleton. The contract above is what `crates/umbra-engine/tests/plugin_leaves.rs`
//! checks; WI-18 implements it.

#![forbid(unsafe_code)]

use umbra_sdk::{
    BlackboardAccess, Condition as SdkCondition, EntityView, Error, LeafCtx, export_plugin,
};

/// The capability `example.cond.bearing_within`.
struct BearingWithin {
    target_key: String,
    max_bearing_deg: f64,
}

impl SdkCondition for BearingWithin {
    const ID: &'static str = "example.cond.bearing_within";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const SCHEMA: &'static str = r#"{
  "title": "example.cond.bearing_within parameters",
  "description": "Success iff the target held in the blackboard key `target_key` is a contact within `max_bearing_deg` of the entity's heading.",
  "type": "object",
  "properties": {
    "target_key": { "type": "string", "minLength": 1, "maxLength": 128,
      "description": "An entity key of the graph's blackboard (read only)." },
    "max_bearing_deg": { "type": "number", "exclusiveMinimum": 0, "maximum": 180, "default": 30.0,
      "description": "Largest angle between heading and target direction, degrees (inclusive)." }
  },
  "required": ["target_key"],
  "additionalProperties": false
}"#;
    const METADATA: &'static str = r#"{"blackboard_params":[{"param":"target_key","type":"entity","access":"read"}],"result":"Success when the target is a contact inside the bearing; Failure otherwise"}"#;

    fn configure(params: &str) -> Result<Self, Error> {
        let _ = params;
        unimplemented!("M7 WI-18")
    }

    fn check(
        &self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
    ) -> Result<bool, Error> {
        let _ = (ctx, entity, bb, &self.target_key, self.max_bearing_deg);
        unimplemented!("M7 WI-18")
    }
}

export_plugin! {
    id: "example.plugin.bearing",
    name: "Bearing example",
    version: "0.1.0",
    conditions: [BearingWithin],
}
