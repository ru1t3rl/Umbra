//! Plugin declarations of the ABI 0.2 kinds for the SDK descriptor tests (M7 WI-2, ADR 0050): one tiny
//! capability per kind and the plugins built from them, declared through `__private::Plugin` like the
//! M6 rows of `required_minor.rs`. The capabilities do nothing: the descriptor tests never call them
//! (the call-level rows need the ABI 0.2 structs and are written with WI-7/WI-10).
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the declarations"
)]

use umbra_sdk::__private::{Capabilities, Plugin};
use umbra_sdk::{
    Action, ActionInstance, BlackboardAccess, Condition, ConfigureInput, Contacts, EntityView,
    Error, Inbox, Intent, Intents, KeySpace, KinematicState, LeafCtx, MotionCtx, MotionModel,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, NetworkAdapterFactory, SenseCtx, Sensor,
    SimTime, Status, TargetView,
};

pub const MOTION_ID: &str = "test.motion.glide";
pub const SENSOR_ID: &str = "test.sensor.ping";
pub const CONDITION_ID: &str = "test.cond.always";
pub const ACTION_ID: &str = "test.action.once";

pub const MOTION_SCHEMA: &str = r#"{"type":"object","description":"Glide test model"}"#;
pub const SENSOR_SCHEMA: &str = r#"{"type":"object","description":"Ping test sensor"}"#;
pub const CONDITION_SCHEMA: &str = r#"{"type":"object","description":"Always test condition"}"#;
pub const ACTION_SCHEMA: &str = r#"{"type":"object","description":"Once test action"}"#;
pub const CONDITION_METADATA: &str = r#"{"blackboard_params":[{"param":"flag_key","type":"bool","access":"read"}],"result":"always"}"#;
pub const ACTION_METADATA: &str = r#"{"blackboard_params":[{"param":"done_key","type":"bool","access":"write"}],"result":"once"}"#;

pub struct Glide;
impl MotionModel for Glide {
    const ID: &'static str = MOTION_ID;
    const SCHEMA: &'static str = MOTION_SCHEMA;
    fn configure(_: &str) -> Result<Self, Error> {
        Ok(Self)
    }
    fn step(
        &self,
        _: &MotionCtx<'_>,
        state: &KinematicState,
        _: &[Intent<'_>],
        _: f64,
    ) -> KinematicState {
        *state
    }
}

pub struct Ping;
impl Sensor for Ping {
    const ID: &'static str = SENSOR_ID;
    const SCHEMA: &'static str = SENSOR_SCHEMA;
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
        Ok(())
    }
}

pub struct Always;
impl Condition for Always {
    const ID: &'static str = CONDITION_ID;
    const SCHEMA: &'static str = CONDITION_SCHEMA;
    const METADATA: &'static str = CONDITION_METADATA;
    fn configure(_: &str) -> Result<Self, Error> {
        Ok(Self)
    }
    fn check(
        &self,
        _: &mut LeafCtx<'_>,
        _: &EntityView<'_>,
        _: &mut BlackboardAccess<'_>,
    ) -> Result<bool, Error> {
        Ok(true)
    }
}

pub struct Once;
pub struct OnceInstance;
impl Action for Once {
    const ID: &'static str = ACTION_ID;
    const SCHEMA: &'static str = ACTION_SCHEMA;
    const METADATA: &'static str = ACTION_METADATA;
    type Instance = OnceInstance;
    fn configure(_: &str) -> Result<Self, Error> {
        Ok(Self)
    }
    fn instantiate(&self, _: u32) -> Result<OnceInstance, Error> {
        Ok(OnceInstance)
    }
}
impl ActionInstance for OnceInstance {
    fn tick(
        &mut self,
        _: &mut LeafCtx<'_>,
        _: &EntityView<'_>,
        _: &mut BlackboardAccess<'_>,
        _: &mut Intents<'_>,
    ) -> Result<Status, Error> {
        Ok(Status::Success)
    }
    fn halt(&mut self) {}
}

pub struct NullNet;
pub struct NullNetConfig;
pub struct NullNetAdapter;
impl NetworkAdapterFactory for NullNet {
    const ID: &'static str = "test.network.kinds_null";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Config = NullNetConfig;
    fn configure(_: &ConfigureInput) -> Result<NullNetConfig, Error> {
        Ok(NullNetConfig)
    }
}
impl NetworkAdapterConfig for NullNetConfig {
    type Adapter = NullNetAdapter;
    fn key_space(&self) -> KeySpace {
        KeySpace::new(100, 10).expect("valid")
    }
    fn start(&mut self, _: SimTime) -> Result<NullNetAdapter, Error> {
        Ok(NullNetAdapter)
    }
}
impl NetworkAdapter for NullNetAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage<'_>]) -> Result<(), Error> {
        Ok(())
    }
    fn poll(&mut self, _: SimTime, _: &mut Inbox<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn stop(&mut self) {}
}

macro_rules! plugin {
    ($name:ident, $id:literal, |$caps:ident| $body:block) => {
        pub struct $name;
        impl Plugin for $name {
            const ID: &'static str = $id;
            const NAME: &'static str = "kinds test";
            const VERSION: &'static str = "0.1.0";
            fn capabilities($caps: &mut Capabilities) $body
        }
    };
}

plugin!(MotionOnly, "test.plugin.kinds_motion", |caps| {
    caps.motion_model::<Glide>();
});
plugin!(SensorOnly, "test.plugin.kinds_sensor", |caps| {
    caps.sensor::<Ping>();
});
plugin!(ConditionOnly, "test.plugin.kinds_condition", |caps| {
    caps.condition::<Always>();
});
plugin!(ActionOnly, "test.plugin.kinds_action", |caps| {
    caps.action::<Once>();
});
plugin!(AllKinds, "test.plugin.kinds_all", |caps| {
    caps.network_adapter::<NullNet>();
    caps.motion_model::<Glide>();
    caps.sensor::<Ping>();
    caps.condition::<Always>();
    caps.action::<Once>();
});
