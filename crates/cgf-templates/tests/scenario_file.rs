//! AT-5 (scenario rows), AT-7 (scenario file) and AT-1 (terrain). Requirements: REQ-T-2, REQ-T-5,
//! REQ-C-3, ASM-044, ASM-058, ASM-060, ADR 0010, spec section 7.3.
//!
//! The scenario under test places one `veh.base` entity (`stub.motion.alpha`, `a = 2.0`). Tests
//! vary one line of the base text and look up expected positions in the final text with
//! `pos_after`/`pos_of`. Every diagnostic assertion checks code, path, line and column.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use serde_json::json;

use cgf_core::{Geodetic, Meters, MetersPerSecond, Radians, TickRate};
use cgf_templates::{ClockSetting, Scenario, SourceFile};
use common::{
    alpha_template, errors, fixture_path, lines, loader, ok, pos_after, pos_of, pos_of_nth,
    registry, row, rows, scenario, src, tid,
};

const SCN: &str = "scn/scenario.toml";

fn template() -> SourceFile {
    src(
        "scn/templates/veh.toml",
        &alpha_template("veh.base", None, "2.0"),
    )
}

fn base() -> String {
    lines(&[
        "schema_version = 1",
        "",
        "[scenario]",
        "name = \"test\"",
        "seed = 7",
        "tick_rate_hz = 30",
        "clock = \"afap\"",
        "duration_s = 10.0",
        "template_paths = [\"templates\"]",
        "",
        "[[entities]]",
        "template = \"veh.base\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "heading_deg = 90.0",
    ])
}

fn load(text: &str) -> Result<Scenario, cgf_templates::Diagnostics> {
    scenario(&src(SCN, text), &[template()])
}

fn failing(text: &str) -> Vec<common::Row> {
    rows(&errors(load(text)))
}

fn at(code: &str, pos: (u32, u32)) -> Vec<common::Row> {
    vec![row(code, SCN, pos)]
}

fn loaded(text: &str) -> Scenario {
    ok(load(text))
}

fn ecef_of(lat: f64, lon: f64, alt: f64) -> cgf_core::Ecef {
    Geodetic::from_degrees(lat, lon, alt)
        .expect("valid geodetic")
        .to_ecef()
}

/// `{ ecef_m = [x, y, z] }` with round-trip float formatting.
fn ecef_table(p: cgf_core::Ecef) -> String {
    format!("{{ ecef_m = [{:?}, {:?}, {:?}] }}", p.0.x, p.0.y, p.0.z)
}

// --- the happy path ----------------------------------------------------------------------------

#[test]
fn given_valid_scenario_then_all_fields_are_read_with_units_converted() {
    let s = loaded(&base());

    assert_eq!(s.name, "test");
    assert_eq!(s.source, std::path::Path::new(SCN));
    assert_eq!(s.seed, 7);
    assert_eq!(s.tick_rate, TickRate::new(30).expect("rate"));
    assert_eq!(s.clock, ClockSetting::AsFastAsPossible);
    assert_eq!(s.duration_ticks, 300);
    assert_eq!(s.templates.len(), 1);
    assert_eq!(s.entities.len(), 1);
    let e = &s.entities[0];
    assert_eq!(e.template, tid("veh.base"));
    assert_eq!(e.name, None);
    assert_eq!(e.group, None);
    assert_eq!(e.heading, Radians::from_degrees(90.0));
    assert_eq!(e.speed, MetersPerSecond(0.0));
    assert_eq!(e.motion.id.as_str(), "stub.motion.alpha");
    assert_eq!(e.motion.params, json!({ "a": 2.0 }));
    assert!(e.route.is_none());
}

#[test]
fn name_group_and_speed_are_read() {
    let text = base().replace(
        "heading_deg = 90.0",
        "heading_deg = 90.0\nname = \"base-1\"\ngroup = \"alpha\"\nspeed_mps = 3.5",
    );

    let e = &loaded(&text).entities[0];

    assert_eq!(e.name.as_deref(), Some("base-1"));
    assert_eq!(e.group.as_deref(), Some("alpha"));
    assert_eq!(e.speed, MetersPerSecond(3.5));
}

