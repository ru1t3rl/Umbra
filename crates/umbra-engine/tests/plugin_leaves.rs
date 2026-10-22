//! AT-3 and AT-4 (M7, condition and action rows): the example plugins `example-bearing`
//! (`example.cond.bearing_within`) and `example-dispatch` (`example.action.dispatch`) used as leaves of
//! behavior graphs through the engine, as real libraries. Requirements: Acceptance 1, REQ-P-4,
//! REQ-SDK-3, REQ-B-3, REQ-B-15, REQ-B-17, ADR 0050, ADR 0052, ADR 0056, ASM-643, ASM-649, ASM-669.
//!
//! Contracts (the crate docs of `examples/plugins/bearing` and `dispatch` are the source):
//! - `bearing_within`: Success iff the entity in the key `target_key` is a current contact within
//!   `max_bearing_deg` of the heading; Failure when unset or not a contact;
//! - `dispatch`: Running for `ceil(delay_s * hz)` ticks from its first tick; on the next tick a
//!   `Spawn` `offset_forward_m` ahead, `done_key = true`, Success; `halt` resets it.
//!
//! AT-3 scene: four cells 20 km apart (the 2 km sensors never see across), each a blue `guard` at
//! heading north with a red target. A guard's graph is a sequence `enemy_in_range -> bearing_within ->
//! wait`, so it emits `Hold` on a tick exactly when the bearing condition holds.
//!
//! Step 4 of M7 (WI-2): ignored until WI-18 (it needs WI-4 for the spawns and WI-7/9/10 for ABI 0.2).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod kinds_support;
mod plugin_support;

use core::fmt::Write as _;
use core::ops::ControlFlow;
use core::time::Duration;
use std::path::PathBuf;
use std::sync::LazyLock;

use common::options;
use kinds_support::{ProbeLog, example_plugins, lat, lon, registry_with_probe, write_dir};
use umbra_core::{
    Access, BbType, BlackboardParam, ComponentId, Intent, Provenance, SimEvent, TemplateId,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::{
    HostOptions, RunUntil, instantiate_with_plugins, load_plugins, load_scenario_with_plugins,
    validate_path_with_plugins,
};

static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| example_plugins(&["bearing", "dispatch"]));

const GROUND: &str = "model = \"builtin.ground.kinematic\"\n\
    params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }\n";

fn template(id: &str, extra: &str) -> String {
    format!("schema_version = 1\nid = \"{id}\"\n\n[motion]\n{GROUND}\n{extra}")
}

fn base_scenario(duration_s: f64, entities: &str) -> String {
    format!(
        r#"schema_version = 1

[scenario]
name = "plugin_leaves"
seed = 7
tick_rate_hz = 30
clock = "afap"
duration_s = {duration_s:?}
template_paths = ["templates", "behaviors"]

[terrain]
provider = "builtin.terrain.flat"
params = {{ height_m = 0.0 }}
{entities}"#
    )
}

// ------------------------------------------------------------------------------------- AT-3

const GUARD_GRAPH: &str = r#"schema_version = 1
id = "example.behavior.guard"
version = "1.0.0"
description = "Hold while the nearest hostile is within a bearing of the heading"
root = "main"

[interface]
params = { max_bearing_deg = { type = "number", exclusiveMinimum = 0, default = 30.0 } }

[blackboard]
target = "entity"

[nodes.main]
type = "sequence"
children = ["enemy", "aimed", "hold"]

[nodes.enemy]
type = "condition"
ref = "builtin.cond.enemy_in_range"
params = { range_m = 2000.0, store_as = "target" }

[nodes.aimed]
type = "condition"
ref = "example.cond.bearing_within"
params = { target_key = "target", max_bearing_deg = { param = "max_bearing_deg" } }

[nodes.hold]
type = "action"
ref = "builtin.action.wait"
params = { duration_s = 100.0 }
"#;

/// One guard cell: a guard and its target, 20 km from the other cells.
struct Cell {
    name: &'static str,
    /// Uses the template with `max_bearing_deg = 60` instead of the default 30.
    wide: bool,
    /// East offset of the cell, metres.
    east_m: f64,
    /// The target's offset from the guard: north and east, metres.
    target_m: (f64, f64),
    /// Whether the target is inside the guard's bearing limit.
    holds: bool,
}

