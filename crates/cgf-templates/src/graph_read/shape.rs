//! Whole-graph shape checks on one located file (ADR 0012, ASM-075, REQ-B-12): the graph is a tree
//! rooted at `root`, nested at most [`MAX_NODE_DEPTH`] levels; FSM states are reachable; a guard
//! holds only conditions and condition combinators.
//!
//! The checks read the file's tables, not the `GraphDef`, so they also run on a file whose other
//! nodes have errors (an unknown name is skipped here: the reader has reported it, ASM-056).

use std::collections::{BTreeMap, BTreeSet};

use cgf_core::MAX_NODE_DEPTH;

use super::nodes::in_file_order;
use super::{as_table, shown};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::value::{LKind, LTable, LValue, Origin};

/// The node types a guard subtree may contain (ADR 0012).
const GUARD_TYPES: [&str; 4] = ["condition", "inverter", "sequence", "selector"];

/// A written reference to a node that exists.
struct Edge<'t> {
    name: &'t str,
    at: &'t Origin,
}

struct Node<'t> {
    ty: &'t str,
    key: &'t Origin,
    table: &'t LTable,
    /// Children, FSM states, then FSM guards, in the order written.
    edges: Vec<Edge<'t>>,
    /// The FSM guards (a subset of `edges`).
    guards: Vec<Edge<'t>>,
}

type Nodes<'t> = BTreeMap<&'t str, Node<'t>>;

/// Every shape error of the graph file `file`: `node_reused`, `node_cycle`, `node_too_deep`,
/// `unreachable_node`, `unreachable_state` and `invalid_guard`. Unsorted.
pub(crate) fn check(file: &LTable) -> Vec<Diagnostic> {
    let Some(table) = file.get("nodes").and_then(as_table) else {
        return Vec::new();
    };
    let nodes = collect(table);
    let mut out = Vec::new();
    let root = file
        .get("root")
        .and_then(string)
        .filter(|name| nodes.contains_key(name));
    if let Some(root) = root {
        let reached = walk(root, &nodes, &mut out);
        for (name, node) in &nodes {
            if !reached.contains(name) {
                let message = format!(
                    "node `{}` is not reachable from root `{}`",
                    shown(name),
                    shown(root)
                );
                out.push(at(DiagnosticCode::UnreachableNode, message, node.key));
            }
        }
    }
    for (name, node) in &nodes {
        if node.ty == "fsm" {
            guards(name, node, &nodes, &mut out);
            unreachable_states(name, node.table, &mut out);
        }
    }
    out
}

fn string(value: &LValue) -> Option<&str> {
    match value.kind() {
        LKind::String(text) => Some(text),
        _ => None,
    }
}

fn at(code: DiagnosticCode, message: String, origin: &Origin) -> Diagnostic {
    Diagnostic::new(code, message, Some(origin.location()))
}

/// The node tables with their references to other nodes that exist.
fn collect(table: &LTable) -> Nodes<'_> {
    let names: BTreeSet<&str> = table.iter().map(|(name, _)| name).collect();
    let mut nodes = BTreeMap::new();
    for (name, entry) in table.iter() {
        let Some(node_table) = as_table(entry.value()) else {
            continue;
        };
        let ty = node_table.get("type").and_then(string).unwrap_or("");
        let (mut edges, guards) = references(ty, node_table, &names);
        edges.extend(guards.iter().map(|g| Edge {
            name: g.name,
            at: g.at,
        }));
        nodes.insert(
            name,
            Node {
                ty,
                key: entry.key_origin(),
                table: node_table,
                edges,
                guards,
            },
        );
    }
    nodes
}

