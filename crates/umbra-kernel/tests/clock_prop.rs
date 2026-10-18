//! AT-7 (property): clock monotonicity under random operation sequences.
//! Requirements: REQ-K-3, REQ-K-8, REQ-C-1, D-5, section 15 (clock monotonicity).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;

use proptest::prelude::*;
use umbra_core::TickRate;
use umbra_kernel::{Clock, ClockMode, ManualWallClock, Pace, TimeScale};

#[derive(Debug, Clone)]
enum Op {
    Poll,
    Pause,
    Resume,
    SetMode(u8),
    RequestSteps(u8),
    WallAdvance(u16),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => Just(Op::Poll),
        1 => Just(Op::Pause),
        1 => Just(Op::Resume),
        1 => (0_u8..5).prop_map(Op::SetMode),
        2 => (0_u8..6).prop_map(Op::RequestSteps),
        3 => (0_u16..400).prop_map(Op::WallAdvance),
    ]
}

fn mode(i: u8) -> ClockMode {
    let scale = |s| TimeScale::new(s).expect("valid scale");
    match i {
        0 => ClockMode::RealTime,
        1 => ClockMode::Scaled(scale(4.0)),
        2 => ClockMode::Scaled(scale(0.5)),
        3 => ClockMode::AsFastAsPossible,
        _ => ClockMode::Stepped,
    }
}

proptest! {
    /// Under any sequence of operations: a tick adds exactly one to the tick count, time never
    /// decreases (ticks or seconds), nothing ticks while paused, `poll` does not advance, a wait is
    /// always for a future deadline, and stepped mode never runs more ticks than were requested.
    #[test]
    fn clock_monotonicity(ops in prop::collection::vec(op(), 1..200), start_mode in 0_u8..5) {
        let wall = ManualWallClock::new();
        let rate = TickRate::new(10).expect("non-zero rate");
        let mut clock = Clock::new(rate, mode(start_mode), Box::new(wall.clone()));
        let mut requested_total: u64 = 0;
        let mut stepped_ticks: u64 = 0;
        let mut last_tick = 0_u64;
        let mut last_seconds = 0.0_f64;

        for op in ops {
            match op {
                Op::Pause => clock.pause(),
                Op::Resume => clock.resume(),
                Op::SetMode(i) => clock.set_mode(mode(i)),
                Op::RequestSteps(n) => {
                    clock.request_steps(u64::from(n));
                    requested_total += u64::from(n);
                }
                Op::WallAdvance(ms) => wall.advance(Duration::from_millis(u64::from(ms))),
                Op::Poll => {
                    let before = clock.now();
                    let pace = clock.poll();
                    prop_assert_eq!(clock.now(), before, "poll must not advance time");
                    if clock.is_paused() {
                        prop_assert_eq!(pace, Pace::Idle, "no tick while paused");
                    }
                    match pace {
                        Pace::Tick => {
                            let was_stepped = clock.mode() == ClockMode::Stepped;
                            let after = clock.advance().expect("advance below u64::MAX");
                            prop_assert_eq!(after.tick(), before.tick() + 1);
                            if was_stepped {
                                stepped_ticks += 1;
                            }
                        }
                        Pace::WaitUntil(deadline) => {
                            prop_assert!(deadline > clock.wall_now(), "waits are for the future");
                        }
                        Pace::Idle => {}
                    }
                }
            }

            let now = clock.now();
            prop_assert!(now.tick() >= last_tick, "tick decreased");
            prop_assert!(now.as_seconds().0 >= last_seconds, "seconds decreased");
            prop_assert!(stepped_ticks <= requested_total, "stepped ran more than requested");
            last_tick = now.tick();
            last_seconds = now.as_seconds().0;
        }
    }
}
