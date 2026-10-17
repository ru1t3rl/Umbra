//! AT-7: the simulation clock. Requirements: REQ-K-3, REQ-K-8, REQ-C-1, D-5, ASM-035.
//!
//! All pacing tests run on a `ManualWallClock`: no sleeping, no real time (the single exception is
//! `std_wall_clock_is_monotonic`). Pacing arithmetic uses 10 Hz so that periods are exact.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;

use cgf_core::{SimTime, TickRate};
use cgf_kernel::{
    Clock, ClockMode, KernelError, ManualWallClock, Pace, StdWallClock, TimeScale, WallClock,
};

const HZ: u32 = 10;
const PERIOD: Duration = Duration::from_millis(100);

fn rate() -> TickRate {
    TickRate::new(HZ).expect("non-zero rate")
}

fn scale(s: f64) -> TimeScale {
    TimeScale::new(s).expect("valid scale")
}

/// A clock on a shared manual wall clock (clones share time).
fn clock(mode: ClockMode) -> (Clock, ManualWallClock) {
    let wall = ManualWallClock::new();
    (Clock::new(rate(), mode, Box::new(wall.clone())), wall)
}

/// Polls once, expecting `Tick`, then advances.
fn tick(c: &mut Clock) {
    assert_eq!(c.poll(), Pace::Tick);
    c.advance().expect("advance");
}

/// Counts consecutive `Tick` polls (advancing after each) until another pace is returned.
fn drain_ticks(c: &mut Clock, limit: u32) -> (u32, Pace) {
    let mut n = 0;
    loop {
        let p = c.poll();
        if p != Pace::Tick || n >= limit {
            return (n, p);
        }
        c.advance().expect("advance");
        n += 1;
    }
}

fn near(a: Duration, b: Duration) -> bool {
    a.abs_diff(b) <= Duration::from_micros(1)
}

/// REQ-K-3: as fast as possible always ticks, and each advance adds exactly one tick.
#[test]
fn afap_always_ticks() {
    let (mut c, _wall) = clock(ClockMode::AsFastAsPossible);

    for _ in 0..1000 {
        tick(&mut c);
    }

    assert_eq!(c.now().tick(), 1000);
}

/// REQ-K-3: `poll` never advances time.
#[test]
fn poll_never_advances_time() {
    let (mut c, _wall) = clock(ClockMode::AsFastAsPossible);

    for _ in 0..5 {
        let _ = c.poll();
    }

    assert_eq!(c.now(), SimTime::start(rate()));
}

/// REQ-C-1: `advance` returns the new time, one tick later, and the clock reports it.
#[test]
fn advance_returns_the_next_tick() {
    let (mut c, _wall) = clock(ClockMode::AsFastAsPossible);

    let t = c.advance().expect("advance");

    assert_eq!(t, SimTime::at_tick(rate(), 1));
    assert_eq!(c.now(), t);
}

/// REQ-K-3: pause stops ticking in every mode (even with pending steps and elapsed wall time);
/// resume restores the mode's behavior.
#[test]
fn paused_never_ticks_in_any_mode() {
    for mode in [
        ClockMode::RealTime,
        ClockMode::Scaled(scale(2.0)),
        ClockMode::AsFastAsPossible,
        ClockMode::Stepped,
    ] {
        let (mut c, wall) = clock(mode);
        c.request_steps(5);
        c.pause();
        wall.advance(Duration::from_secs(10));

        assert!(c.is_paused(), "{mode:?}");
        for _ in 0..3 {
            assert_eq!(c.poll(), Pace::Idle, "{mode:?}");
        }

        c.resume();
        assert!(!c.is_paused(), "{mode:?}");
        assert_eq!(c.poll(), Pace::Tick, "{mode:?}");
    }
}

/// REQ-K-3: stepped mode runs exactly the requested ticks, then idles.
#[test]
fn stepped_runs_exactly_requested_ticks_then_idles() {
    let (mut c, _wall) = clock(ClockMode::Stepped);
    assert_eq!(c.poll(), Pace::Idle, "nothing requested yet");

    c.request_steps(3);
    let (ticks, then) = drain_ticks(&mut c, 100);

    assert_eq!((ticks, then), (3, Pace::Idle));
    assert_eq!(c.now().tick(), 3);
}

