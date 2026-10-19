//! `example-wander`: the example Umbra plugin (M4 WI-13, REQ-SDK-3, REQ-SDK-4, M4 acceptance 1).
//!
//! Plugin `example.plugin.wander` 0.1.0, built with `umbra-sdk` only and `#![forbid(unsafe_code)]`
//! like every plugin crate (ASM-116). It offers one behavior, `example.behavior.wander`, a random walk:
//!
//! - params `speed_mps` (number > 0, default 5) and `turn_every_s` (number > 0, default 10);
//! - on its first tick, and then every `ceil(turn_every_s * hz)` ticks, it draws a new absolute heading
//!   `2 * pi * next_f64()` (radians, true heading) from the entity's own RNG stream through the host API
//!   (REQ-K-4: no private RNG, no wall clock; time is the tick count of the instance);
//! - every tick it emits `SetSpeed(speed)` and `SetHeading(heading)` and returns Running (forever);
//! - `speed_mps` is bindable (`param_types`), so a graph calling it as a `subgraph` may bind it to a
//!   blackboard key (`{ key = .. }`); unsetting the binding falls back to the configured speed;
//! - output `turns` (integer): how many headings it has drawn since it was created or last halted;
//! - `halt` resets the instance (timer and `turns`), is idempotent and safe before the first tick;
//!   `halt_running` keeps the SDK default (the same full reset), so a `timeout` that halts it starts a
//!   fresh walk.
//!
//! It is the dogfood of the SDK (D-7) and the code `docs/plugin-guide.md` walks through. Build the
//! package with `cargo xtask build-plugins` and run `examples/scenarios/plugin_wander` with
//! `--plugin-path target/plugins` (ASM-113).

#![forbid(unsafe_code)]

use std::f64::consts::TAU;

use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intent, Intents, Interface,
    Params, Status, TickContext, Value, ValueType, export_plugin,
};

/// Default cruise speed in meters per second.
const DEFAULT_SPEED_MPS: f64 = 5.0;
/// Default time between heading changes in seconds.
const DEFAULT_TURN_EVERY_S: f64 = 10.0;

/// The capability `example.behavior.wander`.
struct WanderFactory;

impl BehaviorFactory for WanderFactory {
    const ID: &'static str = "example.behavior.wander";
    // ADR 0009 subset; the top-level `description` is the catalog text.
    const PARAM_SCHEMA: &'static str = r#"{
  "title": "example.behavior.wander parameters",
  "description": "Wanders: drives at `speed_mps` and draws a new random heading from the entity's RNG stream on its first tick and then every `turn_every_s` seconds (rounded up to whole ticks). Running forever. `speed_mps` may be bound by a calling graph; output `turns` counts the headings drawn since the start or the last halt.",
  "type": "object",
  "properties": {
    "speed_mps": {
      "type": "number",
      "exclusiveMinimum": 0,
      "default": 5.0,
      "description": "Cruise speed in meters per second (the motion model clamps it to its limits)."
    },
    "turn_every_s": {
      "type": "number",
      "exclusiveMinimum": 0,
      "default": 10.0,
      "description": "Seconds between heading changes."
    }
  },
  "additionalProperties": false,
  "examples": [
    { "speed_mps": 5.0, "turn_every_s": 10.0 }
  ]
}"#;
    type Behavior = Wander;

    fn interface() -> Interface {
        Interface::default()
            .param("speed_mps", ValueType::Number)
            .output("turns", ValueType::Integer)
            .result("Running forever")
    }

    fn configure(params: &Params<'_>) -> Result<Wander, Error> {
        let value = params.value()?;
        let number = |key: &str, default: f64| -> Result<f64, Error> {
            match value.get(key) {
                None => Ok(default),
                Some(v) => v
                    .as_f64()
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .ok_or_else(|| Error::params(format!("{key} must be a number > 0, got {v}"))),
            }
        };
        Ok(Wander {
            speed_mps: number("speed_mps", DEFAULT_SPEED_MPS)?,
            turn_every_s: number("turn_every_s", DEFAULT_TURN_EVERY_S)?,
        })
    }
}

/// A configured wander behavior.
struct Wander {
    speed_mps: f64,
    turn_every_s: f64,
}

