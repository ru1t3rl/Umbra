//! The checks of one graph file that need no other file: its shape, the interface against the
//! blackboard, and every node (REQ-B-12, ADR 0014, ASM-056, ASM-101).

use cgf_core::{ComponentKind, NodeKind};

use super::call::{self, Seen, Written};
use super::view::View;
use super::{Set, leaf};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::graph_read::{as_table, shape};
use crate::value::{LValue, Origin};

/// Every error of the file `view`, also when the file has other errors: what is checked needs only
/// the parts that did read (a node with an error is skipped).
pub(super) fn check(set: &Set<'_>, view: &View<'_>, seen: &mut Seen, out: &mut Vec<Diagnostic>) {
    out.extend(shape::check(&view.parts.table));
    blackboard_clash(view, out);
    outputs_from(view, out);
    let mut written = Written::new();
    for (name, def) in &view.parts.nodes {
        let (Some(def), Some(table)) = (def, view.node_table(name)) else {
            continue;
        };
        match &def.kind {
            NodeKind::Condition(l) => {
                leaf::check(set, view, ComponentKind::Condition, l, table, out);
            }
            NodeKind::Action(l) => leaf::check(set, view, ComponentKind::Action, l, table, out),
            NodeKind::Subgraph(c) => call::check(set, view, c, table, &mut written, seen, out),
            _ => {}
        }
    }
}

fn error(code: DiagnosticCode, message: String, at: &Origin) -> Diagnostic {
    Diagnostic::new(code, message, Some(at.location()))
}

/// A `[blackboard]` key that repeats an interface param is `duplicate_name`: every param is
/// already a read-only key of the same name (ADR 0014, ASM-071).
fn blackboard_clash(view: &View<'_>, out: &mut Vec<Diagnostic>) {
    let Some(interface) = &view.parts.interface else {
        return;
    };
    let Some(table) = view.parts.table.get("blackboard").and_then(as_table) else {
        return;
    };
    for (key, entry) in table.iter() {
        if interface.types.contains_key(key) {
            let message = format!(
                "blackboard key `{key}` repeats an interface parameter; every parameter is already a read-only key"
            );
            out.push(error(
                DiagnosticCode::DuplicateName,
                message,
                entry.key_origin(),
            ));
        }
    }
}

/// An output port reads a declared key of the port's type (`outputs.from`, ADR 0014).
fn outputs_from(view: &View<'_>, out: &mut Vec<Diagnostic>) {
    let Some(interface) = &view.parts.interface else {
        return;
    };
    let ports = view
        .parts
        .table
        .get("interface")
        .and_then(as_table)
        .and_then(|t| t.get("outputs"))
        .and_then(as_table);
    for (port, def) in &interface.outputs {
        let at = ports
            .and_then(|t| t.get(port))
            .and_then(as_table)
            .and_then(|t| t.get("from"))
            .map(LValue::origin);
        let Some(at) = at else { continue };
        let problem = match view.key(&def.from) {
            None if view.parts.blackboard.is_none() => continue,
            None => format!(
                "output port `{port}` reads key `{}`, which this graph does not declare",
                def.from
            ),
            Some((ty, _)) if ty != def.ty => format!(
                "output port `{port}` is {}, but key `{}` is {ty}",
                def.ty, def.from
            ),
            Some(_) => continue,
        };
        out.push(error(DiagnosticCode::InvalidBinding, problem, at));
    }
}
