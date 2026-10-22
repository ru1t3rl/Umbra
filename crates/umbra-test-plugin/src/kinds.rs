//! The ABI 0.2 capabilities of the fixture (cargo feature `kinds`, M7 WI-2, ADR 0050, ASM-668): the
//! crate then is plugin `test.plugin.kinds` 0.1.0 with only the capabilities below, built on `umbra-sdk`
//! only. They are the fixture of the gate-13 rows of the four new kinds and of the host bridge tests.
//!
//! Status: M7 WI-2 skeleton. The ids, params and metadata are the contract the tests assume; the
//! bodies are implemented in WI-10 (they panic with `M7 WI-10` until then, and the SDK's
//! `Capabilities::{motion_model, sensor, condition, action}` stubs panic first).
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

use umbra_sdk::{
    Action, ActionInstance, BlackboardAccess, Condition, Contacts, EntityView, Error, Intent,
    Intents, KinematicState, LeafCtx, MotionCtx, MotionModel, SenseCtx, Sensor, Status, TargetView,
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

macro_rules! motion {
    ($name:ident, $id:literal, $desc:literal) => {
        pub(crate) struct $name;
        impl MotionModel for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = plain_schema!($desc);
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn step(
                &self,
                _: &MotionCtx<'_>,
                _: &KinematicState,
                _: &[Intent<'_>],
                _: f64,
            ) -> KinematicState {
                unimplemented!("M7 WI-10")
            }
        }
    };
}

macro_rules! sensor {
    ($name:ident, $id:literal, $desc:literal) => {
        pub(crate) struct $name;
        impl Sensor for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = plain_schema!($desc);
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn sense(
                &self,
                _: &mut SenseCtx<'_>,
                _: &TargetView<'_>,
                _: &[TargetView<'_>],
                _: &mut Contacts<'_>,
            ) -> Result<(), Error> {
                unimplemented!("M7 WI-10")
            }
        }
    };
}

macro_rules! condition {
    ($name:ident, $id:literal, $schema:expr, $metadata:expr) => {
        pub(crate) struct $name;
        impl Condition for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = $schema;
            const METADATA: &'static str = $metadata;
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn check(
                &self,
                _: &mut LeafCtx<'_>,
                _: &EntityView<'_>,
                _: &mut BlackboardAccess<'_>,
            ) -> Result<bool, Error> {
                unimplemented!("M7 WI-10")
            }
        }
    };
}

/// An action and its instance type.
macro_rules! action {
    ($name:ident, $instance:ident, $id:literal, $schema:expr, $metadata:expr) => {
        pub(crate) struct $name;
        pub(crate) struct $instance;
        impl Action for $name {
            const ID: &'static str = $id;
            const SCHEMA: &'static str = $schema;
            const METADATA: &'static str = $metadata;
            type Instance = $instance;
            fn configure(_: &str) -> Result<Self, Error> {
                Ok(Self)
            }
            fn instantiate(&self, _: u32) -> Result<$instance, Error> {
                Ok($instance)
            }
        }
        impl ActionInstance for $instance {
            fn tick(
                &mut self,
                _: &mut LeafCtx<'_>,
                _: &EntityView<'_>,
                _: &mut BlackboardAccess<'_>,
                _: &mut Intents<'_>,
            ) -> Result<Status, Error> {
                unimplemented!("M7 WI-10")
            }
            fn halt(&mut self) {}
        }
    };
}

motion!(Coast, "test.motion.coast", "Coasts along its velocity");
motion!(
    TerrainEcho,
    "test.motion.terrain_echo",
    "Echoes the terrain"
);
motion!(
    NonUnit,
    "test.motion.non_unit",
    "Returns a non-unit orientation"
);
motion!(Nan, "test.motion.nan", "Returns a NaN position");
motion!(PanicStep, "test.motion.panic_step", "Panics in step");

sensor!(All, "test.sensor.all", "Contacts every candidate");
sensor!(
    Los,
    "test.sensor.los",
    "Contacts candidates in line of sight"
);
sensor!(Coin, "test.sensor.coin", "Contacts candidates by coin flip");
sensor!(
    UnknownContact,
    "test.sensor.unknown_contact",
    "Contacts a non-candidate"
);
sensor!(PanicSense, "test.sensor.panic_sense", "Panics in sense");

condition!(Always, "test.cond.always", plain_schema!("True"), "");
condition!(
    Flag,
    "test.cond.flag",
    key_schema!("True iff the key is true"),
    key_metadata!("bool", "read")
);
condition!(
    Mark,
    "test.cond.mark",
    key_schema!("Sets the key"),
    key_metadata!("bool", "write")
);
condition!(
    Undeclared,
    "test.cond.undeclared",
    plain_schema!("Reads an undeclared key"),
    ""
);
condition!(
    WrongType,
    "test.cond.wrong_type",
    key_schema!("Writes a number to a bool key"),
    key_metadata!("bool", "write")
);
condition!(
    NonFinite,
    "test.cond.non_finite",
    key_schema!("Writes NaN to a number key"),
    key_metadata!("number", "write")
);
condition!(
    PanicCheck,
    "test.cond.panic_check",
    plain_schema!("Panics in check"),
    ""
);

action!(
    Count,
    CountInstance,
    "test.action.count",
    r#"{"type":"object","description":"Runs for ticks ticks","properties":{"ticks":{"type":"integer","minimum":1}},"required":["ticks"],"additionalProperties":false}"#,
    ""
);
action!(
    Emit,
    EmitInstance,
    "test.action.emit",
    plain_schema!("Emits a custom intent"),
    ""
);
action!(
    MarkAction,
    MarkInstance,
    "test.action.mark",
    key_schema!("Sets the key"),
    key_metadata!("bool", "write")
);
action!(
    ReadOnlyWrite,
    ReadOnlyWriteInstance,
    "test.action.read_only_write",
    key_schema!("Writes a read-only key"),
    key_metadata!("bool", "read")
);
action!(
    PanicTick,
    PanicTickInstance,
    "test.action.panic_tick",
    plain_schema!("Panics in tick"),
    ""
);
