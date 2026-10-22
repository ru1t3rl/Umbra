//! AT-2 (M7, sensor row): the example plugin `example-radar` (`example.sensor.radar`) through the
//! engine as a real library. Requirements: Acceptance 1, REQ-P-4, REQ-SDK-3, REQ-B-5, REQ-K-4,
//! REQ-C-6, ADR 0050, ADR 0056, ASM-649, ASM-669.
//!
//! Contract with the plugin (the crate docs of `examples/plugins/radar` are the source): for each
//! candidate in id order inside the range and the elevation gate and with terrain line of sight
//! (both ends raised by `antenna_height_m` along the geocentric vertical), one `next_f64()` from the
//! observer's stream decides `draw < detect_probability`. `kinds_support::registry_with_reference_radar`
//! registers that contract as the core sensor `test.sensor.radar_ref`; running the same scenario with
//! either sensor must give the same contacts on every tick (same entity ids, so same streams).
//!
//! The scene: observer `radar-1` at the origin, `near-1` 1000 m east, `behind-1` 3000 m east behind a
//! 300 m wall between 1800 and 2200 m east, `north-1` 500 m north, and `high-1`, a stationary aircraft
//! 500 m east at 2000 m (elevation about 76 degrees). All stand still.
//!
//! Step 4 of M7 (WI-2): ignored until WI-17 (and WI-6 for the aircraft, WI-7/8/10 for ABI 0.2).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;
mod kinds_support;
mod plugin_support;

use core::fmt::Write as _;
use core::ops::ControlFlow;
use core::time::Duration;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use common::options;
use kinds_support::{
    RADAR_REF_ID, example_plugins, lat, lon, registry_with_reference_radar, write_dir,
};
use umbra_core::Contact;
use umbra_engine::{
    RunUntil, instantiate_with_plugins, load_scenario_with_plugins, validate_path_with_plugins,
};

static PLUGINS: LazyLock<PathBuf> = LazyLock::new(|| example_plugins(&["radar"]));

const PLUGIN_RADAR: &str = "example.sensor.radar";
const TICKS: u64 = 150;

/// Where each station stands: name, north m, east m, alt m, template.
const SCENE: [(&str, f64, f64, f64, &str); 5] = [
    ("radar-1", 0.0, 0.0, 0.0, "example.radar.station"),
    ("near-1", 0.0, 1000.0, 0.0, "example.radar.ground"),
    ("behind-1", 0.0, 3000.0, 0.0, "example.radar.ground"),
    ("north-1", 500.0, 0.0, 0.0, "example.radar.ground"),
    ("high-1", 0.0, 500.0, 2000.0, "example.radar.aircraft"),
];

/// Writes the scene with the observer's sensor `kind` and `params` (inline table text) and, with
/// `wall`, the wall between 1800 and 2200 m east.
fn write_scene(kind: &str, params: &str, wall: bool) -> PathBuf {
    let terrain = if wall {
        format!(
            "{{ wall_lon_lo_deg = {:.7}, wall_lon_hi_deg = {:.7}, wall_height_m = 300.0 }}",
            lon(1800.0),
            lon(2200.0)
        )
    } else {
        "{ slope = 0.0 }".to_owned()
    };
    let mut scenario = format!(
        r#"schema_version = 1

[scenario]
name = "radar_scene"
seed = 7
tick_rate_hz = 30
clock = "afap"
duration_s = 5.0
template_paths = ["templates"]

[terrain]
provider = "test.terrain.relief"
params = {terrain}
"#
    );
    for (name, north, east, alt, template) in SCENE {
        write!(
            scenario,
            "\n[[entities]]\ntemplate = \"{template}\"\nname = \"{name}\"\n\
             position = {{ lat_deg = {:.7}, lon_deg = {:.7}, alt_m = {alt:?} }}\n",
            lat(north),
            lon(east)
        )
        .expect("write to a String");
    }
    let ground =
        "params = { max_speed_mps = 10.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }";
    let station = format!(
        "schema_version = 1\nid = \"example.radar.station\"\nextends = \"example.radar.ground\"\n\n\
         [[sensors]]\nkind = \"{kind}\"\nparams = {params}\n"
    );
    let base = format!(
        "schema_version = 1\nid = \"example.radar.ground\"\n\n[motion]\n\
         model = \"builtin.ground.kinematic\"\n{ground}\n"
    );
    let aircraft = "schema_version = 1\nid = \"example.radar.aircraft\"\n\n[motion]\n\
         model = \"builtin.air.kinematic\"\nparams = { max_speed_mps = 100.0, min_speed_mps = 0.0, max_accel_mps2 = 5.0, \
         max_climb_rate_mps = 10.0, max_bank_deg = 45.0 }\n";
    write_dir(
        "radar",
        &[
            ("scenario.toml", scenario.as_str()),
            ("templates/ground.toml", base.as_str()),
            ("templates/station.toml", station.as_str()),
            ("templates/aircraft.toml", aircraft),
        ],
    )
    .join("scenario.toml")
}

