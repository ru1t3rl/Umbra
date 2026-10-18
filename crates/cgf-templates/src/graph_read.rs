//! The behavior graph file reader (ADR 0012, ADR 0014, REQ-B-6, REQ-B-7, REQ-B-14, REQ-B-16,
//! REQ-T-2, REQ-T-5, ASM-074, ASM-075, ASM-101).
//!
//! Reads one `*.behavior.toml` file into a [`GraphDef`]: syntax, `schema_version`, structure,
//! unknown keys, name and ID grammar, node settings, and the name lookups that building the
//! `GraphDef` needs (a child, state or guard name resolves to an index). Everything that needs
//! other files or the registry, and the whole-graph shape checks (reuse, cycles, unreachable
//! nodes and states, guards), belong to the graph set (WI-13).
//!
//! Sections: this file (top level, blackboard, `[editor]`), `interface` (ADR 0014), `nodes`
//! (node settings), `fsm` (the `fsm` node), `params` (`{ param }` and `{ key }` forms).

use std::collections::BTreeMap;
use std::path::Path;

use cgf_core::{
    BbType, BehaviorId, GraphDef, NameError, NodeId, Provenance, Version, validate_name,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, sort_and_dedup};
use crate::reader::Reader;
use crate::source::SourceFile;
use crate::template::{Version as FileVersion, check_version};
use crate::value::{LKind, LTable, LValue, Origin, parse};

mod fsm;
mod interface;
mod nodes;
mod params;

/// The file name suffix that makes a `.toml` file a behavior graph (ASM-074, case-sensitive).
const GRAPH_SUFFIX: &[u8] = b".behavior.toml";

/// The longest piece of user text echoed in a message.
const ECHO_LEN: usize = 40;

const GRAPH_KEYS: [&str; 9] = [
    "schema_version",
    "id",
    "version",
    "description",
    "root",
    "interface",
    "blackboard",
    "nodes",
    "editor",
];

/// The result of reading one graph file.
pub(crate) struct GraphRead {
    /// The graph, only when the file has no errors.
    pub(crate) graph: Option<GraphDef>,
    /// Every error of the file, sorted.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Whether `path` names a behavior graph (ASM-074): the file name ends in `.behavior.toml`.
pub(crate) fn is_graph_path(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.as_encoded_bytes().ends_with(GRAPH_SUFFIX))
}

/// Reads `file` as a graph; `source` is recorded in the [`GraphDef`].
pub(crate) fn read_graph(file: &SourceFile, source: Provenance) -> GraphRead {
    let (table, mut diagnostics) = parse(file);
    let syntax_errors = diagnostics.iter().any(|d| d.code == DiagnosticCode::Syntax);
    let mut graph = None;
    if !syntax_errors && let Some(table) = &table {
        let mut reader = Reader::new(file);
        match check_version(&mut reader, table) {
            FileVersion::Newer => diagnostics = Vec::new(),
            FileVersion::Ok | FileVersion::Bad => {
                graph = read_body(&mut reader, table, file, source);
            }
        }
        diagnostics.extend(reader.into_diagnostics());
    }
    sort_and_dedup(&mut diagnostics);
    if !diagnostics.is_empty() {
        graph = None;
    }
    debug_assert!(
        graph.is_some() || !diagnostics.is_empty(),
        "a graph that is not read must have a diagnostic"
    );
    GraphRead { graph, diagnostics }
}

fn read_body(
    reader: &mut Reader<'_>,
    table: &LTable,
    file: &SourceFile,
    source: Provenance,
) -> Option<GraphDef> {
    reader.unknown_keys(table, &GRAPH_KEYS);
    let id = read_id(reader, table);
    let version = read_version(reader, table);
    let description = table
        .get("description")
        .and_then(|value| reader.string("description", value))
        .map(str::to_string);
    let interface = interface::read(reader, table);
    let blackboard = table
        .get("blackboard")
        .map_or_else(|| Some(BTreeMap::new()), |v| read_blackboard(reader, v));
    if let Some(value) = table.get("editor") {
        // ASM-075: dropped after the parse-stage value checks, but it must be a table.
        let _ = reader.table("editor", value);
    }
    let node_table = nodes::read(reader, table);
    let root = read_root(reader, table, node_table.as_ref().map(|n| &n.index));
    let mut graph = GraphDef::new(id?, version?, source, interface?, root?, node_table?.defs)
        .with_origin(file.path().to_path_buf());
    if let Some(text) = description {
        graph = graph.with_description(text);
    }
    for (key, ty) in blackboard? {
        graph = graph.with_blackboard_key(key, ty);
    }
    Some(graph)
}