#[test]
fn terrain_defaults_to_flat_zero() {
    let s = loaded(&base());

    assert_eq!(s.terrain.id.as_str(), "builtin.terrain.flat");
    assert_eq!(s.terrain.params, json!({ "height_m": 0.0 }));
}

#[test]
fn terrain_section_selects_a_provider_and_params() {
    let text = base().replace(
        "[[entities]]",
        "[terrain]\nprovider = \"stub.terrain.flat\"\nparams = { height_m = 12.5 }\n\n[[entities]]",
    );

    let s = loaded(&text);

    assert_eq!(s.terrain.id.as_str(), "stub.terrain.flat");
    assert_eq!(s.terrain.params, json!({ "height_m": 12.5 }));
}

#[test]
fn entities_keep_file_order() {
    let more: Vec<String> = ["alpha", "mike"]
        .iter()
        .map(|name| {
            format!(
                "\n[[entities]]\ntemplate = \"veh.base\"\nname = \"{name}\"\nposition = {{ lat_deg = 52.0, lon_deg = 4.0 }}\nheading_deg = 0.0\n"
            )
        })
        .collect();
    let text = base() + &more.concat();
    let text = text.replacen(
        "[[entities]]\ntemplate = \"veh.base\"\n",
        "[[entities]]\ntemplate = \"veh.base\"\nname = \"zulu\"\n",
        1,
    );

    let names: Vec<Option<String>> = loaded(&text).entities.into_iter().map(|e| e.name).collect();

    assert_eq!(
        names,
        [
            Some("zulu".to_string()),
            Some("alpha".to_string()),
            Some("mike".to_string())
        ]
    );
}

// --- AT-5: schema_version ----------------------------------------------------------------------

#[test]
fn schema_version_1_is_accepted() {
    assert!(load(&base()).is_ok());
}

#[test]
fn newer_schema_version_is_rejected_at_its_value_and_file_stops() {
    let text = lines(&["schema_version = 2", "[scenario]", "colour = \"red\""]);

    assert_eq!(failing(&text), at("unsupported_schema_version", (1, 18)));
}

#[test]
fn missing_schema_version_is_missing_key() {
    let text = base().replace("schema_version = 1\n", "");

    let found = errors(load(&text));

    // The location of an absent key is not specified by the plan: code, file and message only.
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "missing_key");
    assert!(
        found[0].message.contains("schema_version"),
        "{}",
        found[0].message
    );
}

#[test]
fn schema_version_zero_or_negative_is_invalid_value() {
    assert_eq!(
        failing(&base().replace("schema_version = 1", "schema_version = 0")),
        at("invalid_value", (1, 18))
    );
    assert_eq!(
        failing(&base().replace("schema_version = 1", "schema_version = -3")),
        at("invalid_value", (1, 18))
    );
}

#[test]
fn schema_version_as_string_is_wrong_type() {
    assert_eq!(
        failing(&base().replace("schema_version = 1", "schema_version = \"1\"")),
        at("wrong_type", (1, 18))
    );
}

// --- AT-7: positions (ASM-044) -----------------------------------------------------------------

#[test]
fn position_by_lat_lon_and_by_ecef_parse_to_equivalent_positions() {
    let expected = ecef_of(52.0, 4.0, 100.0);
    let by_geodetic = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { lat_deg = 52.0, lon_deg = 4.0, alt_m = 100.0 }",
    );
    let by_ecef = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        &format!("position = {}", ecef_table(expected)),
    );

    let (a, b) = (loaded(&by_geodetic), loaded(&by_ecef));

    assert_eq!(a.entities[0].position.to_ecef(), expected);
    assert_eq!(b.entities[0].position.to_ecef(), expected);
}

