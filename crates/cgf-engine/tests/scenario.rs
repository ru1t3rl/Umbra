//! AT-7 (engine rows), AT-3 (terrain/clock bounds) and REQ-S-2: loading and instantiating
//! scenarios through the built-in registry. Requirements: REQ-T-1, REQ-S-2, REQ-C-3, REQ-C-6,
//! REQ-M-4, ASM-044, ASM-059, ASM-060, ASM-062.
//!
//! Components are built by ID through the registry (the engine never names a concrete motion
//! model type). Scenarios under test are mostly in-memory sources; the example scenario is loaded
//! from its files.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::time::Duration;

use cgf_core::{Geodetic, TerrainProvider};
use cgf_engine::scenario::{ScenarioError, instantiate, load_scenario, validate_path};
use cgf_engine::{ClockMode, RunUntil, TimeScale};
use cgf_templates::{ClockSetting, Loader, Scenario, SourceFile};
use common::{
    example_dir, example_path, fixture_path, instantiate_with, load_example, registry, run_silent,
};

const TEMPLATE: &str = "schema_version = 1\nid = \"ground.base\"\n[motion]\nmodel = \"builtin.ground.kinematic\"\nparams = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }\n";

fn scenario_text(terrain: &str, clock: &str, entities: &str) -> String {
    format!(
        "schema_version = 1\n\n[scenario]\nname = \"inline\"\nseed = 42\ntick_rate_hz = 30\n{clock}\nduration_s = 60.0\ntemplate_paths = [\"templates\"]\n\n{terrain}\n{entities}"
    )
}

