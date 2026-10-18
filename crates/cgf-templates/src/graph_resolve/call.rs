//! `subgraph` call sites (REQ-B-9, REQ-B-10, REQ-B-12, ADR 0014, ASM-076, ASM-085, ASM-088): the
//! target resolves, its version fits the range, params and input bindings fit its interface, and
//! output bindings fit its ports.

use std::collections::{BTreeMap, BTreeSet};

use cgf_core::{
    BbType, ComponentId, ComponentInfo, ComponentKind, LeafRef, NodeKind, ParamSchema, SubgraphRef,
    Version,
};

use super::bind::{Args, annotate, param_refs, substitute};
use super::view::{View, check_refs};
use super::{Resolved, Set, leaf};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::graph_read::as_table;
use crate::schema::CompiledSchema;
use crate::value::{LTable, LValue, Origin};

/// What a call site needs to know about its target.
struct Callee<'a> {
    id: &'a ComponentId,
    version: &'a Version,
    schema: &'a ParamSchema,
    /// Every param with its type, binding-only ones included.
    types: &'a BTreeMap<String, BbType>,
    outputs: BTreeMap<String, BbType>,
    /// Params that must be supplied.
    required: Vec<String>,
    /// The target's file, when it is a graph of this load.
    graph: Option<View<'a>>,
}

/// The most graphs a supplied value is followed through (the loader's depth limit, capped so the
/// recursion stays small whatever the option says).
const MAX_CHAIN: usize = 256;

/// The (graph, supplied values) pairs already followed (ASM-106 d).
pub(super) type Seen = BTreeSet<String>;

/// Per graph: the parent keys output bindings write, to find a second writer (ASM-088).
pub(super) type Written = BTreeMap<String, Origin>;

/// Checks the `subgraph` node `table` (`call`) of the graph `view`.
pub(super) fn check(
    set: &Set<'_>,
    view: &View<'_>,
    call: &SubgraphRef,
    table: &LTable,
    written: &mut Written,
    seen: &mut Seen,
    out: &mut Vec<Diagnostic>,
) {
    let id_at = table
        .get("ref")
        .map_or_else(|| table.origin(), |v| v.origin());
    let callee = match set.resolve(&call.id) {
        Resolved::Graph(target) => graph_callee(&call.id, target),
        Resolved::Registered(info) => registered_callee(set, &call.id, info),
        Resolved::Unknown => {
            out.push(set.unknown_behavior(&call.id, id_at));
            return;
        }
        Resolved::Silent => return,
    };
    let Some(callee) = callee else { return };
    version(&callee, call, table, out);
    let params = View::params_of(table);
    inputs(&callee, view, call, &params, out);
    let supplied = parameters(
        set,
        &callee,
        (view, &view.defaults()),
        (call, table),
        true,
        out,
    );
    outputs(&callee, view, call, table, written, out);
    if let Some(target) = &callee.graph {
        let limit = set.depth_limit().min(MAX_CHAIN);
        propagate(
            set,
            (target, &call.id),
            &supplied,
            &mut Chain::new(limit, seen),
            out,
        );
    }
}

fn graph_callee<'a>(id: &'a ComponentId, target: View<'a>) -> Option<Callee<'a>> {
    let graph = target.read.graph.as_ref()?;
    let interface = &graph.interface;
    Some(Callee {
        id,
        version: &graph.version,
        schema: &interface.params,
        types: &interface.types,
        outputs: interface
            .outputs
            .iter()
            .map(|(port, port_def)| (port.clone(), port_def.ty))
            .collect(),
        required: interface
            .types
            .keys()
            .filter(|p| interface.is_required(p))
            .cloned()
            .collect(),
        graph: Some(target),
    })
}

fn registered_callee<'a>(
    set: &'a Set<'_>,
    id: &'a ComponentId,
    info: &'a ComponentInfo,
) -> Option<Callee<'a>> {
    let schema = set
        .loader
        .registry
        .param_schema(ComponentKind::Behavior, id)?;
    let mut required: Vec<String> = schema
        .as_json()
        .get("required")
        .and_then(|r| r.as_array())
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_string))
        .collect();
    required.extend(
        info.param_types
            .iter()
            .filter(|(_, ty)| matches!(ty, BbType::Entity | BbType::Position))
            .map(|(name, _)| name.clone()),
    );
    required.sort();
    required.dedup();
    Some(Callee {
        id,
        version: &info.version,
        schema,
        types: &info.param_types,
        outputs: info.outputs.clone(),
        required,
        graph: None,
    })
}