#[test]
fn alt_m_defaults_to_zero() {
    let explicit = base().replace("lon_deg = 4.0 }", "lon_deg = 4.0, alt_m = 0.0 }");

    let (without, with) = (loaded(&base()), loaded(&explicit));

    assert_eq!(
        without.entities[0].position.to_ecef(),
        with.entities[0].position.to_ecef()
    );
    assert_eq!(
        without.entities[0].position.to_ecef(),
        ecef_of(52.0, 4.0, 0.0)
    );
}

#[test]
fn alt_m_is_height_above_the_ellipsoid() {
    let text = base().replace("lon_deg = 4.0 }", "lon_deg = 4.0, alt_m = 250.5 }");

    let s = loaded(&text);

    assert_eq!(s.entities[0].position.to_ecef(), ecef_of(52.0, 4.0, 250.5));
}

#[test]
fn both_position_forms_is_invalid_value_at_the_position_table() {
    let text = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { lat_deg = 52.0, lon_deg = 4.0, ecef_m = [1.0, 2.0, 3.0] }",
    );

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "position = "))
    );
}

#[test]
fn neither_position_form_is_invalid_value_at_the_position_table() {
    let text = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { }",
    );

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "position = "))
    );
}

#[test]
fn ecef_m_with_two_numbers_is_invalid_value_at_the_array() {
    let text = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { ecef_m = [1.0, 2.0] }",
    );

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "ecef_m = "))
    );
}

#[test]
fn ecef_m_with_four_numbers_is_invalid_value_at_the_array() {
    let text = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { ecef_m = [1.0, 2.0, 3.0, 4.0] }",
    );

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "ecef_m = "))
    );
}

#[test]
fn ecef_m_integers_are_accepted_as_numbers() {
    let text = base().replace(
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "position = { ecef_m = [3925375, 274488, 5002803] }",
    );

    let s = loaded(&text);

    assert_eq!(s.entities[0].position.to_ecef().0.x, 3_925_375.0);
}

#[test]
fn latitude_out_of_range_is_invalid_value_at_position() {
    let text = base().replace("lat_deg = 52.0", "lat_deg = 91.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "position = "))
    );
}

#[test]
fn longitude_out_of_range_is_invalid_value_at_position() {
    let text = base().replace("lon_deg = 4.0", "lon_deg = 181.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "position = "))
    );
}

#[test]
fn position_with_a_string_coordinate_is_wrong_type_at_the_value() {
    let text = base().replace("lat_deg = 52.0", "lat_deg = \"north\"");

    assert_eq!(failing(&text), at("wrong_type", pos_of(&text, "\"north\"")));
}

#[test]
fn position_with_an_unknown_key_is_unknown_key_at_the_key() {
    let text = base().replace("lon_deg = 4.0 }", "lon_deg = 4.0, alt = 1.0 }");

    assert_eq!(failing(&text), at("unknown_key", pos_of(&text, "alt =")));
}

// --- AT-7: routes ------------------------------------------------------------------------------

fn with_route(waypoints: &str) -> String {
    format!(
        "{}[entities.route]\narrival_radius_m = 5.0\ncruise_speed_mps = 15.0\nwaypoints = {waypoints}\n",
        base()
    )
}

#[test]
fn waypoints_accept_both_forms() {
    let ecef = ecef_of(52.0, 4.02, 0.0);
    let text = with_route(&format!(
        "[\n  {{ lat_deg = 52.0, lon_deg = 4.01 }},\n  {},\n]",
        ecef_table(ecef)
    ));

    let s = loaded(&text);

    let route = s.entities[0].route.as_ref().expect("route");
    assert_eq!(route.waypoints.len(), 2);
    assert_eq!(route.waypoints[0].to_ecef(), ecef_of(52.0, 4.01, 0.0));
    assert_eq!(route.waypoints[1].to_ecef(), ecef);
    assert_eq!(route.arrival_radius, Meters(5.0));
    assert_eq!(route.cruise_speed, MetersPerSecond(15.0));
}