fn from_text(text: &str) -> Scenario {
    let registry = registry();
    let loader = Loader::new(&registry).expect("loader");
    loader
        .scenario_from_sources(
            &SourceFile::new("inline/scenario.toml", text),
            &[SourceFile::new("inline/templates/ground.toml", TEMPLATE)],
        )
        .unwrap_or_else(|d| {
            panic!(
                "{}",
                d.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

fn entity(position: &str, extra: &str) -> String {
    format!(
        "[[entities]]\ntemplate = \"ground.base\"\nposition = {position}\nheading_deg = 90.0\n{extra}\n"
    )
}

fn run_with(scenario: &Scenario) -> cgf_engine::scenario::ScenarioRun {
    instantiate_with(scenario, Duration::ZERO, None, None)
}

/// ASM-044, REQ-K-4: the same point given as lat/lon/alt or as ECEF in a file gives identical state
/// hashes, before and after running.
#[test]
fn given_same_point_by_ecef_and_by_geodetic_in_files_then_state_hashes_equal() {
    let ecef = Geodetic::from_degrees(52.3, 4.9, 120.0)
        .expect("geodetic")
        .to_ecef();
    let route = "[entities.route]\narrival_radius_m = 5.0\ncruise_speed_mps = 12.0\nwaypoints = [{ lat_deg = 52.3, lon_deg = 4.91 }]\n";
    let by_geodetic = scenario_text(
        "",
        "clock = \"afap\"",
        &entity("{ lat_deg = 52.3, lon_deg = 4.9, alt_m = 120.0 }", route),
    );
    let by_ecef = scenario_text(
        "",
        "clock = \"afap\"",
        &entity(
            &format!(
                "{{ ecef_m = [{:?}, {:?}, {:?}] }}",
                ecef.0.x, ecef.0.y, ecef.0.z
            ),
            route,
        ),
    );
    let (mut a, mut b) = (
        run_with(&from_text(&by_geodetic)).engine,
        run_with(&from_text(&by_ecef)).engine,
    );
    assert_eq!(a.state_hash(), b.state_hash(), "initial hashes");

    run_silent(&mut a, RunUntil::Ticks(120));
    run_silent(&mut b, RunUntil::Ticks(120));

    assert_eq!(a.state_hash(), b.state_hash(), "hashes after 120 ticks");
}

/// ASM-059: entities spawn in file order; ids follow.
#[test]
fn instantiate_spawns_in_file_order_with_ids_from_one() {
    let scenario = load_example();

    let run = run_with(&scenario);

    let ids: Vec<u64> = run.entities.iter().map(|e| e.id.get()).collect();
    assert_eq!(ids, [1, 2, 3]);
    let names: Vec<Option<&str>> = run.entities.iter().map(|e| e.name.as_deref()).collect();
    assert_eq!(names, [Some("base-1"), Some("tank-1"), Some("truck-1")]);
    let groups: Vec<Option<&str>> = run.entities.iter().map(|e| e.group.as_deref()).collect();
    assert_eq!(groups, [Some("alpha"), Some("alpha"), Some("bravo")]);
    let templates: Vec<String> = run
        .entities
        .iter()
        .map(|e| e.template.to_string())
        .collect();
    assert_eq!(
        templates,
        ["ground_vehicle.base", "tank.mbt_generic", "truck.cargo"]
    );
    let spawned: Vec<u64> = run
        .engine
        .snapshot()
        .entities
        .iter()
        .map(|e| e.id.get())
        .collect();
    assert_eq!(spawned, [1, 2, 3]);
    assert_eq!(run.duration_ticks, scenario.duration_ticks);
}

#[test]
fn instantiated_entities_use_the_motion_model_named_by_their_template() {
    let run = run_with(&load_example());

    let models: Vec<String> = run
        .engine
        .snapshot()
        .entities
        .iter()
        .map(|e| e.motion_model.to_string())
        .collect();

    assert_eq!(models, ["builtin.ground.kinematic"; 3]);
}

/// ASM-044, REQ-C-3: placement by lat/lon and by ECEF both land on the stated point.
#[test]
fn instantiated_entities_start_at_the_placed_positions() {
    let scenario = load_example();
    let run = run_with(&scenario);

    for (spawned, placed) in run
        .engine
        .snapshot()
        .entities
        .iter()
        .zip(&scenario.entities)
    {
        let d = spawned.state.position.distance(placed.position.to_ecef()).0;
        assert!(
            d < 1e-6,
            "entity {} starts {d} m from its placement",
            spawned.id
        );
    }
}

/// REQ-M-4, ASM-058: overrides in the scenario reach the model that is built: entity 1 is limited
/// to 5 m/s although both ask for a cruise speed of 15 m/s.
#[test]
fn override_reaches_the_motion_model() {
    let route = "[entities.route]\narrival_radius_m = 5.0\ncruise_speed_mps = 15.0\nwaypoints = [{ lat_deg = 52.0, lon_deg = 4.07 }]\n";
    let slow = entity(
        "{ lat_deg = 52.0, lon_deg = 4.0 }",
        &format!("[entities.overrides.motion.params]\nmax_speed_mps = 5.0\n{route}"),
    );
    let fast = entity(
        "{ lat_deg = 52.1, lon_deg = 4.0 }",
        &route.replace("52.0, lon_deg = 4.07", "52.1, lon_deg = 4.07"),
    );
    let scenario = from_text(&scenario_text(
        "",
        "clock = \"afap\"",
        &format!("{slow}\n{fast}"),
    ));
    let mut engine = run_with(&scenario).engine;
    let mut peak = [0.0_f64; 2];

    engine
        .run(RunUntil::Ticks(900), |view| {
            for (i, e) in view.snapshot().entities.iter().enumerate() {
                peak[i] = peak[i].max(e.state.velocity.norm());
            }
        })
        .expect("run");

    assert!(
        peak[0] <= 5.0 + 1e-9,
        "overridden entity reached {} m/s",
        peak[0]
    );
    assert!(peak[0] > 4.9, "it does reach its limit: {}", peak[0]);
    assert!(
        peak[1] > 14.9,
        "the other entity keeps the template limit: {}",
        peak[1]
    );
}

/// REQ-C-6, REQ-T-1: the terrain provider is chosen by ID and parameters: a scenario with terrain
/// at 100 m holds a ground entity at 100 m.
#[test]
fn scenario_terrain_height_reaches_the_engine() {
    let terrain = "[terrain]\nprovider = \"builtin.terrain.flat\"\nparams = { height_m = 100.0 }\n";
    let scenario = from_text(&scenario_text(
        terrain,
        "clock = \"afap\"",
        &entity("{ lat_deg = 52.0, lon_deg = 4.0, alt_m = 100.0 }", ""),
    ));
    let mut engine = run_with(&scenario).engine;

    run_silent(&mut engine, RunUntil::Ticks(10));

    let height = engine.snapshot().entities[0].geodetic().height().0;
    assert!((height - 100.0).abs() < 1e-6, "height {height}");
}

/// ASM-059: names and groups are engine-level metadata, not simulation state: they are not in the
/// state hash.
#[test]
fn instantiate_hash_ignores_names_and_groups() {
    let plain = scenario_text(
        "",
        "clock = \"afap\"",
        &entity("{ lat_deg = 52.0, lon_deg = 4.0 }", ""),
    );
    let named = scenario_text(
        "",
        "clock = \"afap\"",
        &entity(
            "{ lat_deg = 52.0, lon_deg = 4.0 }",
            "name = \"someone\"\ngroup = \"red\"",
        ),
    );

    let (a, b) = (run_with(&from_text(&plain)), run_with(&from_text(&named)));

    assert_eq!(a.engine.state_hash(), b.engine.state_hash());
    assert_eq!(b.entities[0].name.as_deref(), Some("someone"));
}

/// ASM-062: the run options replace the scenario's clock.
#[test]
fn run_options_clock_override_replaces_the_scenario_clock() {
    let mut engine = instantiate_with(
        &load_example(),
        Duration::ZERO,
        None,
        Some(ClockMode::Stepped),
    )
    .engine;

    run_silent(&mut engine, RunUntil::Idle);

    assert_eq!(
        engine.now().tick(),
        0,
        "a stepped clock with no requests runs nothing"
    );
}

/// ASM-060: `cgf-templates` cannot depend on the kernel, so it mirrors the `TimeScale` bounds; the
/// two must agree at and just beyond both bounds.
#[test]
fn time_scale_bounds_agree_between_templates_and_kernel() {
    let accepts = |v: f64| {
        let text = scenario_text(
            "",
            &format!("clock = \"scaled\"\ntime_scale = {v:?}"),
            &entity("{ lat_deg = 52.0, lon_deg = 4.0 }", ""),
        );
        let registry = registry();
        let loader = Loader::new(&registry).expect("loader");
        loader
            .scenario_from_sources(
                &SourceFile::new("inline/scenario.toml", text.as_str()),
                &[SourceFile::new("inline/templates/ground.toml", TEMPLATE)],
            )
            .is_ok()
    };

    for v in [0.001, 0.5, 1.0, 1_000_000.0, 0.000_999, 1_000_001.0, 0.0] {
        assert_eq!(accepts(v), TimeScale::new(v).is_ok(), "time_scale {v:?}");
    }
}

#[test]
fn scaled_scenario_clock_maps_to_the_kernel_time_scale() {
    let text = scenario_text(
        "",
        "clock = \"scaled\"\ntime_scale = 4.0",
        &entity("{ lat_deg = 52.0, lon_deg = 4.0 }", ""),
    );

    let scenario = from_text(&text);

    assert_eq!(scenario.clock, ClockSetting::Scaled(4.0));
}

// --- loading from files ------------------------------------------------------------------------

/// REQ-S-2: the engine facade loads a scenario file with the built-in registry.
#[test]
fn load_scenario_reads_the_example_file() {
    let scenario = load_scenario(&registry(), &example_path()).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(scenario.name, "walking_skeleton");
    assert_eq!(scenario.seed, 42);
    assert_eq!(scenario.tick_rate.hz(), 30);
    assert_eq!(scenario.clock, ClockSetting::AsFastAsPossible);
    assert_eq!(scenario.duration_ticks, 7200);
    assert_eq!(scenario.terrain.id.as_str(), "builtin.terrain.flat");
    assert_eq!(scenario.templates.len(), 3);
    assert_eq!(scenario.entities.len(), 3);
}

#[test]
fn load_scenario_with_errors_returns_invalid_with_every_diagnostic() {
    let result = load_scenario(&registry(), &fixture_path("bad_scenario/scenario.toml"));

    let Err(ScenarioError::Invalid(diagnostics)) = result else {
        panic!(
            "expected ScenarioError::Invalid, got {:?}",
            result.map(|s| s.name)
        );
    };
    let path = fixture_path("bad_scenario/scenario.toml");
    let found: Vec<(String, std::path::PathBuf, u32, u32)> = diagnostics
        .iter()
        .map(|d| {
            let l = d.location.as_ref().expect("located");
            (
                d.code.as_str().to_string(),
                l.path.clone(),
                l.line,
                l.column,
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("invalid_value".to_string(), path.clone(), 8, 16),
            ("unknown_template".to_string(), path, 14, 12),
        ]
    );
}

#[test]
fn validate_path_on_the_example_directory_checks_four_files_and_reports_nothing() {
    let report = validate_path(&registry(), &example_dir()).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(report.diagnostics, []);
    let names: Vec<String> = report
        .files_checked
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(
        names,
        [
            "scenario.toml",
            "ground_vehicle_base.toml",
            "tank_mbt_generic.toml",
            "truck_cargo.toml"
        ]
    );
}

#[test]
fn validate_path_on_the_bad_fixture_reports_its_diagnostics_without_instantiating() {
    let report = validate_path(&registry(), &fixture_path("bad_scenario/scenario.toml"))
        .unwrap_or_else(|e| panic!("{e}"));

    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["invalid_value", "unknown_template"]);
}

/// REQ-S-2: a loaded scenario instantiates with a manual wall clock, so tests and embedders can
/// drive it without real time.
#[test]
fn instantiate_accepts_a_loaded_scenario_and_runs_it() {
    let scenario = load_example();
    let mut run = instantiate(
        &registry(),
        &scenario,
        common::options(Duration::ZERO, None, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));

    run_silent(&mut run.engine, RunUntil::Ticks(60));

    assert_eq!(run.engine.now().tick(), 60);
}

/// The unused-terrain-trait guard: the flat terrain a scenario selects is a `TerrainProvider`.
#[test]
fn flat_terrain_from_the_registry_reports_its_height() {
    let registry = registry();
    let id = cgf_core::ComponentId::new("builtin.terrain.flat").expect("id");
    let factory = registry.terrain(&id).expect("registered");

    let terrain = factory
        .build(&serde_json::json!({ "height_m": 12.5 }))
        .unwrap_or_else(|e| panic!("{e}"));

    let at = Geodetic::from_degrees(52.0, 4.0, 0.0).expect("geodetic");
    assert_eq!(terrain.height_at(at).0, 12.5);
}