fn read_id(reader: &mut Reader<'_>, table: &LTable) -> Option<BehaviorId> {
    let Some(value) = table.get("id") else {
        missing_key(reader, table, "id");
        return None;
    };
    reader.id("id", value, BehaviorId::new)
}

/// `version`: a semver string (REQ-B-14), `invalid_version` at the value otherwise.
fn read_version(reader: &mut Reader<'_>, table: &LTable) -> Option<Version> {
    let Some(value) = table.get("version") else {
        missing_key(reader, table, "version");
        return None;
    };
    let text = reader.string("version", value)?;
    match Version::parse(text) {
        Ok(version) => Some(version),
        Err(error) => {
            let message = format!(
                "`version` {:?} is not a semantic version (MAJOR.MINOR.PATCH): {error}",
                shown(text)
            );
            reader.error(DiagnosticCode::InvalidVersion, message, value.origin());
            None
        }
    }
}

/// `root`: the name of a node; an unknown name is `unknown_node` at the value.
fn read_root(
    reader: &mut Reader<'_>,
    table: &LTable,
    index: Option<&BTreeMap<String, NodeId>>,
) -> Option<NodeId> {
    let Some(value) = table.get("root") else {
        missing_key(reader, table, "root");
        return None;
    };
    let name = reader.string("root", value)?;
    let id = index?.get(name).copied();
    if id.is_none() {
        let message = format!(
            "`root` names node `{0}`, which no `[nodes.{0}]` table defines",
            shown(name)
        );
        reader.error(DiagnosticCode::UnknownNode, message, value.origin());
    }
    id
}

/// `[blackboard]`: key name to type. `None` when any entry is bad.
fn read_blackboard(reader: &mut Reader<'_>, value: &LValue) -> Option<BTreeMap<String, BbType>> {
    let table = reader.table("blackboard", value)?;
    let mut out = BTreeMap::new();
    let mut ok = true;
    for (key, entry) in table.iter() {
        ok &= check_name(reader, key, "blackboard key", entry.key_origin());
        match bb_type(reader, entry.value()) {
            Some(ty) => {
                out.insert(key.to_string(), ty);
            }
            None => ok = false,
        }
    }
    ok.then_some(out)
}

/// A blackboard type name (`bool`, `number`, `integer`, `string`, `entity`, `position`).
pub(super) fn bb_type(reader: &mut Reader<'_>, value: &LValue) -> Option<BbType> {
    let text = reader.string("type", value)?;
    let ty = BbType::from_name(text);
    if ty.is_none() {
        let message = format!(
            "unknown type `{}` (use bool, number, integer, string, entity or position)",
            shown(text)
        );
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    ty
}

/// Checks `name` against the one segment grammar (`cgf_core::validate_name`) and reports
/// `invalid_value` at `at` with a bounded echo. Returns whether the name is valid.
pub(super) fn check_name(reader: &mut Reader<'_>, name: &str, what: &str, at: &Origin) -> bool {
    match validate_name(name) {
        Ok(()) => true,
        Err(error) => {
            // `TooLong` already echoes a bounded prefix; the others do not name the offender.
            let message = if matches!(error, NameError::TooLong { .. }) {
                format!("{what} is not valid: {error}")
            } else {
                format!("{what} `{}` is not valid: {error}", shown(name))
            };
            reader.error(DiagnosticCode::InvalidValue, message, at);
            false
        }
    }
}

/// `missing_key` for a required top-level key, located at the table (the plan leaves the place of
/// an absent key open, as in M2).
fn missing_key(reader: &mut Reader<'_>, table: &LTable, key: &str) {
    let message = format!("missing required key `{key}`");
    reader.error(DiagnosticCode::MissingKey, message, table.origin());
}

/// A user-supplied string for a message: at most [`ECHO_LEN`] characters, so a huge value never
/// floods a diagnostic.
pub(super) fn shown(text: &str) -> String {
    if text.chars().count() <= ECHO_LEN {
        text.to_string()
    } else {
        let head: String = text.chars().take(ECHO_LEN).collect();
        format!("{head}...")
    }
}

/// Whether a value is a table (used where a `{ .. }` form is looked for).
pub(super) fn as_table(value: &LValue) -> Option<&LTable> {
    match value.kind() {
        LKind::Table(table) => Some(table),
        _ => None,
    }
}
