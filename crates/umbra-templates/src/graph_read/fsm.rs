//! The `fsm` node (ADR 0012): states, exits and transitions. State and exit names resolve to
//! indices here; an unknown `initial` is `no_initial_state` and a name used for both a state and
//! an exit is `duplicate_name`. Reachability and guard-kind checks are WI-13.

use umbra_core::{
    ExitDef, FsmDef, NodeId, StateDef, Status, TransitionDef, TransitionTarget, Trigger,
};

use super::nodes::{Cx, in_file_order};
use super::{check_name, shown};
use crate::diagnostic::DiagnosticCode;
use crate::value::{LTable, LValue};

/// Reads the settings of an `fsm` node. States and exits whose value is bad are still listed (with a
/// placeholder), so later lookups of their names do not cascade into more errors; the file has an
/// error then, so the placeholder never reaches a graph.
pub(super) fn read(cx: &mut Cx<'_, '_>, table: &LTable) -> Option<FsmDef> {
    cx.known(table, &["initial", "states", "exits", "transitions"]);
    let (states, states_ok) = states(cx, table);
    let (exits, exits_ok) = table
        .get("exits")
        .map_or_else(|| (Vec::new(), true), |v| exits(cx, v));
    clashes(cx, &states, &exits);
    let initial = initial(cx, table, &states);
    let transitions = table
        .get("transitions")
        .map_or_else(|| Some(Vec::new()), |v| transitions(cx, v, &states, &exits));
    if !(states_ok && exits_ok) {
        return None;
    }
    let mut fsm = FsmDef::new(initial?, states.iter().map(|s| s.def.clone()).collect());
    for exit in &exits {
        fsm = fsm.with_exit(exit.def.clone());
    }
    for transition in transitions? {
        fsm = fsm.with_transition(transition);
    }
    Some(fsm)
}

/// A state or exit with where its name is written.
struct Named<T> {
    def: T,
    at: crate::value::Origin,
}

/// The states in file order and whether all of them are good. A missing `states` table is
/// reported and yields no states.
fn states(cx: &mut Cx<'_, '_>, table: &LTable) -> (Vec<Named<StateDef>>, bool) {
    let Some(map) = cx
        .require(table, "states")
        .and_then(|v| cx.reader.table("states", v))
    else {
        return (Vec::new(), false);
    };
    let mut out = Vec::new();
    let mut ok = true;
    for (name, entry) in in_file_order(map) {
        ok &= check_name(cx.reader, name, "state name", entry.key_origin());
        let node = cx.node_ref(entry.value(), "states");
        ok &= node.is_some();
        out.push(Named {
            def: StateDef::new(name, node.unwrap_or(NodeId(0))),
            at: entry.key_origin().clone(),
        });
    }
    (out, ok)
}

/// The exits in file order and whether all of them are good.
fn exits(cx: &mut Cx<'_, '_>, value: &LValue) -> (Vec<Named<ExitDef>>, bool) {
    let Some(map) = cx.reader.table("exits", value) else {
        return (Vec::new(), false);
    };
    let mut out = Vec::new();
    let mut ok = true;
    for (name, entry) in in_file_order(map) {
        ok &= check_name(cx.reader, name, "exit name", entry.key_origin());
        let result = status(cx, "exits", entry.value());
        ok &= result.is_some();
        out.push(Named {
            def: ExitDef::new(name, result.unwrap_or(Status::Failure)),
            at: entry.key_origin().clone(),
        });
    }
    (out, ok)
}

/// `"success"` or `"failure"` (never `running`).
fn status(cx: &mut Cx<'_, '_>, setting: &str, value: &LValue) -> Option<Status> {
    match cx.reader.string(setting, value)? {
        "success" => Some(Status::Success),
        "failure" => Some(Status::Failure),
        other => {
            let message = format!(
                "`{setting}` must be \"success\" or \"failure\", found `{}`",
                shown(other)
            );
            cx.reader
                .error(DiagnosticCode::InvalidValue, message, value.origin());
            None
        }
    }
}

/// States and exits share one namespace (ASM-072): an exit named like a state is `duplicate_name`
/// at the exit.
fn clashes(cx: &mut Cx<'_, '_>, states: &[Named<StateDef>], exits: &[Named<ExitDef>]) {
    for exit in exits {
        if states.iter().any(|s| s.def.name == exit.def.name) {
            let message = format!(
                "`{}` is both a state and an exit of this fsm",
                shown(&exit.def.name)
            );
            cx.reader
                .error(DiagnosticCode::DuplicateName, message, &exit.at);
        }
    }
}