/// The observer's contacts after each of `TICKS` ticks.
fn contacts_per_tick(path: &Path, with_plugin: bool) -> (Vec<Vec<Contact>>, Vec<(String, u64)>) {
    let plugins: &[PathBuf] = if with_plugin {
        std::slice::from_ref(&*PLUGINS)
    } else {
        &[]
    };
    let loaded = load_scenario_with_plugins(&registry_with_reference_radar(), path, plugins)
        .expect("load the radar scene");
    let run = instantiate_with_plugins(&loaded, options(Duration::from_micros(500), None, None))
        .expect("instantiate");
    let names: Vec<(String, u64)> = run
        .entities
        .iter()
        .map(|e| (e.name.clone().unwrap_or_default(), e.id.get()))
        .collect();
    let observer = run.entities[0].id;
    let mut engine = run.engine;
    let mut seen = Vec::new();
    engine
        .run(RunUntil::Ticks(TICKS), |view| {
            let snapshot = view.snapshot();
            let entity = snapshot
                .entities
                .iter()
                .find(|e| e.id == observer)
                .expect("the observer");
            seen.push(entity.contacts.clone());
            ControlFlow::Continue(())
        })
        .expect("run");
    (seen, names)
}

/// The names of the contacts of one tick.
fn names_of(contacts: &[Contact], names: &[(String, u64)]) -> Vec<String> {
    contacts
        .iter()
        .map(|c| {
            names
                .iter()
                .find(|(_, id)| *id == c.entity.get())
                .map(|(n, _)| n.clone())
                .unwrap_or_default()
        })
        .collect()
}

/// Acceptance 1, REQ-K-4: with a detection probability below one the plugin's contacts equal the
/// reference's on every tick (so the draws are the same, in the same order, from the observer's
/// stream), and the draws do matter (the contact set changes over the run).
#[test]
#[ignore = "M7 WI-17"]
fn radar_plugin_contacts_match_reference_and_respect_line_of_sight() {
    let params = "{ range_m = 5000.0, max_elevation_deg = 30.0, detect_probability = 0.5 }";
    let plugin = write_scene(PLUGIN_RADAR, params, true);
    let reference = write_scene(RADAR_REF_ID, params, true);

    let (got, _) = contacts_per_tick(&plugin, true);
    let (want, _) = contacts_per_tick(&reference, false);

    assert_eq!(got.len(), want.len());
    for (tick, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g.len(), w.len(), "tick {tick}");
        for (a, b) in g.iter().zip(w) {
            assert_eq!(
                (a.entity, a.position, a.velocity, &a.force),
                (b.entity, b.position, b.velocity, &b.force),
                "tick {tick}"
            );
            assert!((a.range.0 - b.range.0).abs() < 1e-9, "tick {tick}");
        }
    }
    let distinct: std::collections::BTreeSet<Vec<u64>> = got
        .iter()
        .map(|c| c.iter().map(|c| c.entity.get()).collect())
        .collect();
    assert!(
        distinct.len() > 1,
        "at 50 % the contact set varies: {distinct:?}"
    );
}

/// REQ-C-6: terrain line of sight (the wall hides `behind-1`) and the elevation gate (`high-1` is
/// above 30 degrees) are respected on every tick; the visible ones are seen every tick at p = 1.
#[test]
#[ignore = "M7 WI-17"]
fn radar_plugin_contacts_respect_range_elevation_and_line_of_sight() {
    let params = "{ range_m = 5000.0, max_elevation_deg = 30.0 }";
    let path = write_scene(PLUGIN_RADAR, params, true);

    let (seen, names) = contacts_per_tick(&path, true);

    for (tick, contacts) in seen.iter().enumerate() {
        let mut found = names_of(contacts, &names);
        found.sort();
        assert_eq!(found, ["near-1", "north-1"], "tick {tick}");
    }
}

/// The same scene without the wall and the gate: everything inside the range is a contact, and a
/// range below the distance to `near-1` (1000 m) and `high-1` (2062 m) removes them.
#[test]
#[ignore = "M7 WI-17"]
fn radar_plugin_without_wall_or_gate_sees_everything_in_range() {
    let open = write_scene(PLUGIN_RADAR, "{ range_m = 5000.0 }", false);
    let short = write_scene(PLUGIN_RADAR, "{ range_m = 800.0 }", false);

    let (all, names) = contacts_per_tick(&open, true);
    let (close, close_names) = contacts_per_tick(&short, true);

    let mut found = names_of(&all[0], &names);
    found.sort();
    assert_eq!(found, ["behind-1", "high-1", "near-1", "north-1"]);
    let mut found = names_of(&close[0], &close_names);
    found.sort();
    assert_eq!(found, ["north-1"], "near-1 is 1000 m away, high-1 2062 m");
}

/// REQ-T-3: out-of-range sensor parameters are validation errors, found without running.
#[test]
#[ignore = "M7 WI-17"]
fn radar_params_out_of_range_are_validation_errors() {
    let path = write_scene(PLUGIN_RADAR, "{ range_m = 0.0 }", false);

    let report = validate_path_with_plugins(
        &registry_with_reference_radar(),
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