fn error(code: DiagnosticCode, message: String, at: &Origin) -> Diagnostic {
    Diagnostic::new(code, message, Some(at.location()))
}

/// `version_mismatch` at the call's `version` value, with a note at the target's `version`.
fn version(callee: &Callee<'_>, call: &SubgraphRef, table: &LTable, out: &mut Vec<Diagnostic>) {
    let Some(req) = &call.version else { return };
    if req.matches(callee.version) {
        return;
    }
    let at = table
        .get("version")
        .map_or_else(|| table.origin(), |v| v.origin());
    let message = format!(
        "`{}` is version {}, which does not satisfy `{req}`",
        callee.id, callee.version
    );
    let diagnostic = error(DiagnosticCode::VersionMismatch, message, at);
    let defined = callee
        .graph
        .and_then(|g| g.parts.table.get("version"))
        .map(|v| v.origin().location());
    let note = match defined {
        Some(location) => (format!("`{}` is defined here", callee.id), Some(location)),
        None => (format!("`{}` is a registered behavior", callee.id), None),
    };
    out.push(diagnostic.with_note(note.0, note.1));
}

/// `{ key = .. }` input bindings: the param exists, the parent key is declared with the same type
/// (an `integer` key may feed a `number` param), and the target does not need the param at build
/// time (ADR 0014).
fn inputs(
    callee: &Callee<'_>,
    view: &View<'_>,
    call: &SubgraphRef,
    params: &LTable,
    out: &mut Vec<Diagnostic>,
) {
    for (param, key) in &call.inputs {
        let Some(at) = params.get(param).map(LValue::origin) else {
            continue;
        };
        let Some(&want) = callee.types.get(param) else {
            let message = format!("`{}` has no parameter `{param}` to bind", callee.id);
            let mut found = error(DiagnosticCode::InvalidBinding, message, at);
            if callee.graph.is_none() {
                found = found.with_note(format!("`{param}` is not declared bindable"), None);
            }
            out.push(found);
            continue;
        };
        let Some((have, _)) = view.key(key) else {
            if view.parts.blackboard.is_some() {
                let message = format!("`{key}` is not declared in this graph's blackboard");
                out.push(error(DiagnosticCode::InvalidBinding, message, at));
            }
            continue;
        };
        if have != want && !(have == BbType::Integer && want == BbType::Number) {
            let message = format!(
                "key `{key}` is {have}, but parameter `{param}` of `{}` is {want}",
                callee.id
            );
            out.push(error(DiagnosticCode::InvalidBinding, message, at));
            continue;
        }
        if let Some(reference) = callee.graph.and_then(|g| used_as_reference(&g, param)) {
            let message = format!(
                "parameter `{param}` of `{}` is needed at build time and cannot be bound at run time",
                callee.id
            );
            let found = error(DiagnosticCode::InvalidBinding, message, at)
                .with_note("used as `{ param }` here", Some(reference.location()));
            out.push(found);
        }
    }
}

/// The first `{ param = <param> }` reference in the nodes of `target`.
fn used_as_reference(target: &View<'_>, param: &str) -> Option<Origin> {
    target.parts.nodes.iter().find_map(|(name, _)| {
        let table = target.node_table(name)?;
        let params = table.get("params").and_then(as_table)?;
        param_refs(params)
            .into_iter()
            .find(|r| r.name == param)
            .map(|r| r.value.origin().clone())
    })
}