impl Behavior for Wander {
    type Instance = WanderInstance;

    fn instantiate(&self, rate_hz: u32) -> Result<WanderInstance, Error> {
        let period = ticks_for(self.turn_every_s, rate_hz).ok_or_else(|| {
            Error::params(format!(
                "turn_every_s = {} s at {rate_hz} Hz is not a usable tick count",
                self.turn_every_s
            ))
        })?;
        Ok(WanderInstance {
            speed_mps: self.speed_mps,
            bound_speed_mps: None,
            period,
            since_turn: 0,
            heading_rad: 0.0,
            turns: 0,
        })
    }
}

/// Whole ticks for `seconds` at `hz`: `ceil(seconds * hz)`, at least 1, with the two-ULP guard of the
/// core's `TickRate::ticks_for` (ASM-066, ASM-078), so `0.1 s` at 30 Hz is 3 ticks. `None` when the
/// duration is not finite and positive, the rate is 0, or the count does not fit in `u64`.
fn ticks_for(seconds: f64, hz: u32) -> Option<u64> {
    if !seconds.is_finite() || seconds <= 0.0 || hz == 0 {
        return None;
    }
    let product = seconds * f64::from(hz);
    let nearest = product.round();
    let ticks =
        if nearest >= 1.0 && (product - nearest).abs() <= 2.0 * (product.next_up() - product) {
            nearest
        } else {
            product.ceil().max(1.0)
        };
    // 2^64 as f64; anything at or above does not fit in u64.
    if ticks >= 18_446_744_073_709_551_616.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to be whole and within [1, 2^64) above"
    )]
    Some(ticks as u64)
}

/// One entity's walk.
struct WanderInstance {
    /// The configured speed.
    speed_mps: f64,
    /// The speed bound by a calling graph, if any (overrides `speed_mps`).
    bound_speed_mps: Option<f64>,
    /// Ticks between heading changes (at least 1).
    period: u64,
    /// Ticks since the last heading change; 0 means "draw on the next tick".
    since_turn: u64,
    /// The current heading in radians (valid once `turns > 0`).
    heading_rad: f64,
    /// Headings drawn since creation or the last halt.
    turns: i64,
}

impl BehaviorInstance for WanderInstance {
    fn tick(
        &mut self,
        ctx: &mut TickContext<'_>,
        _entity: &EntityView<'_>,
        out: &mut Intents<'_>,
    ) -> Status {
        if self.since_turn == 0 {
            // The entity's own stream through the host (REQ-K-4); never a private RNG.
            self.heading_rad = TAU * ctx.rng().next_f64();
            self.turns = self.turns.saturating_add(1);
        }
        self.since_turn += 1;
        if self.since_turn >= self.period {
            self.since_turn = 0;
        }
        let speed = self.bound_speed_mps.unwrap_or(self.speed_mps);
        // A refused intent faults the call on the host side; nothing more to do here.
        let _ = out.push(Intent::SetSpeed(speed));
        let _ = out.push(Intent::SetHeading(self.heading_rad));
        Status::Running
    }

    fn halt(&mut self) {
        self.since_turn = 0;
        self.heading_rad = 0.0;
        self.turns = 0;
    }

    fn set_input(&mut self, param: &str, value: Option<Value>) -> Result<(), Error> {
        if param != "speed_mps" {
            return Err(Error::new(format!("undeclared input {param}")));
        }
        self.bound_speed_mps = match value {
            None => None,
            Some(Value::Number(n)) if n.is_finite() && n > 0.0 => Some(n),
            #[expect(
                clippy::cast_precision_loss,
                reason = "integer keys feed number params with `i as f64` (ASM-088)"
            )]
            Some(Value::Integer(i)) if i > 0 => Some(i as f64),
            Some(other) => {
                return Err(Error::new(format!(
                    "speed_mps must be a number > 0, got {other:?}"
                )));
            }
        };
        Ok(())
    }

    fn output(&self, port: &str) -> Option<Value> {
        (port == "turns").then_some(Value::Integer(self.turns))
    }
}

export_plugin! {
    id: "example.plugin.wander",
    name: "Wander example",
    version: "0.1.0",
    behaviors: [WanderFactory],
}
