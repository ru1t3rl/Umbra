//! `inverter`, `succeed_always`, `repeat`, `cooldown` and `timeout` (ADR 0013).

use crate::instance::{NodeState, Walk};
use cgf_core::Status;

/// Swaps Success and Failure; Running passes.
pub(crate) fn inverter(w: &mut Walk<'_, '_, '_, '_>, child: usize) -> Status {
    match w.tick(child) {
        Status::Success => Status::Failure,
        Status::Failure => Status::Success,
        Status::Running => Status::Running,
    }
}

/// Maps Failure to Success; Success and Running pass.
pub(crate) fn succeed_always(w: &mut Walk<'_, '_, '_, '_>, child: usize) -> Status {
    match w.tick(child) {
        Status::Running => Status::Running,
        _ => Status::Success,
    }
}

/// Ticks the child once per tick. A child Success counts; the node is Running until `count`
/// successes (forever without `count`), then Success. A child Failure is Failure and restarts
/// the count. A Running child keeps the node Running.
pub(crate) fn repeat(
    w: &mut Walk<'_, '_, '_, '_>,
    idx: usize,
    child: usize,
    count: Option<u32>,
) -> Status {
    let status = w.tick(child);
    let NodeState::Repeat { successes, running } = &mut w.states[idx] else {
        return Status::Failure;
    };
    let result = match status {
        Status::Running => Status::Running,
        Status::Failure => {
            *successes = 0;
            Status::Failure
        }
        Status::Success => {
            *successes = successes.saturating_add(1);
            if count.is_some_and(|n| *successes >= n) {
                *successes = 0;
                Status::Success
            } else {
                Status::Running
            }
        }
    };
    *running = result == Status::Running;
    result
}

/// After the child finishes, fails without ticking it until the duration (whole ticks) has
/// elapsed: a child that finished at tick `t` runs again at `t + n`.
pub(crate) fn cooldown(w: &mut Walk<'_, '_, '_, '_>, idx: usize, child: usize) -> Status {
    let now = w.ctx.now.tick();
    let NodeState::Cooldown { ticks, until } = &mut w.states[idx] else {
        return Status::Failure;
    };
    if until.is_some_and(|u| now < u) {
        return Status::Failure;
    }
    *until = None;
    let ticks = *ticks;
    let status = w.tick(child);
    if status != Status::Running
        && let NodeState::Cooldown { until, .. } = &mut w.states[idx]
    {
        *until = Some(now.saturating_add(ticks));
    }
    status
}

/// Halts the child and fails when it is still Running `n` ticks after it started: a child that
/// started at tick `s` is ticked at `s .. s + n - 1` and halted at `s + n`.
pub(crate) fn timeout(w: &mut Walk<'_, '_, '_, '_>, idx: usize, child: usize) -> Status {
    let now = w.ctx.now.tick();
    let NodeState::Timeout { ticks, started } = &mut w.states[idx] else {
        return Status::Failure;
    };
    if started.is_some_and(|s| now.saturating_sub(s) >= *ticks) {
        *started = None;
        w.halt(child);
        return Status::Failure;
    }
    started.get_or_insert(now);
    let status = w.tick(child);
    if status != Status::Running
        && let NodeState::Timeout { started, .. } = &mut w.states[idx]
    {
        *started = None;
    }
    status
}