/// Literal and referenced values against the target's interface; missing params. `known` are the
/// values of the calling graph's params. Returns the values this call supplies, by param (`None`:
/// bound at run time or not known now). With `report`, bad `{ param }` references and missing
/// params are reported too; when following supplied values down a chain they were reported
/// already.
fn parameters(
    set: &Set<'_>,
    callee: &Callee<'_>,
    (view, known): (&View<'_>, &Args),
    (call, table): (&SubgraphRef, &LTable),
    report: bool,
    out: &mut Vec<Diagnostic>,
) -> Args {
    let params = View::without(&View::params_of(table), call.inputs.keys());
    let bad = if report {
        check_refs(view, Some(callee.schema), &params, out)
    } else {
        check_refs(view, Some(callee.schema), &params, &mut Vec::new())
    };
    let mut known = known.clone();
    for name in &bad {
        known.insert(name.clone(), None);
    }
    let substituted = substitute(&params, &known);
    let at = View::params_origin(table);
    for name in callee.required.iter().filter(|_| report) {
        if params.get(name).is_none() && !call.inputs.contains_key(name) {
            let message = format!(
                "behavior `{}`: missing required parameter `{name}`",
                callee.id
            );
            out.push(error(DiagnosticCode::MissingParam, message, &at));
        }
    }
    let registered = callee.graph.is_none();
    let mut found = if registered && substituted.dropped.is_empty() {
        let mut found = Vec::new();
        let id_at = table.get("ref").map_or(&at, |v| v.origin());
        let _ = set.loader.check_component(
            ComponentKind::Behavior,
            callee.id,
            id_at,
            &substituted.table,
            &mut found,
        );
        found
    } else {
        schema_errors(callee, &substituted.table)
    };
    found.retain(|d| d.code != DiagnosticCode::MissingParam);
    out.extend(annotate(found, &substituted.sources));
    let mut supplied = Args::new();
    for name in call.inputs.keys() {
        supplied.insert(name.clone(), None);
    }
    for (name, _) in params.iter() {
        supplied.insert(name.to_string(), substituted.table.get(name).cloned());
    }
    supplied
}

fn schema_errors(callee: &Callee<'_>, params: &LTable) -> Vec<Diagnostic> {
    let context = format!("{} `{}`", ComponentKind::Behavior.noun(), callee.id);
    CompiledSchema::compile(callee.schema).map_or_else(
        |_| Vec::new(),
        |schema| schema.validate(&params.clone().into_value(), &context),
    )
}

/// Output bindings `outputs = { <port> = "<parent key>" }`: the port exists, the parent key is a
/// declared, writable key of the port's type, and no other binding writes it (ADR 0014, ASM-088).
fn outputs(
    callee: &Callee<'_>,
    view: &View<'_>,
    call: &SubgraphRef,
    table: &LTable,
    written: &mut Written,
    out: &mut Vec<Diagnostic>,
) {
    let Some(bindings) = table.get("outputs").and_then(as_table) else {
        return;
    };
    for (port, parent) in &call.outputs {
        let Some(entry) = bindings.entry(port) else {
            continue;
        };
        let at = entry.value().origin();
        let Some(&want) = callee.outputs.get(port) else {
            let message = format!("`{}` has no output port `{port}`", callee.id);
            out.push(error(
                DiagnosticCode::InvalidBinding,
                message,
                entry.key_origin(),
            ));
            continue;
        };
        let problem = match view.key(parent) {
            None if view.parts.blackboard.is_none() => continue,
            None => Some(format!(
                "`{parent}` is not declared in this graph's blackboard"
            )),
            Some((_, true)) => Some(format!(
                "`{parent}` is an interface parameter and read-only"
            )),
            Some((have, false)) if have != want => Some(format!(
                "key `{parent}` is {have}, but output `{port}` of `{}` is {want}",
                callee.id
            )),
            Some(_) => None,
        };
        if let Some(problem) = problem {
            out.push(error(DiagnosticCode::InvalidBinding, problem, at));
        } else if let Some(first) = written.get(parent) {
            let message = format!("key `{parent}` is already written by another output binding");
            let found = error(DiagnosticCode::InvalidBinding, message, at)
                .with_note("first written here", Some(first.location()));
            out.push(found);
        } else {
            written.insert(parent.clone(), at.clone());
        }
    }
}

/// The chain being followed: the graphs from the first callee on, the most graphs, and the pairs
/// already followed.
struct Chain<'s> {
    path: Vec<ComponentId>,
    limit: usize,
    seen: &'s mut Seen,
}

impl<'s> Chain<'s> {
    fn new(limit: usize, seen: &'s mut Seen) -> Self {
        Self {
            path: Vec::new(),
            limit,
            seen,
        }
    }
}

