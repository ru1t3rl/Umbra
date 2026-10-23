//! AT-13 (M7, WI-6 review fixes): height changes and arrival of an aircraft that can hover
//! (`min_speed = 0`, the schema default) and the guards the reviews found untested. Requirements:
//! REQ-M-2, ADR 0051, ASM-607, ASM-702, ASM-707.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use common::{
    DT, angle_delta, climb_rate_of, dest, euler_of, flat, heading_of, height_of, move_to, speed_of,
    start,
};
use umbra_core::{
    Enu, FlatTerrain, Geodetic, Intent, KinematicState, LocalFrame, Meters, MetersPerSecond,
    MetersPerSecondSquared, MotionModel, Radians, Seconds, TerrainProvider,
};
use umbra_motion::{AirKinematic, AirKinematicParams};

const G: f64 = 9.80665;
const HZ: f64 = 30.0;
const CLIMB: f64 = 10.0;
const DESCENT: f64 = 8.0;
const ACCEL: f64 = 5.0;
/// Step counts at 30 Hz.
const STEPS_5_S: usize = 150;
const STEPS_60_S: usize = 1800;
const STEPS_90_S: usize = 2700;

/// A parameter set and its step.
#[derive(Clone, Copy)]
struct Rig {
    max_speed: f64,
    accel: f64,
    climb: f64,
    descent: f64,
    bank_deg: f64,
    dt: f64,
}

/// 60 m/s, 5 m/s^2, 10 m/s climb, 8 m/s descent, 30 degrees of bank, 30 Hz.
const SMALL: Rig = Rig {
    max_speed: 60.0,
    accel: ACCEL,
    climb: CLIMB,
    descent: DESCENT,
    bank_deg: 30.0,
    dt: 1.0 / HZ,
};
/// 200 m/s, 50 m/s^2, 30 m/s climb, 5 m/s descent, 60 degrees of bank: `accel * dt` of 50 m/s at
/// 1 s and 100 m/s at 2 s, far above the vertical rate limits.
const BIG: Rig = Rig {
    max_speed: 200.0,
    accel: 50.0,
    climb: 30.0,
    descent: 5.0,
    bank_deg: 60.0,
    dt: 1.0,
};

impl Rig {
    fn at(self, dt: f64) -> Self {
        Self { dt, ..self }
    }

    fn model(self) -> AirKinematic {
        model_with(self, 0.0, 0.0)
    }

    /// The turn rate at `speed`, rad/s.
    fn turn_rate(self, speed: f64) -> f64 {
        G * umbra_core::math::tan(self.bank_deg.to_radians()) / speed.max(1.0)
    }

    /// Steps allowed to land at `d` meters (horizontal) and `dh` meters (signed height change)
    /// from a start at speed `v0`: twice (stated headroom) the sum of the straight run at half
    /// of the top speed, the height change at the rate limit, a full turn at the turn rate of the
    /// speed reachable over `d`, and the horizontal and vertical ramps and stops (`2 v / a`
    /// each), plus ten steps for the last step.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive finite step count"
    )]
    fn step_bound(self, d: f64, dh: f64, v0: f64) -> usize {
        let rate = if dh >= 0.0 { self.climb } else { self.descent };
        let v_ref = (v0 * v0 + 2.0 * self.accel * d)
            .sqrt()
            .min(self.max_speed)
            .max(v0);
        let seconds = d / (0.5 * self.max_speed)
            + dh.abs() / rate
            + std::f64::consts::TAU / self.turn_rate(v_ref)
            + 2.0 * (self.max_speed + rate) / self.accel;
        (2.0 * seconds / self.dt).ceil() as usize + 10
    }
}

fn model_with(rig: Rig, min_speed: f64, agl: f64) -> AirKinematic {
    AirKinematic::new(AirKinematicParams {
        max_speed: MetersPerSecond(rig.max_speed),
        min_speed: MetersPerSecond(min_speed),
        max_accel: MetersPerSecondSquared(rig.accel),
        max_climb_rate: MetersPerSecond(rig.climb),
        max_descent_rate: MetersPerSecond(rig.descent),
        max_bank: Radians::from_degrees(rig.bank_deg),
        max_turn_rate: None,
        min_altitude_agl: Meters(agl),
    })
    .expect("valid params")
}

/// `model(min_speed, agl)` of the `SMALL` rig.
fn model(min_speed: f64, agl: f64) -> AirKinematic {
    model_with(SMALL, min_speed, agl)
}

/// The horizontal speed, m/s.
fn horizontal_speed(s: &KinematicState) -> f64 {
    let v = LocalFrame::at(s.position.to_geodetic()).vector_to_enu(s.velocity);
    umbra_core::math::hypot(v.east, v.north)
}