/// `initial`: the name of a state; absent or unknown is `no_initial_state`.
fn initial(cx: &mut Cx<'_, '_>, table: &LTable, states: &[Named<StateDef>]) -> Option<usize> {
    let Some(value) = table.get("initial") else {
        let message = "fsm is missing required key `initial` (the starting state)".to_string();
        cx.reader
            .error(DiagnosticCode::NoInitialState, message, table.origin());
        return None;
    };
    let name = cx.reader.string("initial", value)?;
    let found = states.iter().position(|s| s.def.name == name);
    if found.is_none() {
        let message = format!(
            "`initial` names state `{}`, which `states` does not define",
            shown(name)
        );
        cx.reader
            .error(DiagnosticCode::NoInitialState, message, value.origin());
    }
    found
}

fn transitions(
    cx: &mut Cx<'_, '_>,
    value: &LValue,
    states: &[Named<StateDef>],
    exits: &[Named<ExitDef>],
) -> Option<Vec<TransitionDef>> {
    let items = cx.reader.array("transitions", value)?;
    let all: Vec<Option<TransitionDef>> = items
        .iter()
        .map(|item| transition(cx, item, states, exits))
        .collect();
    all.into_iter().collect()
}

fn transition(
    cx: &mut Cx<'_, '_>,
    value: &LValue,
    states: &[Named<StateDef>],
    exits: &[Named<ExitDef>],
) -> Option<TransitionDef> {
    let table = cx.reader.table("transitions", value)?;
    cx.reader.unknown_keys(table, &["from", "to", "when", "on"]);
    let from = table
        .get("from")
        .or_else(|| missing(cx, table, "from"))
        .and_then(|v| state_index(cx, "from", v, states));
    let to = table
        .get("to")
        .or_else(|| missing(cx, table, "to"))
        .and_then(|v| target(cx, v, states, exits));
    let trigger = trigger(cx, table);
    Some(TransitionDef::new(from?, to?, trigger?))
}

/// Reports a missing transition key (located at the transition) and yields nothing.
fn missing<'t>(cx: &mut Cx<'_, '_>, table: &'t LTable, key: &str) -> Option<&'t LValue> {
    let message = format!("transition is missing required key `{key}`");
    cx.reader
        .error(DiagnosticCode::MissingKey, message, table.origin());
    None
}

fn state_index(
    cx: &mut Cx<'_, '_>,
    setting: &str,
    value: &LValue,
    states: &[Named<StateDef>],
) -> Option<usize> {
    let name = cx.reader.string(setting, value)?;
    let found = states.iter().position(|s| s.def.name == name);
    if found.is_none() {
        let message = format!(
            "`{setting}` names state `{}`, which `states` does not define",
            shown(name)
        );
        cx.reader
            .error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    found
}

fn target(
    cx: &mut Cx<'_, '_>,
    value: &LValue,
    states: &[Named<StateDef>],
    exits: &[Named<ExitDef>],
) -> Option<TransitionTarget> {
    let name = cx.reader.string("to", value)?;
    let found = states
        .iter()
        .position(|s| s.def.name == name)
        .map(TransitionTarget::State)
        .or_else(|| {
            exits
                .iter()
                .position(|e| e.def.name == name)
                .map(TransitionTarget::Exit)
        });
    if found.is_none() {
        let message = format!(
            "`to` names `{}`, which is neither a state nor an exit of this fsm",
            shown(name)
        );
        cx.reader
            .error(DiagnosticCode::InvalidValue, message, value.origin());
    }
    found
}

/// Exactly one of `when` (a guard node) or `on` (`success` or `failure`).
fn trigger(cx: &mut Cx<'_, '_>, table: &LTable) -> Option<Trigger> {
    match (table.get("when"), table.get("on")) {
        (Some(when), None) => cx.node_ref(when, "when").map(Trigger::When),
        (None, Some(on)) => status(cx, "on", on).map(Trigger::On),
        (None, None) | (Some(_), Some(_)) => {
            let message = "a transition needs exactly one of `when` or `on`".to_string();
            cx.reader
                .error(DiagnosticCode::InvalidValue, message, table.origin());
            None
        }
    }
}