#[test]
fn waypoints_keep_their_order() {
    let text = with_route(
        "[{ lat_deg = 52.0, lon_deg = 4.3 }, { lat_deg = 52.0, lon_deg = 4.1 }, { lat_deg = 52.0, lon_deg = 4.2 }]",
    );

    let s = loaded(&text);

    let route = s.entities[0].route.as_ref().expect("route");
    let lons: Vec<Geodetic> = route
        .waypoints
        .iter()
        .map(|p| p.to_ecef().to_geodetic())
        .collect();
    assert!(lons[0].lon().0 > lons[2].lon().0 && lons[2].lon().0 > lons[1].lon().0);
}

#[test]
fn empty_waypoints_is_invalid_value_at_the_array() {
    let text = with_route("[]");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "waypoints = "))
    );
}

#[test]
fn waypoint_with_both_forms_is_invalid_value_at_that_waypoint() {
    let text = with_route("[{ lat_deg = 52.0, lon_deg = 4.0, ecef_m = [1.0, 2.0, 3.0] }]");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "waypoints = ["))
    );
}

#[test]
fn waypoint_latitude_out_of_range_is_invalid_value_at_that_waypoint() {
    let text = with_route("[{ lat_deg = 52.0, lon_deg = 4.0 }, { lat_deg = 95.0, lon_deg = 4.0 }]");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_of(&text, "{ lat_deg = 95.0"))
    );
}

#[test]
fn negative_arrival_radius_is_invalid_value_at_the_value() {
    let text = with_route("[{ lat_deg = 52.0, lon_deg = 4.0 }]")
        .replace("arrival_radius_m = 5.0", "arrival_radius_m = -1.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "arrival_radius_m = "))
    );
}

#[test]
fn route_with_an_unknown_key_is_unknown_key() {
    let text = with_route("[{ lat_deg = 52.0, lon_deg = 4.0 }]") + "loop = true\n";

    assert_eq!(failing(&text), at("unknown_key", pos_of(&text, "loop =")));
}

// --- AT-7: overrides ---------------------------------------------------------------------------

#[test]
fn overrides_are_merged_and_revalidated_with_scenario_location() {
    let good = base() + "[entities.overrides.motion.params]\na = 3.0\n";
    let bad = base() + "[entities.overrides.motion.params]\na = 0.0\n";

    let s = loaded(&good);

    assert_eq!(s.entities[0].motion.params, json!({ "a": 3.0 }));
    let (line, col) = pos_of(&bad, "a = 0.0");
    assert_eq!(failing(&bad), at("param_out_of_range", (line, col + 4)));
}

#[test]
fn override_with_an_unknown_param_is_unknown_param_in_the_scenario_file() {
    let text = base() + "[entities.overrides.motion.params]\nzzz = 1.0\n";

    let found = failing(&text);

    // `a` is inherited from the template, so only the unknown key is wrong.
    assert_eq!(found, at("unknown_param", pos_of(&text, "zzz")));
}

#[test]
fn override_leaves_the_template_unchanged_for_other_entities() {
    let mut text = base() + "[entities.overrides.motion.params]\na = 3.0\n";
    text.push_str("\n[[entities]]\ntemplate = \"veh.base\"\nposition = { lat_deg = 52.1, lon_deg = 4.0 }\nheading_deg = 0.0\n");

    let s = loaded(&text);

    assert_eq!(s.entities[0].motion.params, json!({ "a": 3.0 }));
    assert_eq!(s.entities[1].motion.params, json!({ "a": 2.0 }));
}

// --- AT-7: templates and names -----------------------------------------------------------------

#[test]
fn unknown_template_in_entity_is_unknown_template() {
    let text = base().replace("template = \"veh.base\"", "template = \"veh.nope\"");

    let found = errors(load(&text));

    assert_eq!(
        rows(&found),
        at("unknown_template", pos_after(&text, "template = "))
    );
    assert!(
        found[0].message.contains("veh.nope"),
        "{}",
        found[0].message
    );
}

