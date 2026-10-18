//! The `fsm` node (ADR 0013, ASM-072, ASM-097).
//!
//! Per tick: (1) the first `when` transition of the current state, in file order, whose guard
//! returns Success fires; (2) the current state's node is ticked once; (3) if it returned Success
//! or Failure, the first matching `on` transition fires. An exit returns its mapped status and
//! resets the machine to `initial`; every other tick returns Running.
//!
//! Guards are ordinary nodes and are ticked like any other, so a guard may write the blackboard
//! (a `store_as` condition). Guards are evaluated in file order and stop at the
//! first Success, so a later guard's side effects happen only if the earlier ones failed; the
//! writes are visible to the state node ticked in the same tick.

use crate::instance::{NodeState, Walk};
use crate::program::{Fsm, To, When};
use umbra_core::Status;

/// The state of an FSM that has not started (or was reset): in `initial`, not Running.
pub(crate) fn fresh(f: &Fsm) -> NodeState {
    NodeState::Fsm {
        current: f.initial,
        running: false,
    }
}

/// Every child node: the state nodes, then the guards, in file order. `program.rs` `children_of`
/// lists an FSM's children the same way; keep them in step.
pub(crate) fn children(f: &Fsm) -> impl Iterator<Item = usize> + '_ {
    let guards = f.transitions.iter().filter_map(|t| match t.when {
        When::Guard(g) => Some(g),
        When::Done(_) => None,
    });
    f.states.iter().copied().chain(guards)
}

/// Ticks the FSM at `idx` once.
pub(crate) fn tick(w: &mut Walk<'_, '_, '_, '_>, idx: usize, f: &Fsm) -> Status {
    let mut current = match &w.states[idx] {
        NodeState::Fsm { current, .. } => *current,
        _ => return Status::Failure,
    };
    if let Some(to) = fire_guard(w, f, current) {
        // A parent halt: a state node that is not Running is left as it is (ASM-095).
        if let Some(&node) = f.states.get(current) {
            w.halt(node);
        }
        match to {
            To::Exit(e) => return exit(w, idx, f, e),
            To::State(s) => current = s,
        }
    }
    let Some(&node) = f.states.get(current) else {
        return Status::Failure;
    };
    set(w, idx, current);
    let status = w.tick(node);
    if status != Status::Running
        && let Some(to) = f
            .transitions
            .iter()
            .find(|t| t.from == current && matches!(t.when, When::Done(done) if done == status))
    {
        match to.to {
            To::Exit(e) => return exit(w, idx, f, e),
            To::State(s) => set(w, idx, s),
        }
    }
    Status::Running
}

/// The target of the first guard transition of `current` whose guard succeeds.
fn fire_guard(w: &mut Walk<'_, '_, '_, '_>, f: &Fsm, current: usize) -> Option<To> {
    for t in f.transitions.iter().filter(|t| t.from == current) {
        if let When::Guard(g) = t.when
            && w.tick(g) == Status::Success
        {
            return Some(t.to);
        }
    }
    None
}

fn set(w: &mut Walk<'_, '_, '_, '_>, idx: usize, state: usize) {
    w.states[idx] = NodeState::Fsm {
        current: state,
        running: true,
    };
}

/// Enters exit `e`: the machine returns to `initial` and the exit's status is the result.
fn exit(w: &mut Walk<'_, '_, '_, '_>, idx: usize, f: &Fsm, e: usize) -> Status {
    w.states[idx] = fresh(f);
    f.exits.get(e).copied().unwrap_or(Status::Failure)
}
