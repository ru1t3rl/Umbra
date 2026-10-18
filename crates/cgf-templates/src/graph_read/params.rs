//! `params` tables of `condition`, `action` and `subgraph` nodes: the two reserved one-key forms
//! (ADR 0012, ADR 0014, ASM-076, ASM-089).
//!
//! `{ param = "x" }` stays in the parameters (a build-time reference). `{ key = "k" }` is a
//! run-time input binding: as a whole value of a `subgraph` parameter it moves to
//! `SubgraphRef::inputs`; anywhere else it is `invalid_binding`. The forms are classified with
//! `cgf_core::reserved_form`, applied to values inside `params` only, never to the table itself.
//! Whether a reference or key exists, and its type, is the graph set's job (WI-13).

use std::collections::BTreeMap;

use cgf_core::{ParamTemplate, ParamValue, ReservedForm, reserved_form};

use super::{as_table, check_name};
use crate::diagnostic::DiagnosticCode;
use crate::reader::Reader;
use crate::value::{LKind, LValue};

/// The parameters of one node.
pub(super) struct NodeParams {
    /// Literals and `{ param }` references.
    pub(super) template: ParamTemplate,
    /// Child param to parent blackboard key (`subgraph` only).
    pub(super) inputs: BTreeMap<String, String>,
}

impl NodeParams {
    /// No parameters (the key is absent).
    pub(super) fn none() -> Self {
        Self {
            template: ParamTemplate::new(ParamValue::Object(serde_json::Map::new())),
            inputs: BTreeMap::new(),
        }
    }
}

/// Reads a `params` value. `inputs_allowed` is true for `subgraph` nodes. `None` when bad.
pub(super) fn read(
    reader: &mut Reader<'_>,
    value: &LValue,
    inputs_allowed: bool,
) -> Option<NodeParams> {
    let table = reader.table("params", value)?;
    let mut rest = serde_json::Map::new();
    let mut inputs = BTreeMap::new();
    let mut ok = true;
    for (name, entry) in table.iter() {
        let value = entry.value();
        if let (true, Some(key)) = (inputs_allowed, key_form(value)) {
            if check_name(reader, &key, "blackboard key", value.origin()) {
                inputs.insert(name.to_string(), key);
            } else {
                ok = false;
            }
        } else {
            ok &= reject_keys(reader, value);
            rest.insert(name.to_string(), value.to_param());
        }
    }
    ok.then(|| NodeParams {
        template: ParamTemplate::new(ParamValue::Object(rest)),
        inputs,
    })
}

/// The parent key of a `{ key = .. }` value, classified with `cgf_core::reserved_form` (only
/// one-entry tables are looked at, so big values are never copied).
fn key_form(value: &LValue) -> Option<String> {
    if as_table(value)?.iter().count() != 1 {
        return None;
    }
    match reserved_form(&value.to_param())? {
        ReservedForm::Key(key) => Some(key.to_string()),
        ReservedForm::Param(_) => None,
    }
}

/// Reports every `{ key = .. }` form in `value` (at any depth) as `invalid_binding`; returns
/// whether there was none.
fn reject_keys(reader: &mut Reader<'_>, value: &LValue) -> bool {
    let mut ok = true;
    if key_form(value).is_some() {
        let message =
            "`{ key = .. }` is only allowed as a whole value of a `subgraph` node's `params`"
                .to_string();
        reader.error(DiagnosticCode::InvalidBinding, message, value.origin());
        return false;
    }
    match value.kind() {
        LKind::Table(table) => {
            for (_, entry) in table.iter() {
                ok &= reject_keys(reader, entry.value());
            }
        }
        LKind::Array(items) => {
            for item in items {
                ok &= reject_keys(reader, item);
            }
        }
        _ => {}
    }
    ok
}