#[test]
fn duplicate_entity_name_is_duplicate_name() {
    let mut text = base().replace("heading_deg = 90.0", "heading_deg = 90.0\nname = \"alpha\"");
    text.push_str("\n[[entities]]\ntemplate = \"veh.base\"\nname = \"alpha\"\nposition = { lat_deg = 52.1, lon_deg = 4.0 }\nheading_deg = 0.0\n");

    assert_eq!(
        failing(&text),
        at("duplicate_name", pos_of_nth(&text, "\"alpha\"", 1))
    );
}

#[test]
fn name_of_the_wrong_type_is_wrong_type() {
    let text = base().replace("heading_deg = 90.0", "heading_deg = 90.0\nname = 5");

    assert_eq!(failing(&text), at("wrong_type", pos_of(&text, "5\n")));
}

#[test]
fn negative_speed_is_invalid_value() {
    let text = base().replace("heading_deg = 90.0", "heading_deg = 90.0\nspeed_mps = -1.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "speed_mps = "))
    );
}

#[test]
fn unknown_key_in_an_entity_is_unknown_key() {
    let text = base().replace("heading_deg = 90.0", "heading_deg = 90.0\ncolour = \"red\"");

    assert_eq!(failing(&text), at("unknown_key", pos_of(&text, "colour")));
}

#[test]
fn unknown_key_in_the_scenario_table_is_unknown_key() {
    let text = base().replace("seed = 7", "seed = 7\nplugins = []");

    assert_eq!(failing(&text), at("unknown_key", pos_of(&text, "plugins")));
}

#[test]
fn unknown_top_level_key_is_unknown_key() {
    let text = base() + "extra = 1\n";

    // Appended after the last entity table, `extra` belongs to the entity: still an unknown key.
    assert_eq!(failing(&text), at("unknown_key", pos_of(&text, "extra")));
}

/// ASM-058: one pipeline for templates and scenarios; any error anywhere fails the scenario, even
/// in a template no entity uses, and is located in the template's file.
#[test]
fn error_in_an_unreferenced_template_fails_the_scenario_at_the_template_file() {
    let broken = alpha_template("veh.unused", None, "\"bad\"");

    let found = errors(scenario(
        &src(SCN, &base()),
        &[template(), src("scn/templates/unused.toml", &broken)],
    ));

    assert_eq!(
        rows(&found),
        [row(
            "param_wrong_type",
            "scn/templates/unused.toml",
            pos_of(&broken, "\"bad\"")
        )]
    );
}

// --- AT-7: scenario table values (ASM-060) -----------------------------------------------------

#[test]
fn tick_rate_hz_zero_is_invalid_value() {
    let text = base().replace("tick_rate_hz = 30", "tick_rate_hz = 0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "tick_rate_hz = "))
    );
}

#[test]
fn tick_rate_hz_negative_is_invalid_value() {
    let text = base().replace("tick_rate_hz = 30", "tick_rate_hz = -1");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "tick_rate_hz = "))
    );
}

/// Whole hertz only (ASM-016, human decision 2026-10-17).
#[test]
fn tick_rate_hz_fractional_is_wrong_type() {
    let text = base().replace("tick_rate_hz = 30", "tick_rate_hz = 2.5");

    assert_eq!(
        failing(&text),
        at("wrong_type", pos_after(&text, "tick_rate_hz = "))
    );
}

#[test]
fn tick_rate_hz_as_string_is_wrong_type() {
    let text = base().replace("tick_rate_hz = 30", "tick_rate_hz = \"30\"");

    assert_eq!(
        failing(&text),
        at("wrong_type", pos_after(&text, "tick_rate_hz = "))
    );
}

#[test]
fn seed_negative_is_invalid_value() {
    let text = base().replace("seed = 7", "seed = -1");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "seed = "))
    );
}

#[test]
fn duration_ticks_is_ceil_of_seconds_times_hz() {
    let ticks = |d: &str| {
        loaded(&base().replace("duration_s = 10.0", &format!("duration_s = {d}"))).duration_ticks
    };

    assert_eq!(ticks("10.0"), 300);
    assert_eq!(ticks("2.5"), 75);
    assert_eq!(ticks("10.01"), 301);
    assert_eq!(ticks("0.01"), 1);
}