/// REQ-K-3: step requests accumulate.
#[test]
fn stepped_requests_accumulate() {
    let (mut c, _wall) = clock(ClockMode::Stepped);

    c.request_steps(2);
    c.request_steps(2);
    let (ticks, then) = drain_ticks(&mut c, 100);

    assert_eq!((ticks, then), (4, Pace::Idle));
}

/// REQ-K-3: in real time the next tick waits for its deadline `anchor + k * dt`.
#[test]
fn realtime_waits_until_deadline() {
    let (mut c, wall) = clock(ClockMode::RealTime);

    assert_eq!(c.poll(), Pace::Tick, "tick 0 is due at the anchor");
    c.advance().expect("advance");

    let Pace::WaitUntil(deadline) = c.poll() else {
        panic!("tick 1 must wait");
    };
    assert!(near(deadline, PERIOD), "deadline {deadline:?}");
    wall.advance(Duration::from_millis(99));
    assert!(matches!(c.poll(), Pace::WaitUntil(_)), "still early");
    wall.advance(Duration::from_millis(2));
    assert_eq!(c.poll(), Pace::Tick, "deadline passed");
}

/// REQ-K-3: `wait_until` on the clock waits on the wall clock (here: jumps the manual clock).
#[test]
fn wait_until_moves_the_wall_clock_to_the_deadline() {
    let (c, _wall) = clock(ClockMode::RealTime);

    c.wait_until(Duration::from_secs(3));

    assert!(c.wall_now() >= Duration::from_secs(3));
}

/// REQ-K-3: a scaled clock at 4x runs four ticks per real-time period.
#[test]
fn scaled_4x_ticks_four_times_per_period() {
    let (mut c, wall) = clock(ClockMode::Scaled(scale(4.0)));
    tick(&mut c);

    wall.advance(PERIOD);
    let (ticks, then) = drain_ticks(&mut c, 100);

    assert_eq!(ticks, 4);
    let Pace::WaitUntil(next) = then else {
        panic!("expected a wait, got {then:?}");
    };
    assert!(near(next, Duration::from_millis(125)), "{next:?}");
}

/// REQ-K-3: resuming after a long pause does not burst; the schedule is re-anchored.
#[test]
fn resume_reanchors_without_burst() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    tick(&mut c);
    wall.advance(PERIOD);
    tick(&mut c);
    c.pause();
    wall.advance(Duration::from_secs(10));

    c.resume();
    let (ticks, then) = drain_ticks(&mut c, 1000);

    assert_eq!(ticks, 1, "exactly one tick is due at the new anchor");
    let Pace::WaitUntil(next) = then else {
        panic!("expected a wait, got {then:?}");
    };
    assert!(near(next, Duration::from_millis(10_200)), "{next:?}");
}

/// REQ-K-3: switching mode re-anchors too (AFAP ticks do not count as owed real-time ticks).
#[test]
fn set_mode_reanchors_without_burst() {
    let (mut c, _wall) = clock(ClockMode::AsFastAsPossible);
    for _ in 0..50 {
        tick(&mut c);
    }

    c.set_mode(ClockMode::RealTime);
    let (ticks, then) = drain_ticks(&mut c, 1000);

    assert_eq!(ticks, 1);
    let Pace::WaitUntil(next) = then else {
        panic!("expected a wait, got {then:?}");
    };
    assert!(near(next, PERIOD), "{next:?}");
    assert_eq!(c.mode(), ClockMode::RealTime);
}

/// REQ-K-8: when behind schedule the clock catches up tick by tick (never skipping) and reports
/// the lag; once caught up the lag is gone.
#[test]
fn behind_schedule_catches_up_without_skipping_and_reports_behind() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    tick(&mut c);
    assert_eq!(c.behind(), None, "on schedule");

    wall.advance(Duration::from_millis(1050));
    assert!(
        c.behind().is_some(),
        "tick 1 was due at 100 ms, wall is at 1050 ms"
    );

    let (ticks, then) = drain_ticks(&mut c, 1000);

    assert_eq!(ticks, 10, "ticks 1..=10 are due by 1050 ms");
    assert_eq!(c.now().tick(), 11, "every tick ran; none skipped");
    assert!(matches!(then, Pace::WaitUntil(_)));
    assert_eq!(c.behind(), None, "caught up");
}