/// Horizontal distance from `s` to `goal`, meters.
fn miss(s: &KinematicState, goal: umbra_core::Ecef) -> f64 {
    let off = LocalFrame::at(s.position.to_geodetic()).to_enu(goal);
    umbra_core::math::hypot(off.east, off.north)
}

/// Horizontal distance between the latitude and longitude of `s` and of `goal`, meters (no frame,
/// so a tangent-plane residual of the model shows).
fn geo_miss(s: &KinematicState, goal: umbra_core::Ecef) -> f64 {
    let (a, b) = (s.position.to_geodetic(), goal.to_geodetic());
    let north = (a.lat().0 - b.lat().0) * 6_367_000.0;
    let east = (a.lon().0 - b.lon().0) * 6_367_000.0 * umbra_core::math::cos(a.lat().0);
    umbra_core::math::hypot(east, north)
}

fn goal_height(goal: umbra_core::Ecef) -> f64 {
    goal.to_geodetic().height().0
}

/// `start` with a vertical velocity of `rate` m/s.
fn start_rising(heading: f64, height: f64, rate: f64) -> KinematicState {
    let mut s = start(heading, 0.0, height);
    s.velocity = LocalFrame::at(s.position.to_geodetic()).vector_from_enu(Enu {
        east: 0.0,
        north: 0.0,
        up: rate,
    });
    s
}

/// On `goal` at rest: horizontally within 1 cm, height within 1 mm, horizontal speed 0.
fn landed(s: &KinematicState, goal: umbra_core::Ecef) -> bool {
    miss(s, goal) < 1e-2
        && geo_miss(s, goal) < 1e-4
        && (height_of(s) - goal_height(goal)).abs() < 1e-3
        && horizontal_speed(s) < 1e-9
        && climb_rate_of(s).abs() < 1e-9
}

/// One step toward `goal` with the per-step contract checked against `prev`: climb and descent
/// rates, the horizontal speed change and the vertical rate change (at most `accel * dt`; the
/// landing step may change either by twice that, ASM-701, ASM-707), and no overshoot of the
/// goal height (`from` and `goal` bracket the heights).
fn step_checked(
    rig: Rig,
    m: &AirKinematic,
    prev: &KinematicState,
    goal: umbra_core::Ecef,
    from_height: f64,
    exceptions: &mut usize,
) -> KinematicState {
    let next = m.step(
        prev,
        &[Intent::MoveTo { destination: goal }],
        Seconds(rig.dt),
        &flat(),
    );
    let limit = rig.accel * rig.dt;
    let rate = climb_rate_of(&next);
    assert!(
        rate <= rig.climb + 1e-6 && -rate <= rig.descent + 1e-6,
        "climb rate {rate}"
    );
    let dv = (horizontal_speed(&next) - horizontal_speed(prev)).abs();
    let dr = (rate - climb_rate_of(prev)).abs();
    assert!(
        dv <= 2.0 * limit + 1e-6 && dr <= 2.0 * limit + 1e-6,
        "speed step {dv}, rate step {dr}, limit {limit}"
    );
    if dv > limit + 1e-6 || dr > limit + 1e-6 {
        *exceptions += 1;
    }
    let target = goal_height(goal);
    let past = (height_of(&next) - target) * (target - from_height).signum();
    assert!(past <= 1e-6, "overshoot of the goal height by {past} m");
    next
}

/// Flies `from` toward `goal` until it has landed on it, within `step_bound` steps (the rig's
/// bound for `d` meters and the height change), then checks that it holds there for 60 steps.
/// Returns the steps to land.
fn land_and_hold(rig: Rig, from: KinematicState, goal: umbra_core::Ecef, what: &str) -> usize {
    let m = rig.model();
    let off = LocalFrame::at(from.position.to_geodetic()).to_enu(goal);
    let d = umbra_core::math::hypot(off.east, off.north);
    let dh = goal_height(goal) - height_of(&from);
    let limit = rig.step_bound(d, dh, horizontal_speed(&from));
    let mut s = from;
    let mut exceptions = 0;
    let mut steps = None;
    for i in 0..limit + 60 {
        s = step_checked(rig, &m, &s, goal, height_of(&from), &mut exceptions);
        // horizontally landed while the height is still flown out: exactly on the destination's
        // latitude and longitude (a tangent-plane residual of `dh * d / R` would show here)
        if horizontal_speed(&s) < 1e-9 && miss(&s, goal) < 1e-2 {
            assert!(
                geo_miss(&s, goal) < 1e-4,
                "{what}: {} m off",
                geo_miss(&s, goal)
            );
        }
        match steps {
            None if landed(&s, goal) => steps = Some(i),
            None => {}
            Some(_) => assert!(
                landed(&s, goal),
                "{what}: left the destination after landing (step {i}, {} m off)",
                miss(&s, goal)
            ),
        }
    }
    let steps = steps.unwrap_or_else(|| panic!("{what}: not landed in {limit} steps"));
    assert!(steps <= limit, "{what}: {steps} > {limit}");
    steps
}