const CELLS: [Cell; 4] = [
    Cell {
        name: "guard-wide-miss",
        wide: false,
        east_m: 0.0,
        target_m: (500.0, 500.0),
        holds: false,
    },
    Cell {
        name: "guard-wide-hit",
        wide: true,
        east_m: 20_000.0,
        target_m: (500.0, 500.0),
        holds: true,
    },
    Cell {
        name: "guard-ahead",
        wide: false,
        east_m: 40_000.0,
        target_m: (500.0, 88.0),
        holds: true,
    },
    Cell {
        name: "guard-behind",
        wide: false,
        east_m: 60_000.0,
        target_m: (-500.0, 0.0),
        holds: false,
    },
];

fn guard_scene() -> PathBuf {
    let mut entities = String::new();
    for cell in &CELLS {
        let template = if cell.wide {
            "example.guard.wide"
        } else {
            "example.guard.narrow"
        };
        write!(
            entities,
            "\n[[entities]]\ntemplate = \"{template}\"\nname = \"{name}\"\nforce = \"blue\"\n\
             position = {{ lat_deg = {:.7}, lon_deg = {:.7} }}\nheading_deg = 0.0\n\n\
             [[entities]]\ntemplate = \"example.guard.target\"\nname = \"{name}-target\"\n\
             force = \"red\"\nposition = {{ lat_deg = {:.7}, lon_deg = {:.7} }}\n",
            lat(0.0),
            lon(cell.east_m),
            lat(cell.target_m.0),
            lon(cell.east_m + cell.target_m.1),
            name = cell.name,
        )
        .expect("write to a String");
    }
    let sensor = "\n[[sensors]]\nkind = \"builtin.visual\"\nparams = { range_m = 2000.0, fov_deg = 360.0 }\n";
    let narrow = template(
        "example.guard.narrow",
        &format!("[behavior]\nref = \"example.behavior.guard\"\n{sensor}"),
    );
    let wide = template(
        "example.guard.wide",
        &format!(
            "[behavior]\nref = \"example.behavior.guard\"\nparams = {{ max_bearing_deg = 60.0 }}\n{sensor}"
        ),
    );
    let target = template("example.guard.target", "");
    write_dir(
        "guards",
        &[
            ("scenario.toml", base_scenario(2.0, &entities).as_str()),
            ("templates/narrow.toml", narrow.as_str()),
            ("templates/wide.toml", wide.as_str()),
            ("templates/target.toml", target.as_str()),
            ("behaviors/guard.behavior.toml", GUARD_GRAPH),
        ],
    )
    .join("scenario.toml")
}

