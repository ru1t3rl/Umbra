//! One graph file as the set checks see it: its interface, blackboard and located node tables.

use std::collections::{BTreeMap, BTreeSet};

use umbra_core::{BbType, ParamSchema};

use super::bind::{Args, param_refs};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::graph_read::{GraphRead, Parts, as_table};
use crate::value::{LKind, LTable, LValue, Origin};

/// A graph file that has readable parts.
#[derive(Clone, Copy)]
pub(super) struct View<'a> {
    pub(super) read: &'a GraphRead,
    pub(super) parts: &'a Parts,
}

impl<'a> View<'a> {
    /// The view of `read`, when anything of it could be read.
    pub(super) fn of(read: &'a GraphRead) -> Option<Self> {
        read.parts.as_ref().map(|parts| Self { read, parts })
    }

    /// The table of node `name`.
    pub(super) fn node_table(&self, name: &str) -> Option<&'a LTable> {
        self.parts
            .table
            .get("nodes")
            .and_then(as_table)
            .and_then(|nodes| nodes.get(name))
            .and_then(as_table)
    }

    /// The type of a key a node can name: a declared `[blackboard]` key or an interface param (a
    /// read-only key, ADR 0014). The flag is true for a param. `None` when the key is not declared
    /// or the blackboard could not be read.
    pub(super) fn key(&self, key: &str) -> Option<(BbType, bool)> {
        let blackboard = self.parts.blackboard.as_ref()?;
        if let Some(&ty) = blackboard.get(key) {
            return Some((ty, false));
        }
        let ty = *self.parts.interface.as_ref()?.types.get(key)?;
        Some((ty, true))
    }

    /// The located `default` of interface param `name`.
    fn default_of(&self, name: &str) -> Option<LValue> {
        self.parts
            .table
            .get("interface")
            .and_then(as_table)?
            .get("params")
            .and_then(as_table)?
            .get(name)
            .and_then(as_table)?
            .get("default")
            .cloned()
    }

    /// The default of every interface param; `None` for a param without one (its value is not
    /// known at load time).
    pub(super) fn defaults(&self) -> Args {
        let Some(interface) = &self.parts.interface else {
            return Args::new();
        };
        interface
            .types
            .keys()
            .map(|name| (name.clone(), self.default_of(name)))
            .collect()
    }

    /// The `params` table of node `table`, or an empty one at the node (where a missing param is
    /// reported).
    pub(super) fn params_of(table: &LTable) -> LTable {
        match table.get("params").map(LValue::kind) {
            Some(LKind::Table(params)) => params.clone(),
            _ => LTable::from_entries(table.origin().clone(), BTreeMap::default()),
        }
    }

    /// `params` without the entries named in `names` (the run-time input bindings, which are not
    /// values).
    pub(super) fn without<'n>(params: &LTable, names: impl Iterator<Item = &'n String>) -> LTable {
        let skip: BTreeSet<&String> = names.collect();
        let entries = params
            .iter()
            .filter(|(key, _)| !skip.contains(&(*key).to_string()))
            .map(|(key, entry)| (key.to_string(), entry.clone()))
            .collect();
        LTable::from_entries(params.origin().clone(), entries)
    }

    /// Where a missing param of node `table` is reported: its `params` table, else the node.
    pub(super) fn params_origin(table: &LTable) -> Origin {
        table
            .get("params")
            .map_or_else(|| table.origin().clone(), |v| v.origin().clone())
    }
}

/// The JSON `type` of property `key` of an object schema.
fn property_type<'s>(schema: &'s ParamSchema, key: &str) -> Option<&'s str> {
    schema
        .as_json()
        .get("properties")?
        .get(key)?
        .get("type")?
        .as_str()
}

/// Whether a param of type `ty` can supply a value of JSON type `wanted` (`integer` satisfies
/// `number`; `entity` and `position` have no literal form).
fn compatible(ty: BbType, wanted: &str) -> bool {
    matches!(
        (ty, wanted),
        (BbType::Bool, "boolean")
            | (BbType::Number, "number")
            | (BbType::Integer, "integer" | "number")
            | (BbType::String, "string")
    )
}

/// Checks the `{ param }` references of `params` against the enclosing graph's interface: the
/// param must exist and, when the reference is a whole top-level value, its type must fit the
/// property of `target`. Reports `invalid_binding` at the reference and returns the names of the
/// bad parameters, whose values must not be substituted (ASM-076).
pub(super) fn check_refs(
    view: &View<'_>,
    target: Option<&ParamSchema>,
    params: &LTable,
    out: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut bad = BTreeSet::new();
    let Some(interface) = &view.parts.interface else {
        return bad;
    };
    for reference in param_refs(params) {
        let at = reference.value.origin();
        let problem = match interface.types.get(&reference.name) {
            None => Some(format!(
                "`{{ param = \"{0}\" }}` names no parameter of this graph's interface",
                reference.name
            )),
            Some(&ty) => reference
                .whole_of
                .zip(target)
                .and_then(|(key, schema)| property_type(schema, key).map(|w| (key, w)))
                .filter(|&(_, wanted)| !compatible(ty, wanted))
                .map(|(key, wanted)| {
                    format!(
                        "parameter `{}` is {ty}, but `{key}` needs {wanted}",
                        reference.name
                    )
                }),
        };
        if let Some(message) = problem {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidBinding,
                message,
                Some(at.location()),
            ));
            bad.insert(reference.name);
        }
    }
    bad
}
