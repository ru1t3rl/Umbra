//! The ABI 0.2 capabilities of the fixture (cargo feature `kinds`, M7 WI-2, ADR 0050, ASM-668): the
//! crate then is plugin `test.plugin.kinds` 0.1.0 with only the capabilities below, built on `umbra-sdk`
//! only. They are the fixture of the gate-13 rows of the four new kinds and of the host bridge tests.
//!
//! Every capability is a plain unit or a struct of its params; the bodies follow the table (M7 WI-10).
//! A capability that hits a host refusal returns the SDK's error (`?`), so the call reports the host's
//! code; the motion models, whose `step` cannot fail, return the input state and the SDK reports the
//! refusal. Params are read with `serde_json` through `umbra_sdk::serde_json` (REQ-SDK-3).
//!
//! | Capability | Behavior |
//! |---|---|
//! | `test.motion.coast` | position += velocity * dt (ECEF), velocity and orientation kept |
//! | `test.motion.terrain_echo` | position and orientation kept; velocity := `[height_at(p), normal_at(p).up, 1 or 0 for line_of_sight(p, p + 100 m radially up)]` |
//! | `test.motion.non_unit` | orientation `[2, 0, 0, 0]` |
//! | `test.motion.nan` | position x is NaN |
//! | `test.motion.panic_step` | `step` panics |
//! | `test.sensor.all` | a contact for every candidate (range = 3D distance) |
//! | `test.sensor.los` | a contact for every candidate with terrain line of sight from the observer |
//! | `test.sensor.coin` | one `next_f64()` per candidate in order, a contact iff the draw is below 0.5 |
//! | `test.sensor.unknown_contact` | one contact naming entity 999999, which is no candidate |
//! | `test.sensor.panic_sense` | `sense` panics |
//! | `test.cond.always` | true |
//! | `test.cond.flag` | param `key` (bool key, read): true iff the key is `true` |
//! | `test.cond.mark` | param `key` (bool key, write): sets it `true`, true |
//! | `test.cond.undeclared` | reads the key `secret`, which no param declares (the host refuses it) |
//! | `test.cond.wrong_type` | param `key` (bool key, write): writes `Number(1.0)` (the host refuses it) |
//! | `test.cond.non_finite` | param `key` (number key, write): writes `Number(NaN)` (the host refuses it) |
//! | `test.cond.panic_check` | `check` panics |
//! | `test.action.count` | param `ticks` (integer >= 1): Running for its first `ticks` ticks, emitting `SetSpeed(n)` on tick n; Success on tick `ticks + 1` with no intent; `halt` resets, and so does finishing |
//! | `test.action.emit` | one `Custom { kind: "test.event.ping", payload: "{}" }`, Success |
//! | `test.action.mark` | param `key` (bool key, write): sets it `true`, Success |
//! | `test.action.read_only_write` | param `key` (bool key, read): tries to write it (the host refuses it) |
//! | `test.action.panic_tick` | emits `SetSpeed(1)`, then panics |

use umbra_sdk::serde_json::{self, Value as Json};
use umbra_sdk::{
    Action, ActionInstance, BlackboardAccess, Condition, Contact, Contacts, EntityView, Error,
    Intent, Intents, KinematicState, LeafCtx, MotionCtx, MotionModel, SenseCtx, Sensor, Status,
    TargetView, Value,
};

macro_rules! plain_schema {
    ($desc:literal) => {
        concat!(r#"{"type":"object","description":""#, $desc, r#""}"#)
    };
}

/// A schema with the one required string param `key`.
macro_rules! key_schema {
    ($desc:literal) => {
        concat!(
            r#"{"type":"object","description":""#,
            $desc,
            r#"","properties":{"key":{"type":"string","minLength":1}},"required":["key"],"additionalProperties":false}"#
        )
    };
}

macro_rules! key_metadata {
    ($ty:literal, $access:literal) => {
        concat!(
            r#"{"blackboard_params":[{"param":"key","type":""#,
            $ty,
            r#"","access":""#,
            $access,
            r#""}],"result":"fixture"}"#
        )
    };
}

/// The params as a JSON object.
fn params_of(params: &str) -> Result<Json, Error> {
    serde_json::from_str(params).map_err(|e| Error::params(format!("params are not JSON: {e}")))
}

/// The required string param `key` (the name of the blackboard key, ADR 0050).
fn key_of(params: &str) -> Result<String, Error> {
    params_of(params)?
        .get("key")
        .and_then(Json::as_str)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::params("`key` must be a non-empty string"))
}

