//! AT-17 (engine rows): the default tick rate end to end. Requirements: REQ-C-1, REQ-K-4,
//! REQ-T-5, ADR 0010 (note of 2026-10-19), ADR 0029, ASM-168.
//!
//! The scenarios are the walking-skeleton example with its explicit `tick_rate_hz = 30` line
//! removed (or kept), written to a scratch directory under `CARGO_TARGET_TMPDIR`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use core::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};

use common::{example_dir, instantiate_with, registry, run_silent};
use umbra_core::StateHash;
use umbra_engine::scenario::{
    LoadSettings, Scenario, TickRateSource, load_scenario_with_settings,
    validate_path_with_settings,
};
use umbra_engine::{RunUntil, TickRate};

/// A copy of the example under `tag` whose `tick_rate_hz = 30` line is kept or dropped.
fn copy_example(tag: &str, keep_rate: bool) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("tick-rate-{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).expect("mkdir");
    let source = example_dir();
    for entry in fs::read_dir(source.join("templates")).expect("templates") {
        let path = entry.expect("entry").path();
        fs::copy(
            &path,
            dir.join("templates").join(path.file_name().expect("name")),
        )
        .expect("copy");
    }
    let text = fs::read_to_string(source.join("scenario.toml")).expect("scenario");
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| keep_rate || !l.starts_with("tick_rate_hz"))
        .collect();
    let text = kept.join("\n") + "\n";
    fs::write(dir.join("scenario.toml"), text).expect("write");
    dir.join("scenario.toml")
}

fn settings(hz: u32) -> LoadSettings {
    let mut s = LoadSettings::default();
    s.default_tick_rate = TickRate::new(hz).expect("nonzero");
    s
}

fn load(path: &Path, hz: u32) -> Scenario {
    load_scenario_with_settings(&registry(), path, &settings(hz)).expect("loads")
}

fn final_hash(scenario: &Scenario, ticks: u64) -> StateHash {
    let mut run = instantiate_with(scenario, Duration::ZERO, None, None);
    run_silent(&mut run.engine, RunUntil::Ticks(ticks));
    run.engine.state_hash()
}

#[test]
fn scenario_without_rate_runs_at_60_hz() {
    let path = copy_example("no-rate", false);
    let scenario =
        load_scenario_with_settings(&registry(), &path, &LoadSettings::default()).expect("loads");

    assert_eq!(scenario.tick_rate.hz(), 60);
    assert_eq!(scenario.tick_rate_source, TickRateSource::Default);
    assert_eq!(scenario.duration_ticks, 240 * 60);
    let mut run = instantiate_with(&scenario, Duration::ZERO, None, None);
    run_silent(&mut run.engine, RunUntil::Ticks(60));
    assert_eq!(run.engine.now().rate().hz(), 60);
    assert_eq!(run.engine.now().tick(), 60);
}

#[test]
fn configured_default_30_equals_the_explicit_30_file_tick_by_tick() {
    let defaulted = load(&copy_example("def30", false), 30);
    let explicit = load(&copy_example("exp30", true), 60);

    assert_eq!(defaulted.tick_rate_source, TickRateSource::Default);
    assert_eq!(explicit.tick_rate_source, TickRateSource::Scenario);
    assert_eq!(defaulted.duration_ticks, explicit.duration_ticks);
    let mut a = instantiate_with(&defaulted, Duration::ZERO, None, None);
    let mut b = instantiate_with(&explicit, Duration::ZERO, None, None);
    assert_eq!(a.engine.state_hash(), b.engine.state_hash(), "tick 0");
    for tick in 1..=300 {
        run_silent(&mut a.engine, RunUntil::Ticks(tick));
        run_silent(&mut b.engine, RunUntil::Ticks(tick));
        assert_eq!(a.engine.state_hash(), b.engine.state_hash(), "tick {tick}");
    }
}

#[test]
fn different_defaults_give_different_hashes_and_report_why() {
    let path = copy_example("two-defaults", false);
    let (a, b) = (load(&path, 30), load(&path, 60));

    assert_ne!(final_hash(&a, 300), final_hash(&b, 300));
    assert_eq!((a.tick_rate.hz(), b.tick_rate.hz()), (30, 60));
    assert_eq!(a.tick_rate_source, TickRateSource::Default);
    assert_eq!(b.tick_rate_source, TickRateSource::Default);
}

#[test]
fn validate_and_run_agree_on_duration_ticks_under_a_default() {
    let path = copy_example("agree", false);
    let settings = settings(25);

    let report = validate_path_with_settings(&registry(), &path, &settings).expect("validates");
    let scenario = load_scenario_with_settings(&registry(), &path, &settings).expect("loads");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    // Both read the same file with the same settings, so they report the same file and rate;
    // `duration_ticks` is a pure function of that rate (`duration_ticks_use_the_effective_rate`).
    assert_eq!(
        report.defaulted_tick_rates,
        vec![(scenario.source.clone(), scenario.tick_rate)]
    );
    assert_eq!(scenario.tick_rate.hz(), 25);
    assert_eq!(scenario.duration_ticks, 240 * 25);
}

/// Extra (AT-17 review): the run side of "validate and run agree". The instantiated run reports the
/// same `duration_ticks` as the loaded scenario under a configured default, and stops its tick
/// counter at the effective rate's duration, not the 60 Hz one.
#[test]
fn instantiated_run_reports_the_duration_ticks_of_the_effective_rate() {
    let scenario = load(&copy_example("run-duration", false), 25);

    let run = instantiate_with(&scenario, Duration::ZERO, None, None);

    assert_eq!(run.duration_ticks, scenario.duration_ticks);
    assert_eq!(run.duration_ticks, 240 * 25);
    assert_eq!(run.engine.now().rate().hz(), 25);
}