/// The nodes `node` refers to: (children or states, guards).
fn references<'t>(
    ty: &str,
    node: &'t LTable,
    names: &BTreeSet<&str>,
) -> (Vec<Edge<'t>>, Vec<Edge<'t>>) {
    let known = |value: &'t LValue| {
        let name = string(value)?;
        names.contains(name).then(|| Edge {
            name,
            at: value.origin(),
        })
    };
    match ty {
        "sequence" | "selector" | "parallel" => {
            let items = match node.get("children").map(LValue::kind) {
                Some(LKind::Array(items)) => items.as_slice(),
                _ => &[],
            };
            (items.iter().filter_map(known).collect(), Vec::new())
        }
        "inverter" | "succeed_always" | "repeat" | "cooldown" | "timeout" => (
            node.get("child").and_then(known).into_iter().collect(),
            Vec::new(),
        ),
        "fsm" => {
            let states = node
                .get("states")
                .and_then(as_table)
                .map(|t| {
                    in_file_order(t)
                        .into_iter()
                        .filter_map(|(_, entry)| known(entry.value()))
                        .collect()
                })
                .unwrap_or_default();
            let guards = transitions(node)
                .iter()
                .filter_map(|t| t.get("when").and_then(known))
                .collect();
            (states, guards)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

/// The transition tables of an `fsm` node.
fn transitions(node: &LTable) -> Vec<&LTable> {
    match node.get("transitions").map(LValue::kind) {
        Some(LKind::Array(items)) => items.iter().filter_map(as_table).collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Active,
    Done,
}

/// Depth-first from `root`, reporting a reference to an ancestor (`node_cycle`), to a node already
/// visited (`node_reused`) and the reference that makes the nesting deeper than the limit
/// (`node_too_deep`). Returns the nodes reached. Iterative: the file decides the depth.
fn walk<'t>(root: &'t str, nodes: &Nodes<'t>, out: &mut Vec<Diagnostic>) -> BTreeSet<&'t str> {
    let mut marks: BTreeMap<&str, Mark> = BTreeMap::new();
    let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
    marks.insert(root, Mark::Active);
    while let Some(&(name, next)) = stack.last() {
        let edge = nodes.get(name).and_then(|n| n.edges.get(next));
        let Some(edge) = edge else {
            marks.insert(name, Mark::Done);
            stack.pop();
            continue;
        };
        if let Some(top) = stack.last_mut() {
            top.1 += 1;
        }
        match marks.get(edge.name) {
            Some(Mark::Active) => {
                let message = format!("node `{}` is its own ancestor", shown(edge.name));
                out.push(at(DiagnosticCode::NodeCycle, message, edge.at));
            }
            Some(Mark::Done) => {
                let message = format!(
                    "node `{}` has more than one parent (a graph is a tree)",
                    shown(edge.name)
                );
                out.push(at(DiagnosticCode::NodeReused, message, edge.at));
            }
            None => {
                if stack.len() == MAX_NODE_DEPTH {
                    let message = format!(
                        "nodes are nested deeper than {MAX_NODE_DEPTH} levels at `{}`",
                        shown(edge.name)
                    );
                    out.push(at(DiagnosticCode::NodeTooDeep, message, edge.at));
                }
                marks.insert(edge.name, Mark::Active);
                stack.push((edge.name, 0));
            }
        }
    }
    marks.into_keys().collect()
}

/// A guard (a `when` of `fsm` node `fsm`) must be a tree of conditions, inverters, sequences and
/// selectors; the first other node is reported at the guard reference.
fn guards(fsm: &str, node: &Node<'_>, nodes: &Nodes<'_>, out: &mut Vec<Diagnostic>) {
    for guard in &node.guards {
        let mut seen = BTreeSet::new();
        let mut todo = vec![guard.name];
        while let Some(name) = todo.pop() {
            let Some(inner) = nodes.get(name) else {
                continue;
            };
            if !seen.insert(name) {
                continue;
            }
            if !GUARD_TYPES.contains(&inner.ty) {
                let message = format!(
                    "guard `{}` of fsm `{}` holds node `{}` of type `{}`; a guard may contain only {}",
                    shown(guard.name),
                    shown(fsm),
                    shown(name),
                    shown(inner.ty),
                    GUARD_TYPES.join(", ")
                );
                out.push(at(DiagnosticCode::InvalidGuard, message, guard.at));
                break;
            }
            todo.extend(inner.edges.iter().rev().map(|e| e.name));
        }
    }
}

/// A state that no chain of transitions leads to from `initial` is `unreachable_state`, at the
/// state's key. No `initial` or an unknown one is the reader's `no_initial_state`.
fn unreachable_states(fsm: &str, node: &LTable, out: &mut Vec<Diagnostic>) {
    let Some(states) = node.get("states").and_then(as_table) else {
        return;
    };
    let Some(initial) = node
        .get("initial")
        .and_then(string)
        .filter(|name| states.get(name).is_some())
    else {
        return;
    };
    let mut reached = BTreeSet::from([initial]);
    let mut todo = vec![initial];
    let moves = transitions(node);
    while let Some(from) = todo.pop() {
        for transition in &moves {
            let step = (
                transition.get("from").and_then(string),
                transition.get("to").and_then(string),
            );
            if let (Some(f), Some(to)) = step
                && f == from
                && states.get(to).is_some()
                && reached.insert(to)
            {
                todo.push(to);
            }
        }
    }
    for (name, entry) in in_file_order(states) {
        if !reached.contains(name) {
            let message = format!(
                "state `{}` of fsm `{}` cannot be reached from the initial state `{}`",
                shown(name),
                shown(fsm),
                shown(initial)
            );
            out.push(at(
                DiagnosticCode::UnreachableState,
                message,
                entry.key_origin(),
            ));
        }
    }
}
