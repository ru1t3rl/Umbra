//! AT-1 (M7, motion model row): the example plugin `example-hover` (`example.motion.hover`) through the
//! engine as a real library. Requirements: Acceptance 1 (a third-party-style example of each kind),
//! REQ-P-4, REQ-SDK-3, REQ-M-1, REQ-M-4, REQ-C-6, ADR 0050, ADR 0056, ASM-649, ASM-669.
//!
//! Contract with the plugin (the crate docs of `examples/plugins/hover` are the source): params
//! `height_agl_m`, `max_speed_mps`, `max_climb_mps`, `max_accel_mps2`; with no motion intent the
//! horizontal velocity is kept; the height above the ellipsoid moves toward `terrain + height_agl_m`
//! by at most `max_climb_mps * dt` per step, the terrain coming from the step call. The terrain is the
//! test terrain `test.terrain.relief` (`kinds_support`), a slope east of 4 E.
//!
//! Step 4 of M7 (WI-2): written before WI-17. The rows are ignored until the plugin is implemented
//! (WI-17; it also needs WI-7/8/10 for ABI 0.2 and the motion bridge).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod kinds_support;
mod plugin_support;

use core::ops::ControlFlow;
use core::time::Duration;
use std::path::PathBuf;
use std::sync::LazyLock;

use common::options;
use kinds_support::{example_plugins, registry_with_relief, relief, write_dir};
use umbra_core::motion::contract::check_motion_model;
use umbra_core::{
    ComponentId, FlatTerrain, Geodetic, Intent, KinematicState, Meters, MetersPerSecond,
    ParamValue, Quaternion, Seconds, TerrainProvider, Vec3,
};
use umbra_engine::builtins::standard_registry;
use umbra_engine::{
    HostOptions, RunUntil, instantiate_with_plugins, load_plugins, load_scenario_with_plugins,
    validate_path_with_plugins,
};

static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| example_plugins(&["hover"]));

const DRONE_TEMPLATE: &str = r#"schema_version = 1
id = "example.hover.drone"

[motion]
model = "example.motion.hover"
params = { height_agl_m = 20.0, max_speed_mps = 15.0, max_climb_mps = 5.0, max_accel_mps2 = 4.0 }
"#;

/// A 20 s run of one hovering entity starting 20 m above the terrain at 4 E and moving east at
/// `speed_mps`, over a slope of `slope` (m per m east).
fn scenario(slope: f64, speed_mps: f64, agl_param: &str) -> PathBuf {
    let template = DRONE_TEMPLATE.replace("height_agl_m = 20.0", agl_param);
    let text = format!(
        r#"schema_version = 1

[scenario]
name = "hover_agl"
seed = 7
tick_rate_hz = 30
clock = "afap"
duration_s = 20.0
template_paths = ["templates"]

[terrain]
provider = "test.terrain.relief"
params = {{ slope = {slope:?} }}

[[entities]]
template = "example.hover.drone"
name = "drone"
position = {{ lat_deg = 52.0, lon_deg = 4.0, alt_m = 20.0 }}
heading_deg = 90.0
speed_mps = {speed_mps:?}
"#
    );
    write_dir(
        "hover",
        &[
            ("scenario.toml", text.as_str()),
            ("templates/drone.toml", template.as_str()),
        ],
    )
    .join("scenario.toml")
}

/// Per tick, the drone's height above the relief and above the ellipsoid, and its position.
struct Flight {
    agl_m: Vec<f64>,
    height_m: Vec<f64>,
    start: umbra_core::Ecef,
    end: umbra_core::Ecef,
}

