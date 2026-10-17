//! The template file reader (ADR 0010, REQ-T-2, REQ-T-4, REQ-T-5, ASM-054, ASM-057).
//!
//! Reads one file: syntax, `schema_version`, structure, unknown keys and ID grammar. Everything
//! that needs other files or the registry (inheritance, component IDs, parameter schemas) is the
//! template set's job.

use std::collections::BTreeMap;
use std::path::PathBuf;

use cgf_core::{ComponentId, TemplateId};

use crate::SCHEMA_VERSION;
use crate::diagnostic::{Diagnostic, DiagnosticCode, sort_and_dedup};
use crate::reader::Reader;
use crate::source::SourceFile;
use crate::value::{LKind, LTable, LValue, Origin, parse};

const TEMPLATE_KEYS: [&str; 8] = [
    "schema_version",
    "id",
    "extends",
    "description",
    "motion",
    "behavior",
    "sensors",
    "network",
];

/// A component section (`[motion]`, `[behavior]` or one `[[sensors]]` entry) as written.
#[derive(Debug, Clone, PartialEq)]
pub struct RawComponent {
    /// Where the section starts.
    pub origin: Origin,
    /// The component ID (`model`, `ref` or `kind`) and where its value is; `None` when the section
    /// leaves it to the parent template.
    pub id: Option<(ComponentId, Origin)>,
    /// The `params` table as written.
    pub params: Option<LTable>,
}

/// A template as written in one file, before inheritance.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTemplate {
    /// The file.
    pub source: PathBuf,
    /// The template's ID and where it is written.
    pub id: (TemplateId, Origin),
    /// The parent template and where it is written.
    pub extends: Option<(TemplateId, Origin)>,
    /// The description.
    pub description: Option<String>,
    /// The `[motion]` section.
    pub motion: Option<RawComponent>,
    /// The `[behavior]` section.
    pub behavior: Option<RawComponent>,
    /// The `[[sensors]]` entries, `None` when the key is absent (an empty list replaces).
    pub sensors: Option<Vec<RawComponent>>,
    /// Opaque `network.<adapter>` tables, verbatim (REQ-T-4).
    pub network: BTreeMap<String, LTable>,
}

/// The result of reading one template file.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRead {
    /// The template, only when the file has no errors.
    pub template: Option<RawTemplate>,
    /// Every error of the file, sorted.
    pub diagnostics: Vec<Diagnostic>,
    /// The file's top-level `id` when it could be recovered, even if the file has errors, so the
    /// templates that extend it are not reported as having an unknown parent (ASM-054 rule 3).
    pub known_id: Option<TemplateId>,
    /// Whether the file has syntax errors (its tree is unreliable).
    pub syntax_errors: bool,
}

/// Reads `file` as a template.
pub fn read_template(file: &SourceFile) -> TemplateRead {
    let (table, mut diagnostics) = parse(file);
    let syntax_errors = diagnostics.iter().any(|d| d.code == DiagnosticCode::Syntax);
    let known_id = table.as_ref().and_then(recover_id);
    let mut template = None;
    if !syntax_errors && let Some(table) = &table {
        let mut reader = Reader::new(file);
        match check_version(&mut reader, table) {
            Version::Newer => diagnostics = Vec::new(),
            Version::Ok | Version::Bad => template = read_body(&mut reader, table, file),
        }
        diagnostics.extend(reader.into_diagnostics());
    }
    sort_and_dedup(&mut diagnostics);
    if !diagnostics.is_empty() {
        template = None;
    }
    TemplateRead {
        template,
        diagnostics,
        known_id,
        syntax_errors,
    }
}

/// The top-level `id` of a possibly broken file: a string with a real span that passes the ID
/// grammar.
fn recover_id(table: &LTable) -> Option<TemplateId> {
    let value = table.get("id")?;
    let span = value.origin().span();
    match value.kind() {
        LKind::String(s) if span.start < span.end => TemplateId::new(s).ok(),
        _ => None,
    }
}

enum Version {
    Ok,
    /// Missing, mistyped or below 1: reported, the rest of the file is still read.
    Bad,
    /// Newer than supported: reported, the file is not read further (ASM-057).
    Newer,
}

