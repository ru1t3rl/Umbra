//! AT-7 (engine row): two entities that use one graph keep their own contacts, node state and
//! blackboard.
//! Requirements: REQ-B-5, REQ-B-11, REQ-K-5, ADR 0014.
//!
//! Step 4 of M3: written before WI-15; runs since WI-15 (the scenario instantiation with
//! behaviors and sensors).

#![forbid(unsafe_code)]

mod behavior_support;
mod common;

use core::time::Duration;

use behavior_support::{TICKS, entity, fire_targets, id_of, standard, template_and_graph_sources};
use common::run_to;
use umbra_core::{Intent, MetersPerSecond};
use umbra_engine::scenario::instantiate;
use umbra_templates::{Loader, SourceFile};

/// Two blue tanks of one template (so one graph): blue-near starts where the example's blue-1 does,
/// blue-far 9 km north of it; red-1 is the example's truck.
const SCENARIO: &str = r#"
schema_version = 1
[scenario]
name = "two_blue_one_graph"
seed = 7
tick_rate_hz = 30
clock = "afap"
duration_s = 50.0

[[entities]]
template = "example.tank.blue"
name = "blue-near"
force = "blue"
position = { lat_deg = 52.0, lon_deg = 4.0 }
heading_deg = 90.0
speed_mps = 10.0
[entities.route]
arrival_radius_m = 5.0
cruise_speed_mps = 10.0
waypoints = [ { lat_deg = 52.0, lon_deg = 4.0007280 }, { lat_deg = 52.0, lon_deg = 4.0116475 } ]

[[entities]]
template = "example.tank.blue"
name = "blue-far"
force = "blue"
position = { lat_deg = 52.08, lon_deg = 4.0 }
heading_deg = 90.0
speed_mps = 10.0
[entities.route]
arrival_radius_m = 5.0
cruise_speed_mps = 10.0
waypoints = [ { lat_deg = 52.08, lon_deg = 4.0007280 }, { lat_deg = 52.08, lon_deg = 4.0116475 } ]

[[entities]]
template = "example.truck.red"
name = "red-1"
force = "red"
position = { lat_deg = 52.0017975, lon_deg = 4.0218391 }
heading_deg = 270.0
speed_mps = 20.0
[entities.route]
arrival_radius_m = 5.0
cruise_speed_mps = 20.0
waypoints = [ { lat_deg = 52.0017975, lon_deg = 3.8689651 } ]
"#;

/// REQ-B-11, REQ-B-5: the same graph on two entities gives two independent behaviors: the near
/// tank sees red, engages and fires; the far tank, whose contacts stay empty, never leaves its
/// patrol, whatever the near tank's graph state is doing.
///
/// What this proves: behaviors and contacts are per entity through the whole stack (the far tank
/// is not pulled into `engage` by the near one). It does NOT prove blackboard or node-state
/// isolation, which no observable here depends on (the far tank's `enemy_in_range` fails before
/// the shared key is read); that is proven by `umbra-behavior/tests/isolation.rs`
/// (`two_entities_with_one_graph_share_no_node_state`).
#[test]
fn two_entities_one_graph_diverge_by_their_own_contacts() {
    let registry = standard();
    let scenario = Loader::new(&registry)
        .expect("loader")
        .scenario_from_sources(
            &SourceFile::new("scenario.toml", SCENARIO),
            &template_and_graph_sources(),
        )
        .unwrap_or_else(|e| panic!("scenario: {e}"));
    let run = instantiate(
        &registry,
        &scenario,
        common::options(Duration::from_micros(500), None, None),
    )
    .expect("instantiate");
    let (near, far, red) = (
        id_of(&run, "blue-near"),
        id_of(&run, "blue-far"),
        id_of(&run, "red-1"),
    );
    let trace = run_to(run.engine, TICKS, false);

    let near_fires = trace
        .snapshots
        .iter()
        .any(|s| fire_targets(entity(s, near)).contains(&red));
    let far_contacts = trace
        .snapshots
        .iter()
        .any(|s| !entity(s, far).contacts.is_empty());
    let far_fires = trace
        .snapshots
        .iter()
        .any(|s| !fire_targets(entity(s, far)).is_empty());

    assert!(near_fires, "the near tank engages red");
    assert!(
        !far_contacts && !far_fires,
        "the far tank never sees red and never fires"
    );
    for s in &trace.snapshots {
        let far = entity(s, far);
        assert!(
            matches!(far.intents.first(), Some(Intent::SetSpeed(MetersPerSecond(v))) if (*v - 10.0).abs() < 1e-9),
            "the far tank patrols on every tick: {:?}",
            far.intents
        );
        assert_eq!(
            far.behavior.as_ref().map(umbra_core::BehaviorId::as_str),
            Some("example.behavior.patrol_and_engage")
        );
    }
}