#[test]
fn duration_ticks_uses_the_scenario_tick_rate() {
    let text = base()
        .replace("tick_rate_hz = 30", "tick_rate_hz = 100")
        .replace("duration_s = 10.0", "duration_s = 1.5");

    assert_eq!(loaded(&text).duration_ticks, 150);
}

#[test]
fn duration_zero_is_invalid_value() {
    let text = base().replace("duration_s = 10.0", "duration_s = 0.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "duration_s = "))
    );
}

#[test]
fn duration_negative_is_invalid_value() {
    let text = base().replace("duration_s = 10.0", "duration_s = -2.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "duration_s = "))
    );
}

#[test]
fn duration_too_large_for_u64_ticks_is_invalid_value() {
    let text = base().replace("duration_s = 10.0", "duration_s = 1e300");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "duration_s = "))
    );
}

#[test]
fn clock_modes_map_to_clock_settings() {
    let clock =
        |c: &str| loaded(&base().replace("clock = \"afap\"", &format!("clock = \"{c}\""))).clock;

    assert_eq!(clock("afap"), ClockSetting::AsFastAsPossible);
    assert_eq!(clock("realtime"), ClockSetting::RealTime);
    assert_eq!(clock("stepped"), ClockSetting::Stepped);
}

#[test]
fn unknown_clock_mode_is_invalid_value() {
    let text = base().replace("clock = \"afap\"", "clock = \"warp\"");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "clock = "))
    );
}

#[test]
fn clock_scaled_with_time_scale_in_range_is_a_scaled_setting() {
    let text = base().replace("clock = \"afap\"", "clock = \"scaled\"\ntime_scale = 4.0");

    assert_eq!(loaded(&text).clock, ClockSetting::Scaled(4.0));
}

#[test]
fn clock_scaled_accepts_the_bounds_of_the_kernel_time_scale() {
    let scaled = |v: &str| {
        loaded(&base().replace(
            "clock = \"afap\"",
            &format!("clock = \"scaled\"\ntime_scale = {v}"),
        ))
        .clock
    };

    assert_eq!(scaled("0.001"), ClockSetting::Scaled(0.001));
    assert_eq!(scaled("1000000.0"), ClockSetting::Scaled(1_000_000.0));
}

#[test]
fn clock_scaled_requires_time_scale_in_range() {
    let low = base().replace(
        "clock = \"afap\"",
        "clock = \"scaled\"\ntime_scale = 0.0005",
    );
    let high = base().replace(
        "clock = \"afap\"",
        "clock = \"scaled\"\ntime_scale = 2000000.0",
    );

    assert_eq!(
        failing(&low),
        at("invalid_value", pos_after(&low, "time_scale = "))
    );
    assert_eq!(
        failing(&high),
        at("invalid_value", pos_after(&high, "time_scale = "))
    );
}

#[test]
fn clock_scaled_with_zero_time_scale_is_invalid_value() {
    let text = base().replace("clock = \"afap\"", "clock = \"scaled\"\ntime_scale = 0.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "time_scale = "))
    );
}

#[test]
fn time_scale_without_scaled_is_invalid_value() {
    let text = base().replace("clock = \"afap\"", "clock = \"afap\"\ntime_scale = 4.0");

    assert_eq!(
        failing(&text),
        at("invalid_value", pos_after(&text, "time_scale = "))
    );
}

// --- AT-1 (terrain) ----------------------------------------------------------------------------

#[test]
fn given_unknown_terrain_provider_then_unknown_id_error() {
    let text = base().replace(
        "[[entities]]",
        "[terrain]\nprovider = \"builtin.terrain.hilly\"\n\n[[entities]]",
    );

    let found = errors(load(&text));

    assert_eq!(
        rows(&found),
        at("unknown_id", pos_after(&text, "provider = "))
    );
    assert_eq!(
        found[0].message,
        "terrain provider `builtin.terrain.hilly` is not registered"
    );
    assert_eq!(found[0].notes.len(), 1);
    assert_eq!(
        found[0].notes[0].message,
        "registered terrain providers: builtin.terrain.flat, stub.terrain.flat"
    );
}