/// REQ-K-8: lag is measured against the deadline of the tick just run. Tick 0 was due at the
/// anchor, so 150 ms later (one 100 ms period plus 50 ms) the clock is behind.
#[test]
fn tick_finished_more_than_one_period_late_is_behind() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    tick(&mut c);

    wall.advance(Duration::from_millis(150));

    assert_eq!(c.behind(), Some(Duration::from_millis(150)));
}

/// REQ-K-8: the boundary is one period: P + 1 ms is behind, P - 1 ms and P are not.
#[test]
fn behind_boundary_is_one_period() {
    for (ms, behind) in [(101, true), (100, false), (99, false)] {
        let (mut c, wall) = clock(ClockMode::RealTime);
        tick(&mut c);

        wall.advance(Duration::from_millis(ms));

        assert_eq!(c.behind().is_some(), behind, "{ms} ms");
    }
}

/// REQ-K-8: the same boundary on a later tick of a schedule (tick 1 was due at 100 ms).
#[test]
fn behind_boundary_holds_on_later_ticks() {
    for (ms, behind) in [(201, true), (199, false)] {
        let (mut c, wall) = clock(ClockMode::RealTime);
        tick(&mut c);
        wall.advance(PERIOD);
        tick(&mut c);

        wall.advance(Duration::from_millis(ms - 100));

        assert_eq!(c.behind().is_some(), behind, "wall at {ms} ms");
    }
}

/// REQ-K-8: scaled clocks measure against the scaled period (25 ms at 4x): a 30 ms lag is
/// reported, a 20 ms lag is not.
#[test]
fn scaled_clock_reports_lag_beyond_the_scaled_period() {
    for (ms, behind) in [(30, true), (20, false)] {
        let (mut c, wall) = clock(ClockMode::Scaled(scale(4.0)));
        tick(&mut c);

        wall.advance(Duration::from_millis(ms));

        assert_eq!(c.behind().is_some(), behind, "{ms} ms");
    }
}

/// REQ-K-8: `behind` also works when the driver advanced without polling first (`advance`
/// anchors the schedule if needed).
#[test]
fn behind_works_without_a_prior_poll() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    assert!(c.advance().is_ok());

    wall.advance(Duration::from_millis(150));

    assert!(c.behind().is_some());
}

/// REQ-K-3: switching from real time to a scaled mode after a long wall gap re-anchors: exactly
/// one tick is due, then the next wait is one scaled period away.
#[test]
fn switching_to_scaled_after_a_gap_does_not_burst() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    for _ in 0..3 {
        wall.advance(PERIOD);
        tick(&mut c);
    }
    wall.advance(Duration::from_secs(10));

    c.set_mode(ClockMode::Scaled(scale(4.0)));
    let (ticks, then) = drain_ticks(&mut c, 1000);

    assert_eq!(ticks, 1);
    let Pace::WaitUntil(next) = then else {
        panic!("expected a wait, got {then:?}");
    };
    assert!(near(next, Duration::from_millis(10_300 + 25)), "{next:?}");
}

/// REQ-K-3: leaving stepped mode drops pending steps, so they cannot fire after a later return;
/// setting the current mode again keeps them.
#[test]
fn set_mode_away_from_stepped_clears_pending_steps() {
    let (mut c, _wall) = clock(ClockMode::Stepped);
    c.request_steps(5);
    c.set_mode(ClockMode::Stepped);
    assert_eq!(c.poll(), Pace::Tick, "same mode keeps the steps");

    c.set_mode(ClockMode::AsFastAsPossible);
    c.set_mode(ClockMode::Stepped);

    assert_eq!(c.poll(), Pace::Idle);
}

/// REQ-K-3: setting the current mode again does not re-anchor the schedule.
#[test]
fn set_mode_to_the_current_mode_keeps_the_schedule() {
    let (mut c, wall) = clock(ClockMode::RealTime);
    tick(&mut c);
    wall.advance(Duration::from_millis(500));

    c.set_mode(ClockMode::RealTime);

    assert!(c.behind().is_some(), "still 400 ms late");
}

