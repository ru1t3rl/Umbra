//! `Clock::pending_steps` (M5 plan, review m2; ADR 0024): the steps requested and not yet run.
//! Requirements: REQ-K-3.

#![forbid(unsafe_code)]

mod common;

use umbra_kernel::{ClockMode, ManualWallClock, Pace};

#[test]
fn pending_steps_counts_requests_and_drops_with_each_advance() {
    let mut k = common::kernel_with(1, ClockMode::Stepped, ManualWallClock::new());
    assert_eq!(k.clock().pending_steps(), 0);

    k.clock_mut().request_steps(3);
    k.clock_mut().request_steps(2);
    assert_eq!(k.clock().pending_steps(), 5);

    assert_eq!(k.clock_mut().poll(), Pace::Tick);
    k.tick().expect("tick");
    assert_eq!(k.clock().pending_steps(), 4);
    k.tick().expect("tick");
    assert_eq!(k.clock().pending_steps(), 3);
    k.tick().expect("tick");
    assert_eq!(k.clock().pending_steps(), 2);

    k.clock_mut().cancel_steps();
    assert_eq!(k.clock().pending_steps(), 0);
}

/// Leaving stepped mode drops the pending steps, and they do not come back on return.
#[test]
fn leaving_stepped_mode_clears_the_pending_steps() {
    let mut k = common::kernel_with(1, ClockMode::Stepped, ManualWallClock::new());
    k.clock_mut().request_steps(4);

    k.clock_mut().set_mode(ClockMode::AsFastAsPossible);
    assert_eq!(k.clock().pending_steps(), 0);
    k.clock_mut().set_mode(ClockMode::Stepped);

    assert_eq!(k.clock().pending_steps(), 0);
    assert_eq!(k.clock_mut().poll(), Pace::Idle);
}

/// Only stepped mode consumes steps: ticks in another mode leave the count alone.
#[test]
fn ticks_outside_stepped_mode_do_not_consume_steps() {
    let mut k = common::kernel_with(1, ClockMode::AsFastAsPossible, ManualWallClock::new());
    k.clock_mut().request_steps(3);

    k.tick().expect("tick");
    k.tick().expect("tick");

    assert_eq!(k.clock().pending_steps(), 3);
}