#[test]
fn terrain_params_are_validated_against_the_providers_schema() {
    let text = base().replace(
        "[[entities]]",
        "[terrain]\nprovider = \"builtin.terrain.flat\"\nparams = { height_m = \"high\" }\n\n[[entities]]",
    );

    let found = errors(load(&text));

    assert_eq!(
        rows(&found),
        at("param_wrong_type", pos_of(&text, "\"high\""))
    );
    assert_eq!(
        found[0].message,
        "terrain provider `builtin.terrain.flat`: parameter `height_m` must be a number, found string"
    );
}

#[test]
fn terrain_provider_without_params_when_a_param_is_required_is_missing_param() {
    let text = base().replace(
        "[[entities]]",
        "[terrain]\nprovider = \"builtin.terrain.flat\"\nparams = { }\n\n[[entities]]",
    );

    assert_eq!(
        failing(&text),
        at("missing_param", pos_after(&text, "params = "))
    );
}

// --- REQ-T-2: all errors of a scenario at once ---------------------------------------------------

#[test]
fn given_several_independent_scenario_errors_then_all_are_reported_in_file_order() {
    let text = base()
        .replace("seed = 7", "seed = -1")
        .replace("tick_rate_hz = 30", "tick_rate_hz = 0")
        .replace("template = \"veh.base\"", "template = \"veh.nope\"")
        .replace("heading_deg = 90.0", "heading_deg = \"east\"");

    assert_eq!(
        failing(&text),
        [
            row("invalid_value", SCN, pos_after(&text, "seed = ")),
            row("invalid_value", SCN, pos_after(&text, "tick_rate_hz = ")),
            row("unknown_template", SCN, pos_after(&text, "template = ")),
            row("wrong_type", SCN, pos_after(&text, "heading_deg = ")),
        ]
    );
}

#[test]
fn errors_in_scenario_and_template_files_are_reported_together_in_path_order() {
    let broken = alpha_template("veh.base", None, "0.0");
    let text = base().replace("seed = 7", "seed = -1");

    let found = errors(scenario(
        &src(SCN, &text),
        &[src("scn/templates/veh.toml", &broken)],
    ));

    assert_eq!(
        rows(&found),
        [
            row("invalid_value", SCN, pos_after(&text, "seed = ")),
            row(
                "param_out_of_range",
                "scn/templates/veh.toml",
                pos_of(&broken, "0.0")
            ),
        ]
    );
}

// --- AT-7: template_paths (file system) --------------------------------------------------------

/// `template_paths` are relative to the scenario file, not to the working directory.
#[test]
fn template_paths_are_relative_to_the_scenario_file() {
    let (registry, _) = registry();

    let s = loader(&registry)
        .load_scenario(&fixture_path("scenarios/relative/scenario.toml"))
        .unwrap_or_else(|d| {
            panic!(
                "{}",
                d.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });

    assert_eq!(
        s.templates.len(),
        2,
        "both templates of templates/ and its subdirectory"
    );
    assert_eq!(s.entities[0].template, tid("rel.veh"));
    assert_eq!(s.entities[0].motion.params, json!({ "a": 4.0 }));
}

#[test]
fn template_paths_naming_a_missing_directory_is_an_io_diagnostic() {
    let (registry, _) = registry();

    let found = errors(loader(&registry).load_scenario(&fixture_path(
        "scenarios/missing_template_dir/scenario.toml",
    )));

    // Review item 8 (ASM-066): the entity's unknown template is still reported, with a note.
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].code.as_str(), "io");
    assert_eq!(found[1].code.as_str(), "unknown_template");
    assert!(
        found[0].message.contains("no_such_dir"),
        "{}",
        found[0].message
    );
}