/// REQ-K-3: a time scale is limited to [1e-3, 1e6].
#[test]
fn time_scale_is_limited_to_a_sane_range() {
    for bad in [0.0009, 1e-4, 1_000_001.0, 1e7] {
        assert!(TimeScale::new(bad).is_err(), "{bad}");
    }
    for ok in [1e-3, 1.0, 1e6] {
        assert!(TimeScale::new(ok).is_ok(), "{ok}");
    }
}

/// ASM-035: a read returns the time before the auto-advance step is applied.
#[test]
fn manual_wall_clock_auto_advance_reads_then_advances() {
    let wall = ManualWallClock::with_auto_advance(Duration::from_millis(10));

    let first = wall.now();
    let second = wall.now();

    assert_eq!(first, Duration::ZERO);
    assert_eq!(second, Duration::from_millis(10));
}

/// REQ-K-8: modes without a wall schedule are never behind.
#[test]
fn afap_and_stepped_are_never_behind() {
    for mode in [ClockMode::AsFastAsPossible, ClockMode::Stepped] {
        let (mut c, wall) = clock(mode);
        c.request_steps(1);
        tick(&mut c);
        wall.advance(Duration::from_secs(60));

        assert_eq!(c.behind(), None, "{mode:?}");
    }
}

/// REQ-K-3: a time scale must be finite and strictly positive.
#[test]
fn time_scale_rejects_zero_negative_nan_and_infinite() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(TimeScale::new(bad).is_err(), "{bad}");
    }
    assert_eq!(TimeScale::new(4.0).map(TimeScale::get), Ok(4.0));
}

/// REQ-C-1: advancing past `u64::MAX` is a `TimeOverflow` error and leaves time unchanged.
///
/// Needs `Clock::starting_at`, which the plan does not list (see the M1 running log).
#[test]
fn advance_at_u64_max_is_time_overflow() {
    let at_max = SimTime::at_tick(rate(), u64::MAX);
    let mut c = Clock::starting_at(
        at_max,
        ClockMode::AsFastAsPossible,
        Box::new(ManualWallClock::new()),
    );

    let got = c.advance();

    assert_eq!(got, Err(KernelError::TimeOverflow(u64::MAX)));
    assert_eq!(c.now(), at_max);
}

/// ASM-035: the manual clock starts at zero and `advance` moves it; clones share the time.
#[test]
fn manual_wall_clock_advances_and_clones_share_time() {
    let wall = ManualWallClock::new();
    let clone = wall.clone();
    assert_eq!(wall.now(), Duration::ZERO);

    wall.advance(Duration::from_millis(250));

    assert_eq!(clone.now(), Duration::from_millis(250));
}

/// ASM-035: with auto-advance every read of `now` moves the clock by the step.
#[test]
fn manual_wall_clock_auto_advance_moves_time_on_each_read() {
    let wall = ManualWallClock::with_auto_advance(Duration::from_millis(10));

    let first = wall.now();
    let second = wall.now();

    assert_eq!(second.abs_diff(first), Duration::from_millis(10));
}

/// ASM-035: `sleep_until` jumps the manual clock to the deadline and never goes backwards.
#[test]
fn manual_wall_clock_sleep_until_jumps_forward_only() {
    let wall = ManualWallClock::new();

    wall.sleep_until(Duration::from_secs(5));
    assert_eq!(wall.now(), Duration::from_secs(5));

    wall.sleep_until(Duration::from_secs(1));
    assert_eq!(wall.now(), Duration::from_secs(5));
}

/// ASM-035: the standard wall clock never goes backwards. This and the next test are the only
/// tests that read real time; they never sleep for a measurable duration.
#[test]
fn std_wall_clock_is_monotonic() {
    let wall = StdWallClock::new();

    let mut prev = wall.now();
    for _ in 0..1000 {
        let t = wall.now();
        assert!(t >= prev);
        prev = t;
    }
}

/// ASM-035: sleeping until a deadline that already passed returns immediately.
#[test]
fn std_wall_clock_sleep_until_past_deadline_returns() {
    let wall = StdWallClock::new();

    wall.sleep_until(Duration::ZERO);

    assert!(wall.now() >= Duration::ZERO);
}
