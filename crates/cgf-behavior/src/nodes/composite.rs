//! `sequence`, `selector` and `parallel` (ADR 0013).

use crate::instance::{NodeState, Slot, Walk};
use crate::program::Policy;
use cgf_core::Status;

/// `sequence` (`decisive` = Failure) and `selector` (`decisive` = Success).
///
/// Children are ticked in array order until one returns Running or `decisive`; that status is the
/// result. If none does, the result is `exhausted`. Reactive composites start at the first child
/// every tick; with `memory` they resume at the child that was Running. A child that was Running
/// last tick and is not reached this tick (it comes after the deciding child) is halted.
pub(crate) fn ordered(
    w: &mut Walk<'_, '_, '_, '_>,
    idx: usize,
    children: &[usize],
    memory: bool,
    decisive: Status,
    exhausted: Status,
) -> Status {
    let previous = match &w.states[idx] {
        NodeState::Composite { running } => *running,
        _ => None,
    };
    let start = if memory { previous.unwrap_or(0) } else { 0 };
    let mut result = exhausted;
    let mut stopped_at = None;
    for (pos, &child) in children.iter().enumerate().skip(start) {
        let status = w.tick(child);
        if status == Status::Running || status == decisive {
            result = status;
            stopped_at = Some(pos);
            break;
        }
    }
    if let (Some(prev), Some(pos)) = (previous, stopped_at)
        && prev > pos
        && let Some(&skipped) = children.get(prev)
    {
        w.halt(skipped);
    }
    let running = (result == Status::Running).then_some(stopped_at).flatten();
    w.states[idx] = NodeState::Composite { running };
    result
}

/// `parallel`: every unfinished child is ticked in order each tick. `All` fails as soon as one
/// child fails and succeeds when all succeeded; `Any` mirrors it. The deciding result halts the
/// children that are still Running. Finished results are kept until the parallel finishes.
pub(crate) fn parallel(
    w: &mut Walk<'_, '_, '_, '_>,
    idx: usize,
    children: &[usize],
    policy: Policy,
) -> Status {
    let (decisive, exhausted) = match policy {
        Policy::All => (Status::Failure, Status::Success),
        Policy::Any => (Status::Success, Status::Failure),
    };
    for (pos, &child) in children.iter().enumerate() {
        if matches!(slot(w, idx, pos), Slot::Done(_)) {
            continue;
        }
        let status = w.tick(child);
        if status == decisive {
            halt_running(w, idx, children);
            reset(w, idx);
            return status;
        }
        set_slot(
            w,
            idx,
            pos,
            if status == Status::Running {
                Slot::Running
            } else {
                Slot::Done(status)
            },
        );
    }
    if (0..children.len()).all(|pos| matches!(slot(w, idx, pos), Slot::Done(_))) {
        reset(w, idx);
        return exhausted;
    }
    Status::Running
}

fn slot(w: &Walk<'_, '_, '_, '_>, idx: usize, pos: usize) -> Slot {
    match &w.states[idx] {
        NodeState::Parallel { slots } => slots.get(pos).copied().unwrap_or(Slot::Idle),
        _ => Slot::Idle,
    }
}

fn set_slot(w: &mut Walk<'_, '_, '_, '_>, idx: usize, pos: usize, value: Slot) {
    if let NodeState::Parallel { slots } = &mut w.states[idx]
        && let Some(s) = slots.get_mut(pos)
    {
        *s = value;
    }
}

fn reset(w: &mut Walk<'_, '_, '_, '_>, idx: usize) {
    if let NodeState::Parallel { slots } = &mut w.states[idx] {
        slots.fill(Slot::Idle);
    }
}

fn halt_running(w: &mut Walk<'_, '_, '_, '_>, idx: usize, children: &[usize]) {
    for (pos, &child) in children.iter().enumerate() {
        if slot(w, idx, pos) == Slot::Running {
            w.halt(child);
        }
    }
}