/// AT-3: the guards hold exactly when the target is within their bearing limit, from the first tick
/// to the last, so the condition reads the blackboard target and the heading.
#[test]
#[ignore = "M7 WI-18"]
fn bearing_condition_reads_blackboard_target() {
    let loaded = load_scenario_with_plugins(
        &standard_registry().expect("registry"),
        &guard_scene(),
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("load the guard scene");
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let ticks = run.duration_ticks;
    let guards: Vec<(umbra_core::EntityId, bool)> = CELLS
        .iter()
        .map(|cell| {
            let id = run
                .entities
                .iter()
                .find(|e| e.name.as_deref() == Some(cell.name))
                .expect("a guard")
                .id;
            (id, cell.holds)
        })
        .collect();
    let mut engine = run.engine;
    let mut holding: Vec<Vec<bool>> = vec![Vec::new(); guards.len()];

    engine
        .run(RunUntil::Ticks(ticks), |view| {
            let snapshot = view.snapshot();
            for (i, (id, _)) in guards.iter().enumerate() {
                let entity = snapshot
                    .entities
                    .iter()
                    .find(|e| e.id == *id)
                    .expect("guard");
                holding[i].push(entity.intents.contains(&Intent::Hold));
            }
            ControlFlow::Continue(())
        })
        .expect("run");

    for ((_, expected), seen) in guards.iter().zip(&holding) {
        assert_eq!(seen.len() as u64, ticks);
        assert!(
            seen.iter().all(|h| h == expected),
            "expected hold = {expected} on every tick, saw {seen:?}"
        );
    }
}

/// REQ-B-17, ASM-643: the plugin leaves are in the catalog with their blackboard params.
#[test]
#[ignore = "M7 WI-18"]
fn plugin_leaves_are_listed_with_their_blackboard_params() {
    let (registry, _host) = load_plugins(
        &standard_registry().expect("registry"),
        std::slice::from_ref(&*PLUGINS),
        HostOptions::default(),
    )
    .expect("load the plugins");

    let bearing = registry
        .info(&ComponentId::new("example.cond.bearing_within").expect("id"))
        .expect("registered");
    let dispatch = registry
        .info(&ComponentId::new("example.action.dispatch").expect("id"))
        .expect("registered");

    assert_eq!(bearing.source, Provenance::Plugin);
    assert_eq!(
        bearing.blackboard_params,
        [BlackboardParam::new(
            "target_key",
            BbType::Entity,
            Access::Read
        )]
    );
    assert_eq!(dispatch.source, Provenance::Plugin);
    assert_eq!(
        dispatch.blackboard_params,
        [BlackboardParam::new(
            "done_key",
            BbType::Bool,
            Access::Write
        )]
    );
}

/// REQ-T-3: a graph whose plugin leaf lacks a required param is a validation error at the node.
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_without_done_key_is_a_validation_error() {
    let graph = DISPATCH_GRAPH.replace(", done_key = \"done\"", "");
    let path = dispatch_scene(&graph, 1.0);

    let report = validate_path_with_plugins(
        &registry_with_probe().0,
        &path,
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("validate");

    let codes: Vec<String> = report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_owned())
        .collect();
    assert_eq!(codes, ["missing_param"]);
}

// ------------------------------------------------------------------------------------- AT-4

const DISPATCH_GRAPH: &str = r#"schema_version = 1
id = "example.behavior.dispatcher"
version = "1.0.0"
description = "Spawn a child after a delay, then probe the done flag"
root = "main"

[blackboard]
done = "bool"

[nodes.main]
type = "sequence"
children = ["dispatch", "probe"]

[nodes.dispatch]
type = "action"
ref = "example.action.dispatch"
params = { template = "example.dispatch.child", done_key = "done", delay_s = 1.0, offset_forward_m = 20.0 }

[nodes.probe]
type = "condition"
ref = "test.cond.probe"
params = { key = "done" }
"#;

/// The same, but the dispatch node sits under a 0.5 s `timeout`, which halts it before its 1 s delay.
const TIMED_GRAPH: &str = r#"schema_version = 1
id = "example.behavior.dispatcher"
version = "1.0.0"
description = "Dispatch under a timeout shorter than its delay"
root = "limit"

[blackboard]
done = "bool"

[nodes.limit]
type = "timeout"
duration_s = 0.5
child = "dispatch"

[nodes.dispatch]
type = "action"
ref = "example.action.dispatch"
params = { template = "example.dispatch.child", done_key = "done", delay_s = 1.0, offset_forward_m = 20.0 }
"#;

fn dispatch_scene(graph: &str, duration_s: f64) -> PathBuf {
    let entities = format!(
        "\n[[entities]]\ntemplate = \"example.dispatch.parent\"\nname = \"dispatcher\"\n\
         position = {{ lat_deg = {:.7}, lon_deg = {:.7} }}\nheading_deg = 0.0\n",
        lat(0.0),
        lon(0.0)
    );
    let parent = template(
        "example.dispatch.parent",
        "[behavior]\nref = \"example.behavior.dispatcher\"\n",
    );
    let child = template("example.dispatch.child", "");
    write_dir(
        "dispatch",
        &[
            (
                "scenario.toml",
                base_scenario(duration_s, &entities).as_str(),
            ),
            ("templates/parent.toml", parent.as_str()),
            ("templates/child.toml", child.as_str()),
            ("behaviors/dispatcher.behavior.toml", graph),
        ],
    )
    .join("scenario.toml")
}

struct Dispatched {
    /// Entity count after each tick.
    counts: Vec<usize>,
    /// The events of each tick.
    events: Vec<Vec<SimEvent>>,
    /// The dispatcher and the other entity's positions after the last tick, if there is one.
    positions: Option<(umbra_core::Ecef, umbra_core::Ecef)>,
    probe: ProbeLog,
}

fn dispatch(graph: &str, ticks: u64) -> Dispatched {
    let (registry, probe) = registry_with_probe();
    let path = dispatch_scene(graph, 5.0);
    let loaded = load_scenario_with_plugins(&registry, &path, std::slice::from_ref(&*PLUGINS))
        .expect("load the dispatch scene");
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let dispatcher = run.entities[0].id;
    let mut engine = run.engine;
    let (mut counts, mut events, mut positions) = (Vec::new(), Vec::new(), None);
    engine
        .run(RunUntil::Ticks(ticks), |view| {
            let snapshot = view.snapshot();
            counts.push(snapshot.entities.len());
            events.push(view.report.events.clone());
            let me = snapshot.entities.iter().find(|e| e.id == dispatcher);
            let other = snapshot.entities.iter().find(|e| e.id != dispatcher);
            positions = me
                .zip(other)
                .map(|(a, b)| (a.state.position, b.state.position));
            ControlFlow::Continue(())
        })
        .expect("run");
    Dispatched {
        counts,
        events,
        positions,
        probe,
    }
}

/// AT-4: nothing happens during the 1 s delay (30 ticks), the spawn is applied after the tick on which
/// the action succeeds (tick 31, snapshot index 30), and the entity count stays at two to the end of
/// the 40 ticks.
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_action_spawns_after_delay_and_resets_on_halt() {
    let flight = dispatch(DISPATCH_GRAPH, 40);

    let first_spawned = flight.counts.iter().position(|&n| n == 2);
    assert_eq!(first_spawned, Some(30));
    assert!(flight.counts[..30].iter().all(|&n| n == 1));
    assert!(flight.counts[30..].iter().all(|&n| n == 2));

    // The same action under a 0.5 s `timeout` is halted before its 1 s delay, every time: it resets.
    let halted = dispatch(TIMED_GRAPH, 120);
    assert!(halted.counts.iter().all(|&n| n == 1), "{:?}", halted.counts);
}

/// The spawn lands `offset_forward_m` (20 m) ahead of the dispatcher, which faces north.
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_spawns_the_child_twenty_meters_ahead_of_the_entity() {
    let flight = dispatch(DISPATCH_GRAPH, 40);

    let (dispatcher, child) = flight.positions.expect("both entities exist");
    let ahead = child.to_geodetic().lat().0 - dispatcher.to_geodetic().lat().0;
    assert!(ahead > 0.0, "north of the dispatcher");
    assert!(
        ((child - dispatcher).norm() - 20.0).abs() < 0.5,
        "{} m",
        (child - dispatcher).norm()
    );
}

/// ADR 0052: the spawn is reported as `EntityCreated` of the template on the tick it is applied
/// (and nowhere else).
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_spawn_is_reported_as_an_entity_created_event_of_the_template() {
    let flight = dispatch(DISPATCH_GRAPH, 40);

    let template = TemplateId::new("example.dispatch.child").expect("id");
    let created: Vec<usize> = flight
        .events
        .iter()
        .enumerate()
        .filter(|(_, events)| {
            events.iter().any(|e| {
                matches!(e, SimEvent::EntityCreated { template: Some(t), .. } if *t == template)
            })
        })
        .map(|(tick, _)| tick)
        .collect();
    assert_eq!(created, [30]);
}

/// The action writes `done_key` on the tick it succeeds, and the sequence only reaches the probe then:
/// the probe saw the flag once, set.
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_writes_its_done_key_when_it_succeeds() {
    let flight = dispatch(DISPATCH_GRAPH, 40);

    let seen = flight.probe.lock().expect("probe log").clone();
    assert_eq!(seen, [Some(true)]);
}

/// REQ-B-3: a `timeout` that halts the action before its delay resets it every time, so it never
/// spawns.
#[test]
#[ignore = "M7 WI-18"]
fn dispatch_under_a_shorter_timeout_never_spawns() {
    let flight = dispatch(TIMED_GRAPH, 120);

    assert!(flight.counts.iter().all(|&n| n == 1), "{:?}", flight.counts);
    assert!(flight.probe.lock().expect("probe log").is_empty());
}
