//! AT-17 (reader rows): the default tick rate. Requirements: REQ-C-1, REQ-T-2, REQ-T-5, REQ-K-4,
//! ADR 0010 (note of 2026-10-19), ADR 0029, ASM-016, ASM-168.
//!
//! `[scenario] tick_rate_hz` is optional: an absent key takes `LoaderOptions::default_tick_rate`
//! (60 Hz unless changed), a present key is checked exactly as before and always wins.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{alpha_template, errors, lines, loader, ok, registry, rows, src};
use serde_json::Value;
use umbra_core::hash::fnv1a64;
use umbra_core::{DEFAULT_TICK_RATE_HZ, TickRate};
use umbra_templates::{
    Loader, LoaderOptions, SCENARIO_FILE_SCHEMA, SCHEMA_VERSION, Scenario, SourceFile,
    TEMPLATE_FILE_SCHEMA, TickRateSource,
};

const SCN: &str = "scn/scenario.toml";
/// FNV-1a 64 of `schemas/template.v1.schema.json` at the tag `m4-done` (2152 bytes). The template
/// schema is frozen by the 2026-10-19 decision, so any byte change is a failure.
const TEMPLATE_SCHEMA_FNV_AT_M4_DONE: u64 = 0x5537_ea20_499f_506c;

fn text(rate_line: Option<&str>, duration: &str) -> String {
    let mut l = vec![
        "schema_version = 1",
        "",
        "[scenario]",
        "name = \"t\"",
        "seed = 7",
    ];
    l.extend(rate_line);
    let duration = format!("duration_s = {duration}");
    l.extend([
        "clock = \"afap\"",
        duration.as_str(),
        "template_paths = [\"templates\"]",
    ]);
    l.extend([
        "",
        "[[entities]]",
        "template = \"veh.base\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
    ]);
    lines(&l)
}

fn template() -> SourceFile {
    src(
        "scn/templates/veh.toml",
        &alpha_template("veh.base", None, "2.0"),
    )
}

fn load_with(loader: &Loader<'_>, text: &str) -> Result<Scenario, umbra_templates::Diagnostics> {
    loader.scenario_from_sources(&src(SCN, text), &[template()])
}

fn load_default(rate_line: Option<&str>, duration: &str) -> Scenario {
    let (registry, _) = registry();
    ok(load_with(&loader(&registry), &text(rate_line, duration)))
}

fn load_with_default(hz: u32, rate_line: Option<&str>, duration: &str) -> Scenario {
    let (registry, _) = registry();
    let mut options = LoaderOptions::default();
    options.default_tick_rate = TickRate::new(hz).expect("nonzero");
    let l = Loader::with_options(&registry, options).expect("loader");
    ok(load_with(&l, &text(rate_line, duration)))
}

#[test]
fn absent_tick_rate_is_60_hz_from_default() {
    let s = load_default(None, "10.0");

    assert_eq!(DEFAULT_TICK_RATE_HZ, 60);
    assert_eq!(s.tick_rate.hz(), 60);
    assert_eq!(s.tick_rate_source, TickRateSource::Default);
    assert_eq!(LoaderOptions::default().default_tick_rate.hz(), 60);
}

#[test]
fn absent_tick_rate_uses_loader_option() {
    let s = load_with_default(25, None, "10.0");

    assert_eq!(s.tick_rate.hz(), 25);
    assert_eq!(s.tick_rate_source, TickRateSource::Default);
}

#[test]
fn explicit_value_always_wins_over_any_default() {
    for default in [1, 25, 60, 240] {
        let s = load_with_default(default, Some("tick_rate_hz = 30"), "10.0");

        assert_eq!(s.tick_rate.hz(), 30, "default {default}");
        assert_eq!(s.tick_rate_source, TickRateSource::Scenario);
    }
}

#[test]
fn source_is_scenario_or_default() {
    assert_eq!(
        load_default(Some("tick_rate_hz = 30"), "1.0").tick_rate_source,
        TickRateSource::Scenario
    );
    assert_eq!(
        load_default(None, "1.0").tick_rate_source,
        TickRateSource::Default
    );
    // An explicit value equal to the default is still `Scenario`.
    assert_eq!(
        load_default(Some("tick_rate_hz = 60"), "1.0").tick_rate_source,
        TickRateSource::Scenario
    );
}