/// `b - a`.
fn minus(b: [f64; 3], a: [f64; 3]) -> [f64; 3] {
    [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
}

/// The length of `v`, summed in the order of `umbra_core::Vec3::norm` (bit-identical).
fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// A contact for candidate `c` at range `range_m`.
fn contact<'a>(c: &TargetView<'a>, range_m: f64) -> Contact<'a> {
    Contact {
        entity: c.id,
        position: c.state.position,
        velocity: c.state.velocity,
        force: c.force,
        range_m,
    }
}

/// A contact for `c` at its 3D distance from `observer`.
fn contact_from<'a>(observer: &TargetView<'_>, c: &TargetView<'a>) -> Contact<'a> {
    contact(c, norm(minus(c.state.position, observer.state.position)))
}

// ---------------------------------------------------------------- motion models

/// Declares a motion model whose `step` is `$step(ctx, state, dt)`.
macro_rules! motion {
    ($name:ident, $id:literal, $desc:literal, $step:expr) => {
        pub(crate) struct $name;
        impl MotionModel for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = plain_schema!($desc);
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn step(
                &self,
                ctx: &MotionCtx<'_>,
                state: &KinematicState,
                _: &[Intent<'_>],
                dt: f64,
            ) -> KinematicState {
                let step: fn(&MotionCtx<'_>, &KinematicState, f64) -> KinematicState = $step;
                step(ctx, state, dt)
            }
        }
    };
}

motion!(
    Coast,
    "test.motion.coast",
    "Coasts along its velocity",
    |_, s, dt| KinematicState {
        position: [
            s.position[0] + s.velocity[0] * dt,
            s.position[1] + s.velocity[1] * dt,
            s.position[2] + s.velocity[2] * dt,
        ],
        ..*s
    }
);
motion!(
    TerrainEcho,
    "test.motion.terrain_echo",
    "Echoes the terrain",
    |ctx, s, _| terrain_echo(ctx, s).unwrap_or(*s)
);
motion!(
    NonUnit,
    "test.motion.non_unit",
    "Returns a non-unit orientation",
    |_, s, _| KinematicState {
        orientation: [2.0, 0.0, 0.0, 0.0],
        ..*s
    }
);
motion!(
    Nan,
    "test.motion.nan",
    "Returns a NaN position",
    |_, s, _| KinematicState {
        position: [f64::NAN, s.position[1], s.position[2]],
        ..*s
    }
);
motion!(
    PanicStep,
    "test.motion.panic_step",
    "Panics in step",
    |_, _, _| panic!("panic_step: step panics on purpose")
);

/// `test.motion.terrain_echo`: the terrain's answers at the entity's position as its velocity.
fn terrain_echo(ctx: &MotionCtx<'_>, s: &KinematicState) -> Result<KinematicState, Error> {
    let terrain = ctx.terrain();
    let p = s.position;
    let r = norm(p);
    let up = [
        p[0] + 100.0 * p[0] / r,
        p[1] + 100.0 * p[1] / r,
        p[2] + 100.0 * p[2] / r,
    ];
    let height = terrain.height_at(p)?;
    let normal_up = terrain.normal_at(p)?[2];
    let visible = terrain.line_of_sight(p, up)?;
    Ok(KinematicState {
        velocity: [height, normal_up, if visible { 1.0 } else { 0.0 }],
        ..*s
    })
}

// ---------------------------------------------------------------- sensors

/// What a sensor fixture does.
#[derive(Clone, Copy)]
enum SenseMode {
    All,
    Los,
    Coin,
    UnknownContact,
    Panic,
}

fn sense_as(
    mode: SenseMode,
    ctx: &mut SenseCtx<'_>,
    observer: &TargetView<'_>,
    candidates: &[TargetView<'_>],
    out: &mut Contacts<'_>,
) -> Result<(), Error> {
    match mode {
        SenseMode::All => {
            for c in candidates {
                out.push(&contact_from(observer, c))?;
            }
        }
        SenseMode::Los => {
            for c in candidates {
                if ctx
                    .terrain()
                    .line_of_sight(observer.state.position, c.state.position)?
                {
                    out.push(&contact_from(observer, c))?;
                }
            }
        }
        SenseMode::Coin => {
            for c in candidates {
                if ctx.rng().next_f64() < 0.5 {
                    out.push(&contact_from(observer, c))?;
                }
            }
        }
        SenseMode::UnknownContact => {
            out.push(&Contact {
                entity: 999_999,
                position: observer.state.position,
                velocity: [0.0; 3],
                force: None,
                range_m: 1.0,
            })?;
        }
        SenseMode::Panic => panic!("panic_sense: sense panics on purpose"),
    }
    Ok(())
}

macro_rules! sensor {
    ($name:ident, $id:literal, $desc:literal, $mode:expr) => {
        pub(crate) struct $name;
        impl Sensor for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = plain_schema!($desc);
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn sense(
                &self,
                ctx: &mut SenseCtx<'_>,
                observer: &TargetView<'_>,
                candidates: &[TargetView<'_>],
                out: &mut Contacts<'_>,
            ) -> Result<(), Error> {
                sense_as($mode, ctx, observer, candidates, out)
            }
        }
    };
}

sensor!(
    All,
    "test.sensor.all",
    "Contacts every candidate",
    SenseMode::All
);
sensor!(
    Los,
    "test.sensor.los",
    "Contacts candidates in line of sight",
    SenseMode::Los
);
sensor!(
    Coin,
    "test.sensor.coin",
    "Contacts candidates by coin flip",
    SenseMode::Coin
);
sensor!(
    UnknownContact,
    "test.sensor.unknown_contact",
    "Contacts a non-candidate",
    SenseMode::UnknownContact
);
sensor!(
    PanicSense,
    "test.sensor.panic_sense",
    "Panics in sense",
    SenseMode::Panic
);

// ---------------------------------------------------------------- conditions

/// What a condition fixture does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckMode {
    Always,
    Flag,
    Mark,
    Undeclared,
    WrongType,
    NonFinite,
    Panic,
}

impl CheckMode {
    /// Whether the condition reads its blackboard key from the param `key`.
    fn has_key(self) -> bool {
        matches!(
            self,
            Self::Flag | Self::Mark | Self::WrongType | Self::NonFinite
        )
    }
}

/// A configured condition fixture: its key (when it has one).
pub(crate) struct Keyed {
    key: String,
}

impl Keyed {
    fn configure(mode: CheckMode, params: &str) -> Result<Self, Error> {
        let key = if mode.has_key() {
            key_of(params)?
        } else {
            String::new()
        };
        Ok(Self { key })
    }

    fn check(&self, mode: CheckMode, bb: &mut BlackboardAccess<'_>) -> Result<bool, Error> {
        match mode {
            CheckMode::Always => Ok(true),
            CheckMode::Flag => Ok(bb.get(&self.key)? == Some(Value::Bool(true))),
            CheckMode::Mark => {
                bb.set(&self.key, Some(Value::Bool(true)))?;
                Ok(true)
            }
            CheckMode::Undeclared => Ok(bb.get("secret")? == Some(Value::Bool(true))),
            CheckMode::WrongType => {
                bb.set(&self.key, Some(Value::Number(1.0)))?;
                Ok(true)
            }
            CheckMode::NonFinite => {
                bb.set(&self.key, Some(Value::Number(f64::NAN)))?;
                Ok(true)
            }
            CheckMode::Panic => panic!("panic_check: check panics on purpose"),
        }
    }
}

macro_rules! condition {
    ($name:ident, $id:literal, $schema:expr, $metadata:expr, $mode:expr) => {
        pub(crate) struct $name(Keyed);
        impl Condition for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = $schema;
            const METADATA: &'static str = $metadata;
            fn configure(params: &str) -> Result<Self, Error> {
                Keyed::configure($mode, params).map(Self)
            }
            fn check(
                &self,
                _: &mut LeafCtx<'_>,
                _: &EntityView<'_>,
                bb: &mut BlackboardAccess<'_>,
            ) -> Result<bool, Error> {
                self.0.check($mode, bb)
            }
        }
    };
}

