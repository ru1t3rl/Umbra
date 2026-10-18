//! WI-15: the behavior file JSON Schema (`BEHAVIOR_FILE_SCHEMA`) for editors and
//! `cgf-cli schema behavior`. Requirements: D-14, REQ-B-6, REQ-B-7, REQ-T-3, ADR 0012, ADR 0014.
//!
//! Like `file_schemas.rs` this is the drift guard between a hand-written schema and the
//! hand-written reader: the schema accepts every valid graph file in the repository and rejects
//! every structurally invalid fixture that the reader also rejects. The reader is authoritative.

#![forbid(unsafe_code)]

mod common;

use std::path::{Path, PathBuf};

use cgf_core::ParamSchema;
use cgf_templates::value::parse;
use cgf_templates::{BEHAVIOR_FILE_SCHEMA, CompiledSchema, Diagnostic, SourceFile};
use common::{fixture_path, loader, registry};

fn compiled() -> CompiledSchema {
    let parsed = ParamSchema::from_json_str(BEHAVIOR_FILE_SCHEMA)
        .unwrap_or_else(|e| panic!("the behavior schema must be a JSON object: {e}"));
    CompiledSchema::compile(&parsed)
        .unwrap_or_else(|e| panic!("the behavior schema must compile in the subset: {e}"))
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// Sorted `.behavior.toml` files directly in `dir`.
fn graph_files(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".behavior.toml"))
        .collect();
    found.sort();
    found
}

fn source_at(path: &Path) -> SourceFile {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    SourceFile::new(path, text)
}

fn violations(schema: &CompiledSchema, file: &SourceFile) -> Vec<Diagnostic> {
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
    schema.validate(&root, "behavior file")
}

/// Every valid graph file in the repository: the built-in graphs, the example, the swap fixtures
/// and the graph fixtures of this crate that are structurally and semantically fine.
fn valid_files() -> Vec<PathBuf> {
    let graphs = fixture_path("graphs");
    let mut files = Vec::new();
    for dir in [
        repo("crates/cgf-engine/behaviors"),
        repo("examples/scenarios/patrol_and_engage/behaviors"),
        repo("crates/cgf-engine/tests/fixtures/behavior_swap/a/behaviors"),
        repo("crates/cgf-engine/tests/fixtures/behavior_swap/b/behaviors"),
        graphs.join("scenario_ok/templates"),
        graphs.join("scenario_bad_call/templates"),
        graphs.join("discovery_mixed/templates"),
        graphs.join("sibling_graphs"),
    ] {
        files.extend(graph_files(&dir));
    }
    // Structurally valid, but with a deliberate unknown key `colour`.
    files.retain(|p| !p.ends_with("discovery_mixed/templates/bad.behavior.toml"));
    files
}

#[test]
fn behavior_schema_compiles_in_subset_and_is_a_titled_json_document() {
    let _ = compiled();
    let value: serde_json::Value = serde_json::from_str(BEHAVIOR_FILE_SCHEMA)
        .unwrap_or_else(|e| panic!("the behavior schema is not JSON: {e}"));
    assert!(value["title"].is_string());
}

#[test]
fn schema_accepts_every_valid_graph_file() {
    let schema = compiled();
    let files = valid_files();

    assert!(files.len() >= 15, "found only {} files", files.len());
    for path in &files {
        assert_eq!(
            violations(&schema, &source_at(path)),
            [],
            "{}",
            path.display()
        );
    }
}

#[test]
fn schema_accepts_a_graph_with_all_twelve_node_types() {
    let schema = compiled();
    let all = r#"
schema_version = 1
id = "test.graph.all"
version = "1.0.0"
root = "seq"
[interface]
params = { n = { type = "integer", default = 2, minimum = 1 }, who = { type = "entity" } }
outputs = { seen = { type = "entity", from = "target" } }
result = "Running"
[blackboard]
target = "entity"
[nodes.seq]
type = "sequence"
memory = true
children = ["sel", "par", "inv", "sa", "rep", "cd", "to", "cond", "act", "sub", "machine"]
[nodes.sel]
type = "selector"
children = ["cond"]
[nodes.par]
type = "parallel"
policy = "any"
children = ["act"]
[nodes.inv]
type = "inverter"
child = "cond"
[nodes.sa]
type = "succeed_always"
child = "cond"
[nodes.rep]
type = "repeat"
count = 3
child = "act"
[nodes.cd]
type = "cooldown"
duration_s = 1.5
child = "act"
[nodes.to]
type = "timeout"
duration_s = 9.0
child = "act"
[nodes.cond]
type = "condition"
ref = "builtin.cond.chance"
params = { probability = 0.5 }
[nodes.act]
type = "action"
ref = "builtin.action.wait"
params = { duration_s = { param = "n" } }
[nodes.sub]
type = "subgraph"
ref = "test.graph.other"
version = "^1.0"
params = { who = { key = "target" } }
outputs = { seen = "target" }
[nodes.machine]
type = "fsm"
initial = "s"
states = { s = "act", t = "act" }
exits = { done = "success" }
transitions = [ { from = "s", to = "t", when = "cond" }, { from = "t", to = "done", on = "success" } ]
[editor]
positions = { seq = [0, 0] }
"#;

    let found = violations(&schema, &SourceFile::new("all.behavior.toml", all));

    assert_eq!(found, []);
}

#[test]
fn schema_rejects_every_structurally_invalid_fixture() {
    let schema = compiled();
    let files = graph_files(&fixture_path("graphs/invalid_structure"));

    assert_eq!(files.len(), 12);
    for path in files {
        let found = violations(&schema, &source_at(&path));
        assert!(!found.is_empty(), "the schema accepts {}", path.display());
    }
}

/// The reader and the schema agree on the invalid fixtures (the reader is authoritative).
#[test]
fn the_reader_also_rejects_every_structurally_invalid_fixture() {
    let (registry, _) = registry();
    let loader = loader(&registry);

    for path in graph_files(&fixture_path("graphs/invalid_structure")) {
        let read = loader.graphs_from_sources(&[source_at(&path)], cgf_core::Provenance::File);
        assert!(read.is_err(), "the reader accepts {}", path.display());
    }
}

#[test]
fn schema_rejects_a_string_schema_version() {
    let schema = compiled();
    let text = "schema_version = \"one\"\nid = \"test.graph.x\"\nversion = \"1.0.0\"\nroot = \"a\"\n\n[nodes.a]\ntype = \"action\"\nref = \"builtin.action.wait\"\n";

    let found = violations(&schema, &SourceFile::new("x.behavior.toml", text));

    assert_ne!(
        found.len(),
        0,
        "a string schema_version violates the schema"
    );
}
