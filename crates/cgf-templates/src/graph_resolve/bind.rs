//! `{ param = .. }` references in located `params` tables: finding them, substituting values and
//! pointing a diagnostic back at the reference that supplied a value (ASM-076, ASM-106).

use std::collections::BTreeMap;

use cgf_core::{ReservedForm, reserved_form};

use crate::diagnostic::{Diagnostic, Note};
use crate::graph_read::as_table;
use crate::value::{LEntry, LKind, LTable, LValue, Origin};

/// The value of each interface parameter by name; `None` when it is not known at load time (no
/// default, or bound at run time).
pub(super) type Args = BTreeMap<String, Option<LValue>>;

/// A `{ param = .. }` form inside a `params` table.
pub(super) struct ParamRef<'t> {
    /// The referenced interface parameter.
    pub(super) name: String,
    /// The form.
    pub(super) value: &'t LValue,
    /// The `params` key when the form is the whole value of that key (not nested in a table or
    /// an array).
    pub(super) whole_of: Option<&'t str>,
}

/// The parameter named by a `{ param = .. }` form (the one classifier is
/// `cgf_core::reserved_form`; only one-entry tables are converted).
pub(super) fn param_name(value: &LValue) -> Option<String> {
    if as_table(value)?.iter().count() != 1 {
        return None;
    }
    match reserved_form(&value.to_param())? {
        ReservedForm::Param(name) => Some(name.to_string()),
        ReservedForm::Key(_) => None,
    }
}

/// Every `{ param }` form below the root of `params`, in key order (the root is never a form).
pub(super) fn param_refs(params: &LTable) -> Vec<ParamRef<'_>> {
    let mut out = Vec::new();
    for (key, entry) in params.iter() {
        collect(entry.value(), Some(key), &mut out);
    }
    out
}

fn collect<'t>(value: &'t LValue, whole_of: Option<&'t str>, out: &mut Vec<ParamRef<'t>>) {
    if let Some(name) = param_name(value) {
        out.push(ParamRef {
            name,
            value,
            whole_of,
        });
        return;
    }
    match value.kind() {
        LKind::Table(table) => {
            for (_, entry) in table.iter() {
                collect(entry.value(), None, out);
            }
        }
        LKind::Array(items) => {
            for item in items {
                collect(item, None, out);
            }
        }
        _ => {}
    }
}

/// A reserved one-key form found in a `params` table.
pub(super) struct Form<'t> {
    /// The top-level `params` key that holds it.
    pub(super) top: &'t str,
    /// The form.
    pub(super) value: &'t LValue,
    /// `{ key = .. }` (true) or `{ param = .. }` (false).
    pub(super) is_key: bool,
}

/// Every `{ key = .. }` and `{ param = .. }` form below the root of `params`, in key order (the
/// root is never a form).
pub(super) fn forms(params: &LTable) -> Vec<Form<'_>> {
    let mut out = Vec::new();
    for (key, entry) in params.iter() {
        collect_forms(entry.value(), key, &mut out);
    }
    out
}

fn collect_forms<'t>(value: &'t LValue, top: &'t str, out: &mut Vec<Form<'t>>) {
    if as_table(value).is_some_and(|t| t.iter().count() == 1)
        && let Some(form) = reserved_form(&value.to_param())
    {
        let is_key = matches!(form, ReservedForm::Key(_));
        out.push(Form { top, value, is_key });
        return;
    }
    match value.kind() {
        LKind::Table(table) => {
            for (_, entry) in table.iter() {
                collect_forms(entry.value(), top, out);
            }
        }
        LKind::Array(items) => {
            for item in items {
                collect_forms(item, top, out);
            }
        }
        _ => {}
    }
}

/// Where a substituted value came from.
pub(super) struct Source {
    /// The supplied value.
    pub(super) value: Origin,
    /// The `{ param }` reference it replaced.
    pub(super) reference: Origin,
    /// The parameter.
    pub(super) name: String,
}