fn check_version(reader: &mut Reader<'_>, table: &LTable) -> Version {
    let Some(value) = table.get("schema_version") else {
        let message = "missing required key `schema_version`".to_string();
        reader.error(DiagnosticCode::MissingKey, message, table.origin());
        return Version::Bad;
    };
    let LKind::Integer(version) = value.kind() else {
        let message = format!(
            "`schema_version` must be an integer, found {}",
            value.kind().type_name()
        );
        reader.error(DiagnosticCode::WrongType, message, value.origin());
        return Version::Bad;
    };
    if *version > i64::from(SCHEMA_VERSION) {
        let message = format!(
            "schema_version {version} is newer than the newest supported version {SCHEMA_VERSION}"
        );
        reader.error(
            DiagnosticCode::UnsupportedSchemaVersion,
            message,
            value.origin(),
        );
        return Version::Newer;
    }
    if *version < 1 {
        let message = format!("schema_version must be at least 1, found {version}");
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return Version::Bad;
    }
    Version::Ok
}

fn read_body(reader: &mut Reader<'_>, table: &LTable, file: &SourceFile) -> Option<RawTemplate> {
    reader.unknown_keys(table, &TEMPLATE_KEYS);
    let id = match table.entry("id") {
        None => {
            let message = "missing required key `id`".to_string();
            reader.error(DiagnosticCode::MissingKey, message, table.origin());
            None
        }
        Some(entry) => reader
            .id("id", entry.value(), TemplateId::new)
            .map(|id| (id, entry.value().origin().clone())),
    };
    let extends = table.get("extends").and_then(|value| {
        reader
            .id("extends", value, TemplateId::new)
            .map(|id| (id, value.origin().clone()))
    });
    let description = table
        .get("description")
        .and_then(|value| reader.string("description", value))
        .map(str::to_string);
    let motion = table
        .get("motion")
        .and_then(|value| component(reader, "motion", "model", value));
    let behavior = table
        .get("behavior")
        .and_then(|value| component(reader, "behavior", "ref", value));
    let sensors = table
        .get("sensors")
        .and_then(|value| sensors(reader, value));
    let network = table
        .get("network")
        .map(|value| network(reader, value))
        .unwrap_or_default();
    Some(RawTemplate {
        source: file.path().to_path_buf(),
        id: id?,
        extends,
        description,
        motion,
        behavior,
        sensors,
        network,
    })
}

/// One component section: `id_key` (`model`, `ref` or `kind`) and `params`.
fn component(
    reader: &mut Reader<'_>,
    section: &str,
    id_key: &str,
    value: &LValue,
) -> Option<RawComponent> {
    let table = reader.table(section, value)?;
    reader.unknown_keys(table, &[id_key, "params"]);
    let id = table.get(id_key).and_then(|v| {
        reader
            .id(&format!("{section}.{id_key}"), v, ComponentId::new)
            .map(|id| (id, v.origin().clone()))
    });
    let params = table
        .get("params")
        .and_then(|v| reader.table(&format!("{section}.params"), v).cloned());
    Some(RawComponent {
        origin: table.origin().clone(),
        id,
        params,
    })
}

fn sensors(reader: &mut Reader<'_>, value: &LValue) -> Option<Vec<RawComponent>> {
    let LKind::Array(items) = value.kind() else {
        let message = format!(
            "`sensors` must be an array, found {}",
            value.kind().type_name()
        );
        reader.error(DiagnosticCode::WrongType, message, value.origin());
        return None;
    };
    let mut out = Vec::new();
    for item in items {
        let Some(sensor) = component(reader, "sensors", "kind", item) else {
            continue;
        };
        if item_has_no_kind(item) {
            let message = "missing required key `kind`".to_string();
            reader.error(DiagnosticCode::MissingKey, message, item.origin());
        }
        out.push(sensor);
    }
    Some(out)
}

fn item_has_no_kind(item: &LValue) -> bool {
    matches!(item.kind(), LKind::Table(t) if t.get("kind").is_none())
}

/// `network` is a table of tables named `[a-z][a-z0-9_]*`; the contents are kept as written.
fn network(reader: &mut Reader<'_>, value: &LValue) -> BTreeMap<String, LTable> {
    let mut out = BTreeMap::new();
    let Some(table) = reader.table("network", value) else {
        return out;
    };
    for (name, entry) in table.iter() {
        if !is_adapter_name(name) {
            let message = format!("`{name}` is not a valid adapter name (use [a-z][a-z0-9_]*)");
            reader.error(DiagnosticCode::InvalidValue, message, entry.key_origin());
            continue;
        }
        if let Some(adapter) = reader.table(&format!("network.{name}"), entry.value()) {
            out.insert(name.to_string(), adapter.clone());
        }
    }
    out
}

fn is_adapter_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests;
