//! Shared helpers for the M3 `cgf-engine` behavior tests (new in M3; the M2 helpers in
//! `tests/common/mod.rs` are reused unchanged through `crate::common`).
//!
//! The path and loader helpers repeat the M2 ones (`common::example_dir`, `load_example`,
//! `instantiate_with`) because those are fixed to the walking skeleton and the built-in registry
//! and `tests/common` may not be edited (A-001); only `PAUSE_AT`, `options`, `Trace` and the run
//! helpers are shared.
//!
//! Everything here goes through the standard registry (`standard_registry()`: the M2 built-ins,
//! the built-in leaves, `builtin.visual` and the built-in graphs) and the scenario in
//! `examples/scenarios/patrol_and_engage`.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;
use std::path::{Path, PathBuf};

use cgf_core::{EntityId, Intent, Registry};
use cgf_engine::builtins::standard_registry;
use cgf_engine::scenario::{ScenarioRun, instantiate, load_scenario};
use cgf_engine::{ClockMode, Engine, EntitySnapshot, Snapshot};
use cgf_templates::Scenario;

use super::common::{self, Trace};

/// Ticks of the determinism runs: 50 s at 30 Hz, covering first contact, the approach and the
/// first `Fire` of the example (WI-16 records the exact ticks; the tests bound them generously).
pub const TICKS: u64 = 1500;
/// The scenario's own length in ticks (140 s at 30 Hz).
pub const FULL: u64 = 4200;

pub fn standard() -> Registry {
    standard_registry().expect("the standard registry builds")
}

/// `examples/scenarios/patrol_and_engage`.
pub fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/patrol_and_engage")
}

pub fn fixture_dir(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

pub fn load(dir: &Path) -> Scenario {
    load_scenario(&standard(), &dir.join("scenario.toml"))
        .unwrap_or_else(|e| panic!("scenario in {}: {e}", dir.display()))
}

pub fn load_example() -> Scenario {
    load(&example_dir())
}

/// The example's template and graph files as in-memory sources, relative to the example directory,
/// in path order.
pub fn template_and_graph_sources() -> Vec<cgf_templates::SourceFile> {
    let dir = example_dir();
    let mut paths = Vec::new();
    for sub in ["templates", "behaviors"] {
        let mut names: Vec<PathBuf> = std::fs::read_dir(dir.join(sub))
            .expect("example directory")
            .map(|e| e.expect("entry").path())
            .collect();
        names.sort();
        paths.extend(names);
    }
    paths
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&dir)
                .expect("under the example")
                .to_path_buf();
            cgf_templates::SourceFile::new(rel, std::fs::read_to_string(&p).expect("readable"))
        })
        .collect()
}

/// Instantiates through the standard registry on a manual wall clock that advances `read_cost` per
/// read (different costs are different wall-time scripts for the same simulation).
pub fn instantiate_with(
    scenario: &Scenario,
    read_cost: Duration,
    seed: Option<u64>,
    clock: Option<ClockMode>,
) -> ScenarioRun {
    instantiate(
        &standard(),
        scenario,
        common::options(read_cost, seed, clock),
    )
    .unwrap_or_else(|e| panic!("instantiate: {e}"))
}

pub fn example_run() -> ScenarioRun {
    instantiate_with(&load_example(), Duration::from_micros(500), None, None)
}

pub fn example_engine(seed: Option<u64>, clock: Option<ClockMode>) -> Engine {
    instantiate_with(&load_example(), Duration::from_micros(500), seed, clock).engine
}

/// The ID of the entity named `name` in the run.
pub fn id_of(run: &ScenarioRun, name: &str) -> EntityId {
    let Some(found) = run
        .entities
        .iter()
        .find(|e| e.name.as_deref() == Some(name))
    else {
        panic!("no entity named {name}");
    };
    found.id
}

/// The entity `id` of a snapshot.
pub fn entity(snapshot: &Snapshot, id: EntityId) -> &EntitySnapshot {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no entity {id} in the snapshot"))
}

/// The targets of all `Fire` intents of the entity in this snapshot.
pub fn fire_targets(e: &EntitySnapshot) -> Vec<EntityId> {
    e.intents
        .iter()
        .filter_map(|i| match i {
            Intent::Fire { target, .. } => Some(*target),
            _ => None,
        })
        .collect()
}

/// The first tick (1-based, the label of the snapshot) at which `pick` holds for entity `id`.
pub fn first_tick(
    trace: &Trace,
    id: EntityId,
    pick: impl Fn(&EntitySnapshot) -> bool,
) -> Option<usize> {
    trace
        .snapshots
        .iter()
        .position(|s| pick(entity(s, id)))
        .map(|i| i + 1)
}

/// Runs the engine to `ticks` and returns the per-tick trace.
pub fn run_trace(engine: Engine, ticks: u64) -> Trace {
    common::run_to(engine, ticks, false)
}

/// The first difference between two behavior-side entity snapshots: `(field, left, right)`.
fn behavior_diff(a: &EntitySnapshot, b: &EntitySnapshot) -> Option<(String, String, String)> {
    if let Some(d) = common::entity_diff(a, b) {
        return Some(d);
    }
    let fields: [(&str, String, String); 5] = [
        (
            "intents",
            format!("{:?}", a.intents),
            format!("{:?}", b.intents),
        ),
        (
            "contacts",
            format!("{:?}", a.contacts),
            format!("{:?}", b.contacts),
        ),
        (
            "status",
            format!("{:?}", a.status),
            format!("{:?}", b.status),
        ),
        (
            "behavior",
            format!("{:?}", a.behavior),
            format!("{:?}", b.behavior),
        ),
        ("force", format!("{:?}", a.force), format!("{:?}", b.force)),
    ];
    fields
        .into_iter()
        .find(|(_, x, y)| x != y)
        .map(|(f, x, y)| (f.to_string(), x, y))
}

/// Panics at the first divergent tick, naming the entity and the field: state, route, intents,
/// contacts, status. Behavior state is not part of the v2 hash (ASM-081), so intents and contacts
/// are compared tick by tick as well as the hashes (REQ-K-4).
pub fn assert_same_behavior_trace(label: &str, got: &Trace, want: &Trace) {
    assert_eq!(
        got.hashes.len(),
        want.hashes.len(),
        "{label}: number of ticks"
    );
    for (i, ((gh, wh), (gs, ws))) in got
        .hashes
        .iter()
        .zip(&want.hashes)
        .zip(got.snapshots.iter().zip(&want.snapshots))
        .enumerate()
    {
        assert_eq!(
            gs.entities.len(),
            ws.entities.len(),
            "{label}: entity count at tick {}",
            i + 1
        );
        for (a, b) in gs.entities.iter().zip(&ws.entities) {
            if let Some((field, x, y)) = behavior_diff(a, b) {
                panic!(
                    "{label}: first divergence at tick {}, entity {}, field {field}: {x} vs {y}",
                    i + 1,
                    a.id
                );
            }
        }
        assert_eq!(
            gh,
            wh,
            "{label}: state hash differs at tick {} although no snapshot field does",
            i + 1
        );
    }
}