/// REQ-M-2, ASM-702: a destination straight up from a stopped aircraft is reached.
#[test]
fn given_a_stopped_hover_capable_aircraft_when_the_destination_is_straight_up_then_it_climbs_there()
{
    land_and_hold(
        SMALL,
        start(0.0, 0.0, 1000.0),
        dest(0.0, 0.0, 1500.0),
        "straight up",
    );
}

/// ASM-707: 50 m away and 1000 m higher, from rest: it does not land early at 50 m, it holds
/// horizontally on the destination while it climbs.
#[test]
fn given_a_close_destination_far_above_when_stepped_from_rest_then_it_climbs_before_landing() {
    land_and_hold(
        SMALL,
        start(0.0, 0.0, 1000.0),
        dest(30.0, 40.0, 2000.0),
        "50 m, +1000 m",
    );
}

/// ASM-707: 2 km away, 1000 m higher and 900 m lower, from 30 m/s.
#[test]
fn given_a_far_destination_above_or_below_when_flown_from_speed_then_the_height_is_reached() {
    for up in [2000.0, 100.0] {
        land_and_hold(
            SMALL,
            start(0.0, 30.0, 1000.0),
            dest(1200.0, 1600.0, up),
            &format!("2 km, height {up}"),
        );
    }
}

/// ASM-707 (review round 2, B1): at a coarse step (`accel * dt` of 50 and 100 m/s) the
/// aircraft lands and holds: 50 m away and +1000 m from rest, and 2 km away and +1000 m from
/// 100 m/s. The first version never landed (a residual of centimeters blocked the snap and the
/// aircraft turned 180 degrees every step).
#[test]
fn given_a_coarse_step_when_the_destination_is_above_then_it_lands_and_holds() {
    for dt in [1.0, 2.0] {
        let rig = BIG.at(dt);
        land_and_hold(
            rig,
            start(0.0, 0.0, 1000.0),
            dest(30.0, 40.0, 2000.0),
            &format!("dt {dt}: 50 m, +1000 m"),
        );
        land_and_hold(
            rig,
            start(0.0, 100.0, 1000.0),
            dest(1200.0, 1600.0, 2000.0),
            &format!("dt {dt}: 2 km, +1000 m"),
        );
    }
}

/// ASM-707 (review round 2, B1): the creep case, 60/0/5/10/8/30 degrees at 2 s: it lands.
#[test]
fn given_a_two_second_step_when_the_destination_is_above_then_the_small_rig_lands_and_holds() {
    let rig = SMALL.at(2.0);
    land_and_hold(
        rig,
        start(0.0, 0.0, 1000.0),
        dest(30.0, 40.0, 2000.0),
        "dt 2: 50 m, +1000 m",
    );
    land_and_hold(
        rig,
        start(0.0, 0.0, 1000.0),
        dest(30.0, 40.0, 100.0),
        "dt 2: 50 m, -900 m",
    );
    land_and_hold(
        rig,
        start(0.0, 30.0, 1000.0),
        dest(1200.0, 1600.0, 2000.0),
        "dt 2: 2 km, +1000 m",
    );
}

/// ASM-707: the vertical rate ramps (at most `accel * dt` per step, ASM-707) and the height
/// arrives without overshoot, for a climb and a descent from rest.
#[test]
fn given_a_climb_and_a_descent_when_flown_then_the_rate_ramps_and_the_height_arrives() {
    for up in [1300.0, 700.0] {
        let m = SMALL.model();
        let goal = dest(0.0, 0.0, up);
        let mut s = start(0.0, 0.0, 1000.0);
        let (mut exceptions, mut peak) = (0, 0.0_f64);
        for _ in 0..STEPS_90_S {
            s = step_checked(SMALL, &m, &s, goal, 1000.0, &mut exceptions);
            peak = peak.max(climb_rate_of(&s).abs());
        }
        assert!(landed(&s, goal), "up {up}: height {}", height_of(&s));
        assert!(
            exceptions <= 1,
            "up {up}: {exceptions} steps over the limit"
        );
        let limit = if up > 1000.0 { CLIMB } else { DESCENT };
        assert!((peak - limit).abs() < 1e-9, "up {up}: peak rate {peak}");
    }
}