/// A `params` table with its references replaced.
pub(super) struct Substituted {
    /// The table; keys whose value has an unknown reference are left out.
    pub(super) table: LTable,
    /// The top-level keys that were left out.
    pub(super) dropped: Vec<String>,
    /// The values that were substituted.
    pub(super) sources: Vec<Source>,
}

/// Replaces every `{ param }` form of `params` by its argument. A top-level key with a reference
/// whose value is unknown is dropped (the value cannot be checked at load time).
pub(super) fn substitute(params: &LTable, args: &Args) -> Substituted {
    let mut entries = BTreeMap::new();
    let mut dropped = Vec::new();
    let mut sources = Vec::new();
    for (key, entry) in params.iter() {
        match replace(entry.value(), args, &mut sources) {
            Some(value) => {
                entries.insert(
                    key.to_string(),
                    LEntry::synthetic(entry.key_origin(), value),
                );
            }
            None => dropped.push(key.to_string()),
        }
    }
    Substituted {
        table: LTable::from_entries(params.origin().clone(), entries),
        dropped,
        sources,
    }
}

fn replace(value: &LValue, args: &Args, sources: &mut Vec<Source>) -> Option<LValue> {
    if let Some(name) = param_name(value) {
        let arg = args.get(&name)?.as_ref()?;
        sources.push(Source {
            value: arg.origin().clone(),
            reference: value.origin().clone(),
            name,
        });
        return Some(arg.clone());
    }
    let kind = match value.kind() {
        LKind::Table(table) => {
            let mut entries = BTreeMap::new();
            for (key, entry) in table.iter() {
                let inner = replace(entry.value(), args, sources)?;
                entries.insert(
                    key.to_string(),
                    LEntry::synthetic(entry.key_origin(), inner),
                );
            }
            LKind::Table(LTable::from_entries(table.origin().clone(), entries))
        }
        LKind::Array(items) => LKind::Array(
            items
                .iter()
                .map(|item| replace(item, args, sources))
                .collect::<Option<Vec<_>>>()?,
        ),
        other => other.clone(),
    };
    Some(LValue::synthetic(value.origin(), kind))
}

/// The source that substituted the value a diagnostic is located in, if any.
fn hit<'s>(diagnostic: &Diagnostic, sources: &'s [Source]) -> Option<&'s Source> {
    let at = diagnostic.location.as_ref()?;
    sources.iter().find(|s| {
        let span = s.value.span();
        s.value.path() == at.path && span.start <= at.span.start && at.span.end <= span.end
    })
}

fn noted(mut diagnostic: Diagnostic, source: &Source) -> Diagnostic {
    diagnostic.notes.push(Note {
        message: format!("the value of parameter `{}` is used here", source.name),
        location: Some(source.reference.location()),
    });
    diagnostic
}

/// Adds to every diagnostic located inside a substituted value a note at the reference that
/// supplied the value.
pub(super) fn annotate(found: Vec<Diagnostic>, sources: &[Source]) -> Vec<Diagnostic> {
    found
        .into_iter()
        .map(|d| match hit(&d, sources) {
            Some(source) => noted(d, source),
            None => d,
        })
        .collect()
}

/// Like [`annotate`], but keeps only the diagnostics located inside a value substituted for a
/// reference to `param`: the rest belong to the graph's own literals and defaults, which are
/// checked where they are written (ASM-106 d).
pub(super) fn annotate_only(
    found: Vec<Diagnostic>,
    sources: &[Source],
    param: &str,
) -> Vec<Diagnostic> {
    found
        .into_iter()
        .filter_map(|d| {
            let source = hit(&d, sources).filter(|s| s.name == param)?;
            Some(noted(d, source))
        })
        .collect()
}

/// Whether `value` holds a `{ param = <name> }` reference anywhere.
pub(super) fn mentions(value: &LValue, name: &str) -> bool {
    if let Some(found) = param_name(value) {
        return found == name;
    }
    match value.kind() {
        LKind::Table(table) => table.iter().any(|(_, e)| mentions(e.value(), name)),
        LKind::Array(items) => items.iter().any(|i| mentions(i, name)),
        _ => false,
    }
}
