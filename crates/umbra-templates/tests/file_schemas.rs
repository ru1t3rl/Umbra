//! AT-10 (WI-12): the file-format JSON Schemas (`TEMPLATE_FILE_SCHEMA`, `SCENARIO_FILE_SCHEMA`)
//! for editors and `umbra schema`. Requirements: D-14, REQ-T-3, REQ-X-2 (`schema`), ADR 0010.
//!
//! The schemas are hand-written next to the hand-written readers, so they can drift. These tests
//! are the drift guard: the compiled schemas accept every valid fixture and the example files, and
//! reject every structurally invalid fixture that the reader also rejects. The reader is authoritative.

#![forbid(unsafe_code)]

mod common;

use std::path::{Path, PathBuf};

use common::{fixture_path, fixture_source, loader, registry};
use umbra_core::ParamSchema;
use umbra_templates::value::parse;
use umbra_templates::{
    CompiledSchema, Diagnostic, SCENARIO_FILE_SCHEMA, SourceFile, TEMPLATE_FILE_SCHEMA,
};

fn compiled(schema: &str) -> CompiledSchema {
    let parsed = ParamSchema::from_json_str(schema)
        .unwrap_or_else(|e| panic!("the file schema must be a JSON object: {e}"));
    CompiledSchema::compile(&parsed).unwrap_or_else(|e| panic!("the file schema must compile: {e}"))
}

/// Sorted `.toml` files directly in `dir`.
fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    found.sort();
    found
}

fn source_at(path: &Path) -> SourceFile {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    SourceFile::new(path, text)
}

fn violations(schema: &CompiledSchema, file: &SourceFile, what: &str) -> Vec<Diagnostic> {
    let (table, syntax) = parse(file);
    assert_eq!(
        syntax.len(),
        0,
        "{} must parse: {syntax:?}",
        file.path().display()
    );
    let root = table
        .unwrap_or_else(|| panic!("{} has no root table", file.path().display()))
        .into_value();
    schema.validate(&root, what)
}

fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/walking_skeleton")
}

#[test]
fn file_schemas_compile_in_subset() {
    let _ = compiled(TEMPLATE_FILE_SCHEMA);
    let _ = compiled(SCENARIO_FILE_SCHEMA);
}

#[test]
fn file_schemas_are_json_documents_with_a_title() {
    for text in [TEMPLATE_FILE_SCHEMA, SCENARIO_FILE_SCHEMA] {
        let value: serde_json::Value =
            serde_json::from_str(text).unwrap_or_else(|e| panic!("file schema is not JSON: {e}"));
        assert!(value["title"].is_string(), "every file schema has a title");
    }
}

#[test]
fn example_and_valid_fixtures_satisfy_file_schemas() {
    let template_schema = compiled(TEMPLATE_FILE_SCHEMA);
    let scenario_schema = compiled(SCENARIO_FILE_SCHEMA);
    let mut templates = toml_files(&fixture_path("templates/valid"));
    templates.extend(toml_files(&example_dir().join("templates")));
    let mut scenarios = toml_files(&fixture_path("scenarios/valid"));
    scenarios.push(example_dir().join("scenario.toml"));

    for path in &templates {
        let found = violations(&template_schema, &source_at(path), "template file");
        assert_eq!(found, [], "{}", path.display());
    }
    for path in &scenarios {
        let found = violations(&scenario_schema, &source_at(path), "scenario file");
        assert_eq!(found, [], "{}", path.display());
    }
}

#[test]
fn structurally_invalid_fixtures_violate_file_schemas() {
    let template_schema = compiled(TEMPLATE_FILE_SCHEMA);
    let scenario_schema = compiled(SCENARIO_FILE_SCHEMA);

    for path in toml_files(&fixture_path("templates/invalid_structure")) {
        let found = violations(&template_schema, &source_at(&path), "template file");
        assert!(
            !found.is_empty(),
            "the template schema accepts {}",
            path.display()
        );
    }
    for path in toml_files(&fixture_path("scenarios/invalid_structure")) {
        let found = violations(&scenario_schema, &source_at(&path), "scenario file");
        assert!(
            !found.is_empty(),
            "the scenario schema accepts {}",
            path.display()
        );
    }
}

/// The reader and the schema agree on the invalid fixtures (the reader is authoritative).
#[test]
fn the_reader_also_rejects_every_structurally_invalid_fixture() {
    let (registry, _) = registry();
    let loader = loader(&registry);

    for path in toml_files(&fixture_path("templates/invalid_structure")) {
        assert!(
            loader.templates_from_sources(&[source_at(&path)]).is_err(),
            "the reader accepts {}",
            path.display()
        );
    }
    for path in toml_files(&fixture_path("scenarios/invalid_structure")) {
        assert!(
            loader
                .scenario_from_sources(&source_at(&path), &[])
                .is_err(),
            "the reader accepts {}",
            path.display()
        );
    }
}

/// The reader accepts the valid template fixtures that name only registered IDs.
#[test]
fn the_reader_accepts_the_valid_template_fixtures_that_use_registered_ids() {
    let (registry, _) = registry();
    let files = [
        fixture_source("templates/valid/minimal.toml"),
        fixture_source("templates/valid/inheriting.toml"),
    ];

    let result = loader(&registry).templates_from_sources(&files);

    assert!(result.is_ok(), "{:?}", result.err().map(|d| d.to_string()));
}

/// ASM-055: the behavior and sensor sections are structurally valid; only their IDs are unknown.
#[test]
fn the_reader_reports_only_unknown_ids_for_the_behavior_and_sensor_fixture() {
    let (registry, _) = registry();
    let file = fixture_source("templates/valid/with_behavior_and_sensors.toml");

    let found = common::errors(loader(&registry).templates_from_sources(&[file]));

    assert_eq!(found.len(), 3, "behavior ref and two sensor kinds");
    assert!(found.iter().all(|d| d.code.as_str() == "unknown_id"));
}

#[test]
fn schemas_are_served_as_constants_that_do_not_depend_on_the_registry() {
    // `umbra schema template|scenario` prints these; the text must be stable and non-trivial.
    assert!(TEMPLATE_FILE_SCHEMA.len() > 100);
    assert!(SCENARIO_FILE_SCHEMA.len() > 100);
}