/// ASM-707: the vertical rate of a hover-capable aircraft is limited by the rate limit, not by its
/// horizontal speed: at 3 m/s it reaches the 10 m/s climb rate (fixed wing keeps its cap).
#[test]
fn given_a_slow_hover_capable_aircraft_when_climbing_then_the_rate_reaches_the_limit() {
    let m = model(0.0, 0.0);
    let intents = [
        Intent::SetSpeed(MetersPerSecond(3.0)),
        move_to(0.0, 5000.0, 4000.0),
    ];
    let mut s = start(0.0, 3.0, 2000.0);
    let mut peak = 0.0_f64;
    for _ in 0..STEPS_5_S {
        s = m.step(&s, &intents, DT, &flat());
        peak = peak.max(climb_rate_of(&s));
    }

    assert!((peak - CLIMB).abs() < 1e-9, "{peak}");
    assert!((horizontal_speed(&s) - 3.0).abs() < 1e-9);
}

/// ASM-607: a fixed-wing aircraft (minimum speed > 0) keeps the 45 degree cap: at 3 m/s it would
/// climb at 3 / sqrt(2) m/s (the speed floor of 3 m/s here).
#[test]
fn given_a_slow_fixed_wing_aircraft_when_climbing_then_the_rate_is_at_most_speed_over_sqrt_two() {
    let from = start(0.0, 3.0, 2000.0);
    let intents = [
        Intent::SetSpeed(MetersPerSecond(3.0)),
        move_to(0.0, 5000.0, 4000.0),
    ];

    let s = model(3.0, 0.0).step(&from, &intents, DT, &flat());

    let rate = climb_rate_of(&s);
    assert!(
        (rate - 3.0 * core::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9,
        "{rate}"
    );
    assert!((speed_of(&s) - 3.0).abs() < 1e-9);
}

/// ASM-607, ASM-702: a stopped hover-capable aircraft below the terrain floor climbs to it,
/// ramping to the rate limit and stopping on the floor without overshoot.
#[test]
fn given_a_stopped_hover_capable_aircraft_below_the_floor_when_stepped_then_it_climbs_to_it() {
    let terrain = FlatTerrain {
        height: Meters(100.0),
    };
    let m = model(0.0, 20.0);
    let mut s = start(0.0, 0.0, 50.0);

    let mut peak = 0.0_f64;
    for _ in 0..600 {
        s = m.step(&s, &[], DT, &terrain);
        peak = peak.max(climb_rate_of(&s));
        assert!(height_of(&s) <= 120.0 + 1e-6, "overshoot {}", height_of(&s));
    }

    assert!((peak - CLIMB).abs() < 1e-9, "peak {peak}");
    assert!(
        (height_of(&s) - 120.0).abs() < 1e-6,
        "height {}",
        height_of(&s)
    );
    assert!(horizontal_speed(&s) < 1e-9);
}

/// One step at rest within 1 mm of the destination, rising at `rate0`, with the destination `up`
/// meters above.
fn one_step_to(up: f64, rate0: f64) -> (KinematicState, KinematicState, umbra_core::Ecef) {
    let from = start_rising(0.0, 1000.0, rate0);
    let goal = dest(0.0, 0.0005, 1000.0 + up);
    let next = model(0.0, 0.0).step(&from, &[Intent::MoveTo { destination: goal }], DT, &flat());
    (from, next, goal)
}

/// ASM-707: rising at 0.1 m/s, with the height error within the step the stopping speed allows
/// (0.005 m), the step lands on the destination, height included, and the rate ends at 0.
#[test]
fn given_a_height_error_inside_one_climb_step_when_over_the_destination_then_it_lands_on_it() {
    let (_, next, goal) = one_step_to(0.005, 0.1);

    assert!((height_of(&next) - goal_height(goal)).abs() < 1e-9);
    assert!(miss(&next, goal) < 1e-6);
    assert!(horizontal_speed(&next) < 1e-12);
    assert!(climb_rate_of(&next).abs() < 1e-12);
}

/// ASM-707: with a larger error (0.02 m) the step only ramps the rate by `accel * dt` and climbs
/// at the new rate (no jump to the destination's height), over the destination.
#[test]
fn given_a_height_error_outside_one_climb_step_when_over_the_destination_then_it_ramps_one_step() {
    let (from, next, goal) = one_step_to(0.02, 0.1);

    let rate = 0.1 + ACCEL / HZ;
    assert!(
        (climb_rate_of(&next) - rate).abs() < 1e-9,
        "{}",
        climb_rate_of(&next)
    );
    // the ECEF round trip of a height at 6.4e6 m is good to about 2e-9
    assert!((height_of(&next) - height_of(&from) - rate / HZ).abs() < 1e-7);
    assert!(miss(&next, goal) < 1e-3);
    assert!(horizontal_speed(&next) < 1e-12);
}

/// ASM-707: a close target the aircraft is moving away from is not landed on (the rate must
/// point to the target): descending at 0.3 m/s with the target 3 mm above, it only ramps up.
#[test]
fn given_a_close_target_behind_the_vertical_motion_when_stepped_then_it_is_not_landed_on() {
    let (_, next, _) = one_step_to(0.003, -0.3);

    let rate = -0.3 + ACCEL / HZ;
    assert!(
        (climb_rate_of(&next) - rate).abs() < 1e-9,
        "{}",
        climb_rate_of(&next)
    );
}

/// ASM-707: the vertical landing needs both rates within `2 * accel * dt`: rising at 1 m/s with
/// the target 2 cm above (inside the next step's travel) the rate only ramps down.
#[test]
fn given_a_fast_vertical_approach_when_close_then_the_rate_ramps_instead_of_landing() {
    let (_, next, _) = one_step_to(0.02, 1.0);

    let rate = 1.0 - ACCEL / HZ;
    assert!(
        (climb_rate_of(&next) - rate).abs() < 1e-9,
        "{}",
        climb_rate_of(&next)
    );
}

/// ASM-707: the rate limits win over the ramp: handed a climb of 50 m/s (another model's state),
/// the aircraft is at the 10 m/s limit after one step.
#[test]
fn given_a_vertical_rate_over_the_limit_when_stepped_then_the_limit_wins() {
    let from = start_rising(0.0, 1000.0, 50.0);
    let goal = dest(0.0, 0.0, 9000.0);

    let up = model(0.0, 0.0).step(&from, &[Intent::MoveTo { destination: goal }], DT, &flat());
    let down = model(0.0, 0.0).step(
        &start_rising(0.0, 1000.0, -50.0),
        &[Intent::MoveTo {
            destination: dest(0.0, 0.0, 10.0),
        }],
        DT,
        &flat(),
    );

    assert!((climb_rate_of(&up) - CLIMB).abs() < 1e-9);
    assert!((climb_rate_of(&down) + DESCENT).abs() < 1e-9);
}

/// ASM-707: the vertical landing needs the old rate within `2 * accel * dt` too: at +0.4 m/s with
/// the target 5 mm above, the new rate (0.2333) would be inside the limit, but the old one is not,
/// so the rate only ramps down.
#[test]
fn given_a_fast_old_rate_when_the_new_rate_is_small_then_the_vertical_landing_waits() {
    let (_, next, _) = one_step_to(0.005, 0.4);

    let rate = 0.4 - ACCEL / HZ;
    assert!(
        (climb_rate_of(&next) - rate).abs() < 1e-9,
        "{}",
        climb_rate_of(&next)
    );
}

/// ASM-707: the same for a descent.
#[test]
fn given_a_height_error_around_one_descent_step_when_over_the_destination_then_the_limit_holds() {
    let (_, inside, goal) = one_step_to(-0.005, -0.1);
    assert!((height_of(&inside) - goal_height(goal)).abs() < 1e-9);
    assert!(horizontal_speed(&inside) < 1e-12);
    assert!(climb_rate_of(&inside).abs() < 1e-12);

    let (from, outside, _) = one_step_to(-0.02, -0.1);
    let rate = -(0.1 + ACCEL / HZ);
    assert!((climb_rate_of(&outside) - rate).abs() < 1e-9);
    assert!((height_of(&outside) - height_of(&from) - rate / HZ).abs() < 1e-7);
}

/// ASM-707 (review round 2, B1): at rest, 2 mm behind the heading and a large height error, at a
/// step of 2 s: the aircraft lands horizontally within a few steps (no 180 degree turn every step)
/// and stays.
#[test]
fn given_a_residual_behind_the_heading_when_at_rest_then_the_aircraft_lands_and_stays() {
    let rig = BIG.at(2.0);
    let goal = dest(0.0, -0.002, 2500.0);
    let m = rig.model();
    let mut s = start(0.0, 0.0, 1000.0);

    let mut horizontal_done = None;
    for i in 0..40 {
        s = m.step(
            &s,
            &[Intent::MoveTo { destination: goal }],
            Seconds(2.0),
            &flat(),
        );
        let on = miss(&s, goal) < 1e-6 && horizontal_speed(&s) < 1e-9;
        match horizontal_done {
            None if on => horizontal_done = Some(i),
            None => {}
            Some(_) => assert!(on, "step {i}: left the destination ({} m)", miss(&s, goal)),
        }
    }

    assert!(
        horizontal_done.is_some_and(|i| i <= 3),
        "{horizontal_done:?}"
    );
}

/// ASM-707 (review round 2, M1): climbing in place straight up at 30 Hz does not spin the
/// aircraft: the heading is the start heading on every step, pitch is 45 degrees (the cap, as a
/// flight-path angle limit) and the aircraft stays over the point.
#[test]
fn given_a_climb_straight_up_when_stepped_then_the_heading_is_constant_and_pitch_is_capped() {
    let m = SMALL.model();
    let goal = dest(0.0, 0.0, 1500.0);
    let mut s = start(1.0, 0.0, 1000.0);

    for i in 0..STEPS_60_S {
        s = m.step(&s, &[Intent::MoveTo { destination: goal }], DT, &flat());
        let euler = euler_of(&s);
        assert!(
            angle_delta(euler.heading.0, 1.0).abs() < 1e-6,
            "step {i}: heading {}",
            euler.heading.0
        );
        if climb_rate_of(&s) > 1e-9 {
            assert!(
                (euler.pitch.0 - core::f64::consts::FRAC_PI_4).abs() < 1e-9,
                "step {i}: pitch {}",
                euler.pitch.0
            );
        }
        assert!(miss(&s, goal) < 1e-6);
    }
}

/// ASM-707 (review round 2, m1): a descent straight down from rest reports -45 degrees (not
/// -90) and keeps its heading.
#[test]
fn given_a_descent_from_rest_when_stepped_then_pitch_is_minus_the_cap_and_the_heading_is_kept() {
    let m = SMALL.model();
    let goal = dest(0.0, 0.0, 500.0);
    let mut s = start(1.0, 0.0, 1000.0);

    for i in 0..30 {
        s = m.step(&s, &[Intent::MoveTo { destination: goal }], DT, &flat());
        let euler = euler_of(&s);
        assert!(
            (euler.pitch.0 + core::f64::consts::FRAC_PI_4).abs() < 1e-9,
            "step {i}: pitch {}",
            euler.pitch.0
        );
        assert!(angle_delta(euler.heading.0, 1.0).abs() < 1e-6);
    }
}

/// ASM-707 (review round 2, m2): when the horizontal speed exceeds the vertical rate the pitch is
/// the plain flight-path angle (here atan(2 / 4)), only the steeper cases are capped.
#[test]
fn given_a_shallow_climb_when_stepped_then_pitch_is_the_flight_path_angle() {
    let m = SMALL.model();
    // 4 m/s forward and rising at 2 m/s already, far destination up and ahead, speed held at 4
    let mut from = start(0.0, 4.0, 1000.0);
    from.velocity = LocalFrame::at(from.position.to_geodetic()).vector_from_enu(Enu {
        east: 0.0,
        north: 4.0,
        up: 2.0,
    });
    let intents = [
        Intent::SetSpeed(MetersPerSecond(4.0)),
        move_to(0.0, 9000.0, 4000.0),
    ];

    let s = m.step(&from, &intents, DT, &flat());

    let flight_path = umbra_core::math::atan2(climb_rate_of(&s), horizontal_speed(&s));
    assert!(flight_path > 0.1 && flight_path < 0.7, "{flight_path}");
    assert!((euler_of(&s).pitch.0 - flight_path).abs() < 1e-9);
}

/// ASM-608 (guard): a hover-capable aircraft at rest told to face another way does not pivot on
/// the spot: heading, speed and position are unchanged.
#[test]
fn given_a_stopped_hover_capable_aircraft_when_told_a_heading_then_it_does_not_pivot() {
    let from = start(0.0, 0.0, 1000.0);

    let next = model(0.0, 0.0).step(&from, &[Intent::SetHeading(Radians(1.0))], DT, &flat());

    assert!(angle_delta(heading_of(&next), 0.0).abs() < 1e-9);
    assert!(speed_of(&next) < 1e-12);
    assert!((next.position - from.position).norm() < 1e-9);
}

/// ASM-694 (guard): without a minimum speed there is no smallest turning circle, so a destination
/// 0.5 m behind a 10 m/s aircraft is turned toward (not held straight until out of a circle).
#[test]
fn given_no_minimum_speed_when_the_destination_is_just_behind_then_the_aircraft_turns() {
    let from = start(0.0, 10.0, 1000.0);

    let next = model(0.0, 0.0).step(&from, &[move_to(0.0, -0.5, 1000.0)], DT, &flat());

    assert!(
        angle_delta(heading_of(&next), 0.0).abs() > 1e-4,
        "heading {}",
        heading_of(&next)
    );
}

/// Terrain whose height is a constant (possibly not finite), for the floor's finite check.
struct ConstTerrain(f64);

impl TerrainProvider for ConstTerrain {
    fn height_at(&self, _at: Geodetic) -> Meters {
        Meters(self.0)
    }

    fn surface_normal_at(&self, _at: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// ASM-702: a terrain height that is not finite (a plugin could return one) drops the floor: the
/// aircraft keeps its height instead of climbing without end or going non-finite.
#[test]
fn given_a_non_finite_terrain_height_when_stepped_then_the_floor_is_dropped() {
    for bad in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let m = model(0.0, 50.0);
        let mut s = start(0.0, 40.0, 1000.0);
        for _ in 0..60 {
            s = m.step(
                &s,
                &[Intent::SetHeading(Radians(0.0))],
                DT,
                &ConstTerrain(bad),
            );
        }
        assert!(
            (height_of(&s) - 1000.0).abs() < 1e-6,
            "{bad}: {}",
            height_of(&s)
        );
        assert!(s.position.0.x.is_finite());
    }
}

/// The floor of `terrain` plus `agl` at the position of `s`.
fn floor_of(terrain: &dyn TerrainProvider, s: &KinematicState, agl: f64) -> f64 {
    terrain.height_at(s.position.to_geodetic()).0 + agl
}

/// Steps `m` over `terrain` for `steps` steps with `intents` and checks, for every step, that the
/// aircraft is not more than `allowance` meters under the floor and that a vertical-rate change
/// above `accel * dt` happens only where the step ends on the floor or at a rate limit (the floor
/// wins over the ramp, ASM-707) or in a landing step (at most twice the limit).
fn fly_over_terrain(
    rig: Rig,
    agl: f64,
    terrain: &dyn TerrainProvider,
    from: KinematicState,
    intents: &[Intent],
    steps: usize,
    allowance: f64,
) {
    let m = model_with(rig, 0.0, agl);
    let ramp = rig.accel * rig.dt;
    let mut s = from;
    for i in 0..steps {
        let next = m.step(&s, intents, Seconds(rig.dt), terrain);
        let floor = floor_of(terrain, &next, agl);
        assert!(
            height_of(&next) >= floor - allowance,
            "step {i}: {} m under the floor {floor} m",
            floor - height_of(&next)
        );
        let (r0, r1) = (climb_rate_of(&s), climb_rate_of(&next));
        let on_floor = (height_of(&next) - floor).abs() < 1e-6;
        let at_limit = (r1 - rig.climb).abs() < 1e-9 || (r1 + rig.descent).abs() < 1e-9;
        if (r1 - r0).abs() > ramp + 1e-9 && !(on_floor || at_limit) {
            assert!(
                (r1 - r0).abs() <= 2.0 * ramp + 1e-9,
                "step {i}: rate {r0} -> {r1} is not explained by the floor or a limit"
            );
        }
        s = next;
    }
}

/// 200 m/s, 5 m/s^2, 15 m/s climb, 10 m/s descent, 30 degrees of bank: the WI-1 floor rig.
const FLOOR_RIG: Rig = Rig {
    max_speed: 200.0,
    accel: 5.0,
    climb: 15.0,
    descent: 10.0,
    bank_deg: 30.0,
    dt: 1.0 / HZ,
};

/// ASM-707, ADR 0051 (the floor wins over the ramp): the WI-1 rising-terrain scenario (5% slope,
/// 100 m/s, a far low destination) for an aircraft that can hover stays above the floor, at three
/// steps and two floors. The ramp alone dipped 8.9 m under it.
#[test]
fn given_rising_terrain_when_a_hover_capable_aircraft_flies_then_it_stays_above_the_floor() {
    for dt in [1.0 / HZ, 0.5, 2.0] {
        for agl in [0.0, 100.0] {
            fly_over_terrain(
                FLOOR_RIG.at(dt),
                agl,
                &common::SlopedTerrain { slope: 0.05 },
                start(0.0, 100.0, agl + 10.0),
                &[move_to(0.0, 30_000.0, -50.0)],
                600,
                0.5,
            );
        }
    }
}

/// ASM-707: descending at the full descent rate (8 m/s) at 30 m/s onto terrain that rises at 80% of
/// the climb rate (a 26.7% slope) stays above the floor: the floor wins over the ramp (18.9 m
/// under it before).
#[test]
fn given_a_full_descent_onto_a_steep_ramp_when_stepped_then_the_floor_holds() {
    fly_over_terrain(
        SMALL,
        20.0,
        &common::SlopedTerrain {
            slope: 0.8 * CLIMB / 30.0,
        },
        start(0.0, 30.0, 120.0),
        &[
            Intent::SetSpeed(MetersPerSecond(30.0)),
            move_to(0.0, 50_000.0, -100.0),
        ],
        1500,
        0.5,
    );
}

/// Terrain at height 0 up to a millimeter north of the origin and `rise` meters higher beyond: the
/// floor a step ahead is `rise` higher than the floor under the aircraft.
struct Ledge {
    rise: f64,
}

impl TerrainProvider for Ledge {
    fn height_at(&self, at: Geodetic) -> Meters {
        let north_m = (at.lat().0 - common::origin().lat().0) * 6_367_000.0;
        Meters(if north_m > 0.001 { self.rise } else { 0.0 })
    }

    fn surface_normal_at(&self, _at: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// ASM-707 (one step): at rest on the floor with the floor 0.3 m higher one step ahead (a step
/// ahead is 5 mm at the first step's speed), the step ends on that floor, at a rate of 9 m/s
/// (above `accel * dt`: the floor wins over the ramp); with the floor 3 m higher the rate is the
/// 10 m/s climb limit and the aircraft climbs one step of it.
#[test]
fn given_a_floor_rising_one_step_ahead_when_at_rest_then_the_step_ends_on_it_or_at_the_limit() {
    let from = start(0.0, 0.0, 0.0);
    let intents = [move_to(0.0, 100.0, 0.0)];

    let near = model(0.0, 0.0).step(&from, &intents, DT, &Ledge { rise: 0.3 });
    let far = model(0.0, 0.0).step(&from, &intents, DT, &Ledge { rise: 3.0 });

    assert!(
        (height_of(&near) - 0.3).abs() < 1e-6,
        "{}",
        height_of(&near)
    );
    assert!(
        (climb_rate_of(&near) - 9.0).abs() < 1e-6,
        "{}",
        climb_rate_of(&near)
    );
    assert!((climb_rate_of(&far) - CLIMB).abs() < 1e-9);
    assert!((height_of(&far) - CLIMB / HZ).abs() < 1e-6);
}

/// ASM-707 (guard): the horizontal landing snap uses the heading error after the step's turn.
/// At rest, 2 mm from the destination, with a turn step of 2.05 rad (117 degrees; the travel guard
/// needs a turn of at least `2 sin e` radians to reach 2 mm, so the turn is large): a destination
/// at 100 degrees (turned fully, 0 after) and one at 170 degrees (turned 117, 53 after) are landed
/// on. Adding the turn to the error (200 and 287 degrees) or taking half of it (about 111
/// degrees for the second) would refuse the landing.
#[test]
fn given_a_large_partial_turn_in_the_step_when_the_destination_is_near_then_it_lands() {
    let rate = G * umbra_core::math::tan(30.0_f64.to_radians());
    let dt = 2.05 / rate;
    let m = model(0.0, 0.0);
    let from = start(0.0, 0.0, 1000.0);
    for bearing_deg in [100.0_f64, 170.0] {
        let (sin, cos) = umbra_core::math::sin_cos(bearing_deg.to_radians());
        let goal = dest(0.002 * sin, 0.002 * cos, 1000.0);

        let next = m.step(
            &from,
            &[Intent::MoveTo { destination: goal }],
            Seconds(dt),
            &flat(),
        );

        assert!(
            miss(&next, goal) < 1e-6 && horizontal_speed(&next) < 1e-12,
            "bearing {bearing_deg}: {} m off",
            miss(&next, goal)
        );
    }
}

/// ASM-707: in a landing step the floor is sampled where the aircraft lands (the destination), not
/// one travel ahead: on a 50% slope, 0.5 m from the destination at rest with a 2 s step (the travel
/// is 1 m), the step ends on the floor of the destination (0.25 m), not on the floor 1 m ahead.
#[test]
fn given_a_landing_step_on_a_slope_when_stepped_then_the_floor_is_sampled_at_the_destination() {
    let terrain = common::SlopedTerrain { slope: 0.5 };
    let goal = dest(0.0, 0.5, 0.0);

    let next = BIG.at(2.0).model().step(
        &start(0.0, 0.0, 0.0),
        &[Intent::MoveTo { destination: goal }],
        Seconds(2.0),
        &terrain,
    );

    assert!(miss(&next, goal) < 1e-6 && horizontal_speed(&next) < 1e-12);
    let floor = floor_of(&terrain, &next, 0.0);
    assert!(
        (height_of(&next) - floor).abs() < 1e-6,
        "{} vs floor {floor}",
        height_of(&next)
    );
}
