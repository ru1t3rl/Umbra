//! `example-dispatch`: an example action plugin (M7 WI-18, REQ-SDK-3, REQ-P-4, REQ-B-3, REQ-B-15,
//! ADR 0050, ADR 0052, ADR 0056).
//!
//! Plugin `example.plugin.dispatch` 0.1.0 (ABI 0.2), built with `umbra-sdk` only and
//! `#![forbid(unsafe_code)]`. One action, `example.action.dispatch`:
//!
//! - params `template` (string, required: a template ID), `done_key` (string, required: a `bool` key of
//!   the graph's `[blackboard]`, written, declared in the metadata as a blackboard param), `delay_s`
//!   (number >= 0, default 2) and `offset_forward_m` (number, default 20);
//! - like `builtin.action.wait`: Running for `n = ceil(delay_s * hz)` ticks from its own first tick; on
//!   tick `n + 1` it emits `Spawn { template, position, heading }` at the entity's position moved by
//!   `offset_forward_m` along the entity's body forward axis (from the orientation quaternion), with the
//!   entity's heading, writes `done_key = true` and returns Success; nothing is emitted on the Running
//!   ticks;
//! - `halt` resets the instance (idempotent), so it counts from its next first tick; after Success the
//!   next tick starts over.
//!
//! Status: M7 WI-2 skeleton. The contract above is what `crates/umbra-engine/tests/plugin_leaves.rs`
//! checks; WI-18 implements it.

#![forbid(unsafe_code)]

use umbra_sdk::{
    Action as SdkAction, ActionInstance, BlackboardAccess, EntityView, Error, Intents, LeafCtx,
    Status, export_plugin,
};

/// The capability `example.action.dispatch`.
struct Dispatch {
    template: String,
    done_key: String,
    delay_s: f64,
    offset_forward_m: f64,
}

/// One activation: ticks counted since the first tick.
struct DispatchInstance {
    waited: u64,
    delay_ticks: u64,
}

impl SdkAction for Dispatch {
    const ID: &'static str = "example.action.dispatch";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const SCHEMA: &'static str = r#"{
  "title": "example.action.dispatch parameters",
  "description": "Waits `delay_s`, spawns `template` `offset_forward_m` ahead of the entity, sets the bool key `done_key` and succeeds.",
  "type": "object",
  "properties": {
    "template": { "type": "string", "minLength": 3, "maxLength": 128,
      "description": "Template to spawn (a template ID)." },
    "done_key": { "type": "string", "minLength": 1, "maxLength": 128,
      "description": "A bool key of the graph's blackboard (written)." },
    "delay_s": { "type": "number", "minimum": 0, "maximum": 3600, "default": 2.0,
      "description": "Delay before the spawn, seconds (rounded up to whole ticks)." },
    "offset_forward_m": { "type": "number", "minimum": -10000, "maximum": 10000, "default": 20.0,
      "description": "Distance ahead of the entity at which the new entity appears, meters." }
  },
  "required": ["template", "done_key"],
  "additionalProperties": false
}"#;
    const METADATA: &'static str = r#"{"blackboard_params":[{"param":"done_key","type":"bool","access":"write"}],"result":"Running while waiting; Success on the tick it spawns"}"#;
    type Instance = DispatchInstance;

    fn configure(params: &str) -> Result<Self, Error> {
        let _ = params;
        unimplemented!("M7 WI-18")
    }

    fn instantiate(&self, rate_hz: u32) -> Result<DispatchInstance, Error> {
        let _ = (
            rate_hz,
            &self.template,
            &self.done_key,
            self.delay_s,
            self.offset_forward_m,
        );
        unimplemented!("M7 WI-18")
    }
}

impl ActionInstance for DispatchInstance {
    fn tick(
        &mut self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
        out: &mut Intents<'_>,
    ) -> Result<Status, Error> {
        let _ = (ctx, entity, bb, out, self.waited, self.delay_ticks);
        unimplemented!("M7 WI-18")
    }

    fn halt(&mut self) {
        unimplemented!("M7 WI-18")
    }
}

export_plugin! {
    id: "example.plugin.dispatch",
    name: "Dispatch example",
    version: "0.1.0",
    actions: [Dispatch],
}
