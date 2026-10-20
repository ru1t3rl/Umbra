//! AT-1 (plugin row, review m14): removing an entity whose behavior is a plugin behavior destroys
//! the plugin instance at the call to `Kernel::remove`, between ticks, and the behavior itself
//! only when its last owner lets go. Requirements: REQ-C-2, REQ-P-8, ADR 0021, ADR 0028.
//!
//! Uses the fixture plugin's `test.behavior.lifecycle` capability, which logs `create_behavior`,
//! `create_instance`, `destroy_instance` and `destroy_behavior`. The kernel may not use
//! `umbra-plugin-host` (ASM-005), so this lives in `umbra-engine`, which already depends on both.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod plugin_support;

use std::sync::{Arc, Mutex};

use umbra_core::{
    ComponentId, Ecef, FlatTerrain, Intent, KinematicState, Meters, MotionModel, MotionModelId,
    Ownership, PluginId, Quaternion, Registry, Seconds, TerrainProvider, TickRate, Vec3,
};
use umbra_kernel::{
    AgentInit, ClockMode, EntityInit, Kernel, KernelConfig, KernelError, ManualWallClock,
};
use umbra_plugin_host::{HostOptions, LogLevel, PluginHost, PluginLog};

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl PluginLog for Recorder {
    fn log(&self, _: &PluginId, _: LogLevel, message: &str) {
        self.0.lock().expect("log lock").push(message.to_owned());
    }
}

impl Recorder {
    fn count(&self, what: &str) -> usize {
        self.0
            .lock()
            .expect("log lock")
            .iter()
            .filter(|m| *m == what)
            .count()
    }
}

struct Still(MotionModelId);

impl MotionModel for Still {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        state: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        *state
    }
}

fn entity() -> EntityInit {
    EntityInit {
        state: KinematicState {
            position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
            velocity: Vec3::ZERO,
            orientation: Quaternion::IDENTITY,
        },
        ownership: Ownership::Owned,
        motion: Arc::new(Still(
            MotionModelId::new("test.model.still").expect("valid id"),
        )),
        route: None,
    }
}

#[test]
fn remove_plugin_behavior_entity_destroys_its_instance_outside_a_tick() {
    let recorder = Arc::new(Recorder::default());
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    let mut host = PluginHost::new(options);
    let dir = plugin_support::fixture(&[]);
    for pkg in PluginHost::discover(&[dir]).expect("discover the fixture") {
        host.load(&pkg).expect("load the fixture");
    }
    let mut registry = Registry::new();
    host.register(&mut registry).expect("register the fixture");
    let behavior = {
        let id = ComponentId::new("test.behavior.lifecycle").expect("valid id");
        let factory = registry.behavior(&id).expect("the lifecycle behavior");
        let _scope = host.build_scope();
        factory.build(&serde_json::json!({})).expect("build")
    };
    let mut kernel = Kernel::new(
        KernelConfig {
            tick_rate: TickRate::new(30).expect("rate"),
            seed: 1,
            clock: ClockMode::AsFastAsPossible,
        },
        Box::new(FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(ManualWallClock::new()),
    );
    let agent = || AgentInit {
        behavior: Some(Arc::clone(&behavior)),
        ..AgentInit::default()
    };
    let first = kernel.spawn_with(entity(), agent()).expect("spawn");
    let second = kernel.spawn_with(entity(), agent()).expect("spawn");
    kernel.tick().expect("tick");
    assert_eq!(recorder.count("create_instance"), 2);
    assert_eq!(recorder.count("destroy_instance"), 0);

    kernel.remove(first).expect("remove");

    assert_eq!(
        recorder.count("destroy_instance"),
        1,
        "the instance is destroyed by `remove` itself, with no tick running"
    );
    assert_eq!(
        recorder.count("destroy_behavior"),
        0,
        "the behavior lives on"
    );
    kernel.tick().expect("the other entity keeps ticking");
    assert_eq!(kernel.remove(first), Err(KernelError::UnknownEntity(first)));
    kernel.remove(second).expect("remove");
    assert_eq!(recorder.count("destroy_instance"), 2);
    drop(behavior);
    drop(kernel);
    drop(registry);
    assert_eq!(recorder.count("destroy_behavior"), 1);
}