condition!(
    Always,
    "test.cond.always",
    plain_schema!("True"),
    "",
    CheckMode::Always
);
condition!(
    Flag,
    "test.cond.flag",
    key_schema!("True iff the key is true"),
    key_metadata!("bool", "read"),
    CheckMode::Flag
);
condition!(
    Mark,
    "test.cond.mark",
    key_schema!("Sets the key"),
    key_metadata!("bool", "write"),
    CheckMode::Mark
);
condition!(
    Undeclared,
    "test.cond.undeclared",
    plain_schema!("Reads an undeclared key"),
    "",
    CheckMode::Undeclared
);
condition!(
    WrongType,
    "test.cond.wrong_type",
    key_schema!("Writes a number to a bool key"),
    key_metadata!("bool", "write"),
    CheckMode::WrongType
);
condition!(
    NonFinite,
    "test.cond.non_finite",
    key_schema!("Writes NaN to a number key"),
    key_metadata!("number", "write"),
    CheckMode::NonFinite
);
condition!(
    PanicCheck,
    "test.cond.panic_check",
    plain_schema!("Panics in check"),
    "",
    CheckMode::Panic
);

// ---------------------------------------------------------------- actions

/// What an action fixture does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TickMode {
    Count,
    Emit,
    Mark,
    ReadOnlyWrite,
    Panic,
}