#[test]
fn duration_ticks_use_the_effective_rate() {
    assert_eq!(load_default(None, "10.0").duration_ticks, 600);
    assert_eq!(load_with_default(25, None, "10.0").duration_ticks, 250);
    assert_eq!(
        load_default(Some("tick_rate_hz = 30"), "10.0").duration_ticks,
        300
    );
    // The float-artifact rule of ASM-066 applies at the default rate too: 0.1 s at 30 Hz is 3.
    assert_eq!(load_with_default(30, None, "0.1").duration_ticks, 3);
}

#[test]
fn zero_negative_fraction_string_still_rejected() {
    let (registry, _) = registry();
    let l = loader(&registry);
    for (line, code) in [
        ("tick_rate_hz = 0", "invalid_value"),
        ("tick_rate_hz = -1", "invalid_value"),
        ("tick_rate_hz = 2.5", "wrong_type"),
        ("tick_rate_hz = \"30\"", "wrong_type"),
    ] {
        let found = rows(&errors(load_with(&l, &text(Some(line), "10.0"))));

        assert_eq!(found.len(), 1, "{line}: {found:?}");
        assert_eq!(found[0].0, code, "{line}");
        assert_eq!(found[0].1, std::path::Path::new(SCN), "{line}");
        assert_eq!(found[0].2, 6, "{line}");
    }
}

#[test]
fn schema_no_longer_requires_tick_rate_and_still_types_it() {
    let schema: Value = serde_json::from_str(SCENARIO_FILE_SCHEMA).expect("schema is JSON");
    let section = &schema["properties"]["scenario"];

    let required: Vec<&str> = section["required"]
        .as_array()
        .expect("required list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(required, ["name", "seed", "duration_s"]);
    let property = &section["properties"]["tick_rate_hz"];
    assert_eq!(property["type"], "integer");
    assert_eq!(property["minimum"], 1);
}

#[test]
fn schema_version_stays_1_and_template_schema_unchanged() {
    assert_eq!(SCHEMA_VERSION, 1);
    let schema: Value = serde_json::from_str(SCENARIO_FILE_SCHEMA).expect("schema is JSON");
    assert_eq!(schema["properties"]["schema_version"]["maximum"], 1);
    assert_eq!(
        fnv1a64(TEMPLATE_FILE_SCHEMA.as_bytes()),
        TEMPLATE_SCHEMA_FNV_AT_M4_DONE
    );
}

/// Writes `scenario.toml` (from `scenario`) and `templates/veh.toml` (from `template`) under `dir`.
fn write_scenario_dir(dir: &Path, scenario: &str, template: &str) -> PathBuf {
    fs::create_dir_all(dir.join("templates")).expect("mkdir");
    fs::write(dir.join("templates/veh.toml"), template).expect("write template");
    fs::write(dir.join("scenario.toml"), scenario).expect("write scenario");
    dir.join("scenario.toml")
}

/// ASM-168 (c): only valid scenarios are reported as defaulted, in file and in directory mode.
#[test]
fn defaulted_rates_list_only_valid_scenarios_in_file_and_directory_mode() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tick-rate-defaulted-report");
    let _ = fs::remove_dir_all(&root);
    let good_template = alpha_template("veh.base", None, "2.0");
    let good = write_scenario_dir(&root.join("good"), &text(None, "1.0"), &good_template);
    let bad_key = write_scenario_dir(
        &root.join("bad_key"),
        &text(Some("bogus = 1"), "1.0"),
        &good_template,
    );
    let bad_template = write_scenario_dir(
        &root.join("bad_template"),
        &text(None, "1.0"),
        &(good_template.clone() + "bogus = 1\n"),
    );
    // A template directory with a file that is not UTF-8: an `io` error before any build.
    let bad_io = write_scenario_dir(&root.join("bad_io"), &text(None, "1.0"), &good_template);
    fs::write(root.join("bad_io/templates/zz.toml"), b"\xff\xfe = 1\n").expect("write");
    let (registry, _) = registry();
    let l = loader(&registry);

    let report = l.validate_path(&root);
    assert!(
        !report.diagnostics.is_empty(),
        "the bad scenarios report errors"
    );
    assert_eq!(
        report.defaulted_tick_rates,
        vec![(good.clone(), TickRate::default())]
    );
    for bad in [&bad_key, &bad_template, &bad_io] {
        let report = l.validate_path(bad);
        assert!(!report.diagnostics.is_empty(), "{}", bad.display());
        assert!(report.defaulted_tick_rates.is_empty(), "{}", bad.display());
    }
    assert_eq!(
        l.validate_path(&good).defaulted_tick_rates,
        vec![(good, TickRate::default())]
    );
}
