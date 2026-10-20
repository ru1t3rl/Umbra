//! AT-4 / AT-19 (engine rows, M5 WI-9, ASM-173): with a confinement, every answer that carries a
//! path (load diagnostics, template sources, plugin origins, graph-validation diagnostics) shows it
//! in display form: no absolute path and nothing that names the scenario root.
//! Requirements: REQ-X-1, REQ-X-5, REQ-X-11, ADR 0026.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod handle_support;
mod plugin_support;

use std::path::{Path, PathBuf};

use handle_support::{config, examples, plugin_lock, scenario_copy, start_engine};
use umbra_engine::Confinement;

/// A confined engine config over `root`, with `plugin_roots` as the plugin directories.
fn confined(root: &Path, plugin_roots: &[PathBuf]) -> umbra_engine::handle::HandleConfig {
    let mut config = config();
    config.load.confinement = Some(Confinement::new(root, plugin_roots).expect("confinement"));
    config
}

fn root_of(scenario: &Path) -> PathBuf {
    std::fs::canonicalize(scenario.parent().expect("a parent")).expect("canonical root")
}

fn assert_clean(text: &str, root: &Path) {
    let root = root.display().to_string();
    assert!(!text.contains(&root), "{text} names the root {root}");
}

#[test]
fn template_sources_are_shown_relative_to_the_root() {
    let scenario = scenario_copy(
        &examples().join("walking_skeleton"),
        "paths-templates",
        |t| t,
    );
    let root = root_of(&scenario);
    let (handle, _thread) = start_engine(confined(&root, &[]));
    let info = handle.load(scenario, None, None).expect("load");

    let templates = handle.templates().expect("templates");

    assert_eq!(info.path, "scenario.toml");
    assert_ne!(templates.len(), 0);
    for template in &templates {
        let source = template.source.display().to_string();
        assert!(
            source.starts_with("templates/"),
            "a root-relative source: {source}"
        );
        assert_clean(&source, &root);
    }
}

#[test]
fn load_diagnostics_are_shown_relative_to_the_root() {
    let scenario = scenario_copy(
        &examples().join("walking_skeleton"),
        "paths-diagnostics",
        |t| t.replace("template = ", "template = \"no.such\" #"),
    );
    let root = root_of(&scenario);
    let (handle, _thread) = start_engine(confined(&root, &[]));

    let error = handle
        .load(scenario, None, None)
        .expect_err("the scenario is invalid");

    let text = format!("{error:?} {error}");
    assert!(text.contains("scenario.toml"), "{text}");
    assert_clean(&text, &root);
}

#[test]
fn validation_diagnostics_are_shown_without_the_root() {
    // An unreadable sibling graph becomes an `io` diagnostic whose message quotes its path.
    let scenario = scenario_copy(
        &examples().join("patrol_and_engage"),
        "paths-validate",
        |t| t,
    );
    let root = root_of(&scenario);
    let (handle, _thread) = start_engine(confined(&root, &[]));
    handle.load(scenario, None, None).expect("load");
    std::fs::remove_file(root.join("behaviors/attack.behavior.toml")).expect("remove a graph");

    let validation = handle
        .validate_graph("draft.behavior.toml".to_owned(), String::new())
        .expect("validate");

    let text = format!("{:?}", validation.diagnostics);
    assert!(
        text.contains("attack.behavior.toml"),
        "the io diagnostic is there: {text}"
    );
    assert_clean(&text, &root);
}

#[test]
fn plugin_origins_are_shown_in_display_form() {
    let _lock = plugin_lock();
    let dir = plugin_support::scratch_copy(&plugin_support::wander_example_dir(), "paths-plugins");
    let plugins = plugin_support::wander();
    let mut with_plugins = confined(&dir, std::slice::from_ref(&plugins));
    with_plugins.load.extra_plugin_paths = vec![plugins.clone()];
    let (handle, _thread) = start_engine(with_plugins);
    handle
        .load(dir.join("scenario.toml"), None, None)
        .expect("load");

    let report = handle.plugins().expect("plugins");

    assert_ne!(report.plugins.len(), 0);
    for plugin in &report.plugins {
        assert!(
            plugin.origin.starts_with("plugins[0]/"),
            "{}",
            plugin.origin
        );
        assert_clean(&plugin.origin, &plugins);
    }
}

#[test]
fn without_a_confinement_paths_keep_their_form() {
    let scenario = scenario_copy(&examples().join("walking_skeleton"), "paths-plain", |t| t);
    let (handle, _thread) = start_engine(config());
    let info = handle.load(scenario.clone(), None, None).expect("load");

    let templates = handle.templates().expect("templates");

    assert_eq!(info.path, scenario.display().to_string());
    assert!(
        templates[0]
            .source
            .starts_with(scenario.parent().expect("a parent"))
    );
}