/// A configured action fixture: its key and tick count (when it has them).
#[derive(Clone)]
pub(crate) struct ActionParams {
    key: String,
    ticks: u32,
}

impl ActionParams {
    fn configure(mode: TickMode, params: &str) -> Result<Self, Error> {
        let key = if matches!(mode, TickMode::Mark | TickMode::ReadOnlyWrite) {
            key_of(params)?
        } else {
            String::new()
        };
        let ticks = if mode == TickMode::Count {
            params_of(params)?
                .get("ticks")
                .and_then(Json::as_u64)
                .and_then(|t| u32::try_from(t).ok())
                .filter(|&t| t >= 1)
                .ok_or_else(|| Error::params("`ticks` must be an integer >= 1"))?
        } else {
            0
        };
        Ok(Self { key, ticks })
    }
}

/// One activation of an action fixture: the ticks it ran so far.
pub(crate) struct Activation {
    params: ActionParams,
    ran: u32,
}

impl Activation {
    fn tick(
        &mut self,
        mode: TickMode,
        bb: &mut BlackboardAccess<'_>,
        out: &mut Intents<'_>,
    ) -> Result<Status, Error> {
        match mode {
            TickMode::Count => {
                if self.ran < self.params.ticks {
                    self.ran += 1;
                    out.push(Intent::SetSpeed(f64::from(self.ran)))?;
                    Ok(Status::Running)
                } else {
                    self.ran = 0;
                    Ok(Status::Success)
                }
            }
            TickMode::Emit => {
                out.push(Intent::Custom {
                    kind: "test.event.ping",
                    payload_json: "{}",
                })?;
                Ok(Status::Success)
            }
            // The same write: the host grants it for `mark` and refuses it for the read-only key of
            // `read_only_write` (its metadata declares `read`).
            TickMode::Mark | TickMode::ReadOnlyWrite => {
                bb.set(&self.params.key, Some(Value::Bool(true)))?;
                Ok(Status::Success)
            }
            TickMode::Panic => {
                out.push(Intent::SetSpeed(1.0))?;
                panic!("panic_tick: tick panics on purpose")
            }
        }
    }
}

/// An action and its instance type.
macro_rules! action {
    ($name:ident, $instance:ident, $id:literal, $schema:expr, $metadata:expr, $mode:expr) => {
        pub(crate) struct $name(ActionParams);
        pub(crate) struct $instance(Activation);
        impl Action for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = $schema;
            const METADATA: &'static str = $metadata;
            type Instance = $instance;
            fn configure(params: &str) -> Result<Self, Error> {
                ActionParams::configure($mode, params).map(Self)
            }
            fn instantiate(&self, _: u32) -> Result<$instance, Error> {
                Ok($instance(Activation {
                    params: self.0.clone(),
                    ran: 0,
                }))
            }
        }
        impl ActionInstance for $instance {
            fn tick(
                &mut self,
                _: &mut LeafCtx<'_>,
                _: &EntityView<'_>,
                bb: &mut BlackboardAccess<'_>,
                out: &mut Intents<'_>,
            ) -> Result<Status, Error> {
                self.0.tick($mode, bb, out)
            }
            fn halt(&mut self) {
                self.0.ran = 0;
            }
        }
    };
}

action!(
    Count,
    CountInstance,
    "test.action.count",
    r#"{"type":"object","description":"Runs for ticks ticks","properties":{"ticks":{"type":"integer","minimum":1}},"required":["ticks"],"additionalProperties":false}"#,
    "",
    TickMode::Count
);
action!(
    Emit,
    EmitInstance,
    "test.action.emit",
    plain_schema!("Emits a custom intent"),
    "",
    TickMode::Emit
);
action!(
    MarkAction,
    MarkInstance,
    "test.action.mark",
    key_schema!("Sets the key"),
    key_metadata!("bool", "write"),
    TickMode::Mark
);
action!(
    ReadOnlyWrite,
    ReadOnlyWriteInstance,
    "test.action.read_only_write",
    key_schema!("Writes a read-only key"),
    key_metadata!("bool", "read"),
    TickMode::ReadOnlyWrite
);
action!(
    PanicTick,
    PanicTickInstance,
    "test.action.panic_tick",
    plain_schema!("Panics in tick"),
    "",
    TickMode::Panic
);
