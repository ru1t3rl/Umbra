//! `condition` and `action` nodes: registry lookup, `{ param }` references, parameter schema,
//! trial build and the blackboard keys the leaf names (REQ-T-1, REQ-T-3, ADR 0014, ASM-076).

use std::collections::BTreeSet;

use cgf_core::{Access, ComponentKind, LeafRef};

use super::Set;
use super::bind::{Args, annotate, substitute};
use super::view::{View, check_refs};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::value::{LKind, LTable};

/// Checks one leaf node of the graph `view` on its own: the parameters are checked with the
/// defaults of the graph's interface (a reference to a param without default is not checked here;
/// the call sites supply it, `call.rs`).
pub(super) fn check(
    set: &Set<'_>,
    view: &View<'_>,
    kind: ComponentKind,
    leaf: &LeafRef,
    table: &LTable,
    out: &mut Vec<Diagnostic>,
) {
    let id_at = table
        .get("ref")
        .map_or_else(|| table.origin(), |v| v.origin());
    if !set.loader.check_registered(kind, &leaf.id, id_at, out) {
        return;
    }
    let params = View::params_of(table);
    let schema = set.loader.registry.param_schema(kind, &leaf.id);
    let bad = check_refs(view, schema, &params, out);
    blackboard_keys(set, view, leaf, &params, out);
    values(set, kind, leaf, table, &view.defaults(), &bad, out);
}

/// Validates the parameters of a leaf with `{ param }` references replaced by `args` (schema and
/// trial build, errors at the supplying value with a note at the reference). Names in `bad` are
/// treated as unknown. A leaf with a reference of unknown value is not checked. The leaf must be
/// registered.
pub(super) fn values(
    set: &Set<'_>,
    kind: ComponentKind,
    leaf: &LeafRef,
    table: &LTable,
    args: &Args,
    bad: &BTreeSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let id_at = table
        .get("ref")
        .map_or_else(|| table.origin(), |v| v.origin());
    let mut args = args.clone();
    for name in bad {
        args.insert(name.clone(), None);
    }
    let substituted = substitute(&View::params_of(table), &args);
    if !substituted.dropped.is_empty() {
        return;
    }
    let mut found = Vec::new();
    let _ = set
        .loader
        .check_component(kind, &leaf.id, id_at, &substituted.table, &mut found);
    out.extend(annotate(found, &substituted.sources));
}

/// A parameter that names a blackboard key (`target_key`, `store_as`) must name a declared key (or
/// an interface param, which is read-only) of the type the leaf needs (ADR 0014).
fn blackboard_keys(
    set: &Set<'_>,
    view: &View<'_>,
    leaf: &LeafRef,
    params: &LTable,
    out: &mut Vec<Diagnostic>,
) {
    let Some(info) = set.loader.registry.info(&leaf.id) else {
        return;
    };
    for named in &info.blackboard_params {
        let Some(value) = params.get(&named.param) else {
            continue;
        };
        let LKind::String(key) = value.kind() else {
            continue;
        };
        let problem = match view.key(key) {
            None if view.parts.blackboard.is_none() => continue,
            None => format!(
                "`{}` names blackboard key `{key}`, which this graph does not declare",
                named.param
            ),
            Some((ty, _)) if ty != named.ty => format!(
                "`{}` names blackboard key `{key}` of type {ty}, but `{}` needs {}",
                named.param, leaf.id, named.ty
            ),
            Some((_, true)) if named.access == Access::Write => format!(
                "`{}` writes `{key}`, which is an interface parameter and read-only",
                named.param
            ),
            Some(_) => continue,
        };
        out.push(Diagnostic::new(
            DiagnosticCode::InvalidBinding,
            problem,
            Some(value.origin().location()),
        ));
    }
}