/// Follows the values a call supplies down the chain (REQ-B-10, ASM-076, ASM-106 d): the leaves of
/// `target` that use a supplied param are checked again with the values, and so are the params of
/// its own `subgraph` nodes against their callees' interfaces, which in turn pass values on. Errors
/// show at the supplying value, with a note at the `{ param }` reference and, below the first
/// callee, the chain of graphs. A graph already on the chain is not entered again (cycles are
/// reported elsewhere), and a (graph, values) pair is followed once.
fn propagate(
    set: &Set<'_>,
    (target, id): (&View<'_>, &ComponentId),
    supplied: &Args,
    chain: &mut Chain<'_>,
    out: &mut Vec<Diagnostic>,
) {
    if chain.path.contains(id) || chain.path.len() >= chain.limit {
        return;
    }
    let mut args = target.defaults();
    args.extend(supplied.iter().map(|(k, v)| (k.clone(), v.clone())));
    if !chain.seen.insert(format!("{id}|{args:?}")) {
        return;
    }
    chain.path.push(id.clone());
    let mut found = Vec::new();
    let mut leaves = Vec::new();
    let mut deeper = Vec::new();
    for (name, def) in &target.parts.nodes {
        let (Some(def), Some(table)) = (def, target.node_table(name)) else {
            continue;
        };
        match &def.kind {
            NodeKind::Condition(l) => {
                leaf_again(
                    set,
                    (target, ComponentKind::Condition),
                    l,
                    table,
                    (&args, supplied),
                    &mut leaves,
                );
            }
            NodeKind::Action(l) => {
                leaf_again(
                    set,
                    (target, ComponentKind::Action),
                    l,
                    table,
                    (&args, supplied),
                    &mut leaves,
                );
            }
            NodeKind::Subgraph(call) => {
                let callee = match set.resolve(&call.id) {
                    Resolved::Graph(next) => graph_callee(&call.id, next),
                    Resolved::Registered(info) => registered_callee(set, &call.id, info),
                    Resolved::Unknown | Resolved::Silent => None,
                };
                let Some(callee) = callee else { continue };
                let mut checked = Vec::new();
                let passed = parameters(
                    set,
                    &callee,
                    (target, &args),
                    (call, table),
                    false,
                    &mut checked,
                );
                let mut via = chain.path.clone();
                via.push(call.id.clone());
                found.extend(through(checked, &via));
                if let Some(next) = &callee.graph {
                    propagate(set, (next, &call.id), &passed, chain, &mut deeper);
                }
            }
            _ => {}
        }
    }
    if chain.path.len() > 1 {
        leaves = through(leaves, &chain.path);
    }
    chain.path.pop();
    found.extend(leaves);
    out.extend(found);
    out.extend(deeper);
}

/// Adds to every diagnostic a note naming the chain of graphs the value travelled.
fn through(found: Vec<Diagnostic>, path: &[ComponentId]) -> Vec<Diagnostic> {
    let names: Vec<String> = path.iter().map(|p| format!("`{p}`")).collect();
    let note = format!("reached through {}", names.join(" -> "));
    found
        .into_iter()
        .map(|d| d.with_note(note.clone(), None))
        .collect()
}

/// A leaf of `target` that uses a param the call supplies is checked again with the values.
/// References already reported as bad are not substituted.
fn leaf_again(
    set: &Set<'_>,
    (target, kind): (&View<'_>, ComponentKind),
    leaf_ref: &LeafRef,
    table: &LTable,
    (args, supplied): (&Args, &Args),
    out: &mut Vec<Diagnostic>,
) {
    if !set
        .loader
        .schemas
        .contains_key(&(kind, leaf_ref.id.clone()))
    {
        return;
    }
    let params = View::params_of(table);
    let touched = param_refs(&params)
        .iter()
        .any(|r| matches!(supplied.get(&r.name), Some(Some(_))));
    if touched {
        let schema = set.loader.registry.param_schema(kind, &leaf_ref.id);
        let bad = check_refs(target, schema, &params, &mut Vec::new());
        leaf::values(set, kind, leaf_ref, table, args, &bad, out);
    }
}
