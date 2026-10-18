//! The `subgraph` node (ADR 0014, ASM-088, ASM-096).
//!
//! Per tick: (1) if a required bound input is unset, halt a Running child as a parent halt and
//! return Failure without ticking it; (2) copy every bound input from the parent's blackboard into
//! the child (`set_input`; unset copies as unset); (3) tick the child once; (4) copy every bound
//! output into the parent's blackboard (`output`; unset copies as unset). The node is Running iff
//! the child returned Running.
//!
//! Allocation: none. The copies move `BbValue`s by value through the object-safe `set_input` and
//! `output` calls, and every `BbValue` is `Copy`-sized or an `Arc<str>`, so a copy is a cheap clone.
//! A refused copy (not expected after validation) halts a Running child and returns Failure.

use crate::instance::{NodeState, Walk};
use crate::program::{Input, Subgraph};
use cgf_core::{BbValue, BehaviorInstance, Status};

/// Ticks the `subgraph` node at `idx` once.
pub(crate) fn tick(w: &mut Walk<'_, '_, '_, '_>, idx: usize, sub: &Subgraph) -> Status {
    let NodeState::Subgraph { child, running } = &mut w.states[idx] else {
        return Status::Failure;
    };
    if sub
        .inputs
        .iter()
        .any(|i| i.required && w.bb.get(&i.key).is_none())
    {
        // A parent halt, not an instance halt (ASM-096 b): the child keeps finished state.
        if std::mem::take(running) {
            child.halt_running();
        }
        return Status::Failure;
    }
    for input in &sub.inputs {
        let value = w.bb.get(&input.key).map(|v| convert(input, v));
        if child.set_input(&input.param, value).is_err() {
            return refuse(child, running);
        }
    }
    let status = child.tick(&mut *w.ctx, w.entity, &mut *w.out);
    *running = status == Status::Running;
    for (port, key) in &sub.outputs {
        let written = match child.output(port) {
            Some(v) => w.bb.set(key, v),
            None => w.bb.unset(key),
        };
        if written.is_err() {
            return refuse(child, running);
        }
    }
    status
}

/// A copy the blackboard refused: the child is halted as a parent halt if it was Running, so
/// `is_running` stays exact, and the node fails.
fn refuse(child: &mut Box<dyn BehaviorInstance>, running: &mut bool) -> Status {
    if std::mem::take(running) {
        child.halt_running();
    }
    Status::Failure
}

/// The value as the child's param type wants it: `integer` to `number` is converted (ASM-088).
fn convert(input: &Input, v: &BbValue) -> BbValue {
    match v {
        #[expect(
            clippy::cast_precision_loss,
            reason = "documented loss above 2^53, ASM-088"
        )]
        BbValue::Integer(i) if input.to_number => BbValue::Number(*i as f64),
        other => other.clone(),
    }
}