fn fly(slope: f64, speed_mps: f64) -> Flight {
    let path = scenario(slope, speed_mps, "height_agl_m = 20.0");
    let loaded = load_scenario_with_plugins(
        &registry_with_relief(),
        &path,
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("load the hover scenario");
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let terrain = relief(slope, None);
    let (mut engine, ticks) = (run.engine, run.duration_ticks);
    let start = engine.snapshot().entities[0].state.position;
    let (mut agl_m, mut height_m, mut end) = (Vec::new(), Vec::new(), start);
    engine
        .run(RunUntil::Ticks(ticks), |view| {
            let entity = &view.snapshot().entities[0];
            let at = entity.geodetic();
            agl_m.push(at.height().0 - terrain.height_at(at).0);
            height_m.push(at.height().0);
            end = entity.state.position;
            ControlFlow::Continue(())
        })
        .expect("run");
    Flight {
        agl_m,
        height_m,
        start,
        end,
    }
}

/// Acceptance 1, REQ-C-6, REQ-M-4: the plugin drives the entity along the slope and holds its height
/// above the terrain (within 0.25 m, a few ticks of lag at 0.3 m/s of climb) the whole way.
#[test]
#[ignore = "M7 WI-17"]
fn hover_plugin_drives_entities_and_holds_agl_over_terrain() {
    let flight = fly(0.03, 10.0);

    let worst = flight
        .agl_m
        .iter()
        .map(|agl| (agl - 20.0).abs())
        .fold(0.0, f64::max);
    assert!(worst < 0.25, "worst AGL deviation {worst} m");
    let travelled = (flight.end - flight.start).norm();
    assert!(travelled > 150.0, "moved {travelled} m in 20 s at 10 m/s");
    let climbed = flight.height_m.last().copied().unwrap_or_default() - 20.0;
    assert!(climbed > 4.0, "followed the slope up: {climbed} m");
}

/// REQ-M-1: the climb rate limit holds when the terrain rises faster than the model can follow.
#[test]
#[ignore = "M7 WI-17"]
fn hover_plugin_never_climbs_faster_than_its_limit() {
    let flight = fly(1.0, 10.0);

    let fastest = flight
        .height_m
        .windows(2)
        .map(|w| (w[1] - w[0]) * 30.0)
        .fold(f64::MIN, f64::max);
    assert!(fastest <= 5.0 + 1e-6, "climbed {fastest} m/s, limit 5");
    assert!(
        fastest > 4.0,
        "and it did climb at the limit: {fastest} m/s"
    );
}

/// The contract row of the plan: the core `motion::contract` suite through the plugin.
#[test]
#[ignore = "M7 WI-17"]
fn hover_model_passes_the_core_motion_contract() {
    let (registry, host) = load_plugins(
        &standard_registry().expect("registry"),
        std::slice::from_ref(&*PLUGINS),
        HostOptions::default(),
    )
    .expect("load the hover plugin");
    let factory = registry
        .motion_model(&ComponentId::new("example.motion.hover").expect("id"))
        .expect("registered");
    let params: ParamValue = serde_json::json!({ "height_agl_m": 20.0 });
    let model = {
        let _scope = host.build_scope();
        factory.build(&params).expect("build")
    };
    let at = |alt| {
        Geodetic::from_degrees(52.0, 4.0, alt)
            .expect("valid")
            .to_ecef()
    };
    let state = |alt: f64, v: f64| KinematicState {
        position: at(alt),
        velocity: Vec3::new(v, 0.0, 0.0),
        orientation: Quaternion::IDENTITY,
    };
    let cases = vec![
        (state(20.0, 0.0), vec![]),
        (state(20.0, 0.0), vec![Intent::Hold]),
        (
            state(20.0, 0.0),
            vec![Intent::SetSpeed(MetersPerSecond(8.0))],
        ),
        (state(60.0, 5.0), vec![]),
    ];

    let verdict = check_motion_model(
        model.as_ref(),
        &cases,
        Seconds(1.0 / 30.0),
        &FlatTerrain {
            height: Meters(0.0),
        },
    );

    assert_eq!(verdict, Ok(()));
    assert_eq!(host.faults(), Vec::new());
}

/// REQ-T-3: a parameter outside the schema is a validation error located at the template, found
/// without running (the plugin's own schema is applied to the model params).
#[test]
#[ignore = "M7 WI-17"]
fn hover_params_out_of_range_are_validation_errors() {
    let path = scenario(0.0, 0.0, "height_agl_m = 0.5");

    let report = validate_path_with_plugins(
        &registry_with_relief(),
        &path,
        std::slice::from_ref(&*PLUGINS),
    )
    .expect("validate");

    let codes: Vec<String> = report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_owned())
        .collect();
    assert_eq!(codes, ["param_out_of_range"]);
}
