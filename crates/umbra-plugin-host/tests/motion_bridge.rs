//! AT-16 (M7 WI-2, motion rows): the host bridge of the motion model kind (ABI 0.2, ADR 0050; REQ-P-4,
//! REQ-P-5, REQ-M-1, REQ-M-4, REQ-C-6, ASM-612, ASM-613) through the SDK-built fixture plugin
//! `test.plugin.kinds`: a plugin motion model passes the core `motion::contract` suite, steps like the
//! reference, sees the terrain of the call exactly as the provider answers, and one model's fault
//! leaves another working. The fault rows (output the host refuses, panics) are in `plugin_failures.rs`
//! (gate 13).
//!
//! Raw-ABI misbehavior the safe SDK cannot produce (a stale or foreign terrain token, a token used
//! from another thread) is in the module `raw_rows` (WI-8, raw fixture in `support/kinds.rs`); the
//! engine-level fault policies are in `umbra-engine/tests/plugin_kinds_faults.rs`.
//!
//! Step 4 of M7 (WI-2), un-ignored with the bridge in WI-8; the fixture is the transitional
//! `kinds_motion_sensor` build until WI-9 (ASM-718).

#![expect(
    unsafe_code,
    reason = "the raw-ABI rows (WI-8) publish a forged plugin with `forged!`"
)]

#[macro_use]
mod support;

use support::kinds::{DT, Slope, Wall, env, state_at, step};
use umbra_core::motion::contract::check_motion_model;
use umbra_core::{Ecef, Geodetic, Intent, MetersPerSecond, TerrainProvider};

fn distance(a: Ecef, b: Ecef) -> f64 {
    (a - b).norm()
}

#[test]
fn given_the_coast_model_when_checked_by_the_core_contract_then_it_passes() {
    let env = env();
    let model = env.motion("test.motion.coast", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };
    let cases = vec![
        (
            state_at(4.0, 50.0, 12.0),
            vec![Intent::SetSpeed(MetersPerSecond(5.0))],
        ),
        (state_at(4.0, 50.0, 0.0), vec![]),
        (state_at(4.0, 50.0, 0.0), vec![Intent::Hold]),
    ];

    let verdict = check_motion_model(model.as_ref(), &cases, DT, &terrain);

    assert_eq!(verdict, Ok(()));
    assert_eq!(env.host.faults(), Vec::new());
}

#[test]
fn given_the_coast_model_when_stepped_then_position_advances_by_velocity_times_dt() {
    let env = env();
    let model = env.motion("test.motion.coast", "{}").expect("build");
    let start = state_at(4.0, 50.0, 12.0);

    let next = step(model.as_ref(), &start, &[], &Slope { per_rad: 0.0 });

    let want = start.position + start.velocity * DT.0;
    assert!(distance(next.position, want) < 1e-9);
    assert_eq!(next.velocity, start.velocity);
    assert_eq!(next.orientation, start.orientation);
}

/// REQ-C-6, ADR 0050: terrain answers equal the provider's (height, normal, line of sight).
#[test]
fn given_the_terrain_echo_model_when_stepped_then_it_reports_what_the_provider_answers() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };
    let start = state_at(4.0, 500.0, 0.0);
    let at = start.position.to_geodetic();

    let next = step(model.as_ref(), &start, &[], &terrain);

    assert_eq!(next.position, start.position);
    assert_eq!(next.velocity.x, terrain.height_at(at).0);
    assert_eq!(next.velocity.y, terrain.surface_normal_at(at).up);
    assert_eq!(next.velocity.z, 1.0, "line of sight above gentle terrain");
}

#[test]
fn given_a_wall_when_the_echo_model_looks_up_from_inside_it_then_line_of_sight_is_blocked() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let lon = 4.0_f64.to_radians();
    let wall = Wall {
        lon_lo: lon - 0.001,
        lon_hi: lon + 0.001,
    };
    let start = state_at(4.0, 10.0, 0.0);

    let next = step(model.as_ref(), &start, &[], &wall);

    assert_eq!(next.velocity.x, 300.0);
    assert_eq!(next.velocity.z, 0.0);
}

/// The query ignores the point's own height (the `TerrainProvider` contract).
#[test]
fn given_two_heights_over_one_point_when_echoed_then_the_terrain_height_is_the_same() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };

    let heights: Vec<f64> = [10.0, 9000.0]
        .into_iter()
        .map(|alt| {
            step(model.as_ref(), &state_at(4.0, alt, 0.0), &[], &terrain)
                .velocity
                .x
        })
        .collect();

    let ground = terrain
        .height_at(Geodetic::from_degrees(52.0, 4.0, 0.0).expect("valid"))
        .0;
    // WI-8 (ASM-721): the host converts each ECEF point as a built-in does (`Ecef::to_geodetic`), whose
    // longitude can differ from the input degrees by an ulp after the round trip, so each answer equals
    // the provider's at the point's own conversion, and both are the ground height within that.
    let own: Vec<f64> = [10.0, 9000.0]
        .into_iter()
        .map(|alt| {
            terrain
                .height_at(state_at(4.0, alt, 0.0).position.to_geodetic())
                .0
        })
        .collect();
    assert_eq!(heights, own);
    assert!(
        heights.iter().all(|h| (h - ground).abs() <= 1e-12 * ground),
        "{heights:?} vs {ground}"
    );
}

/// WI-8: the bridged model is `Send + Sync` like every core model (the kernel shares
/// `Arc<dyn MotionModel>`): stepped from two threads at once (the calls serialise on the image's call
/// lock, each with its own call record and terrain token), equal to a step on this thread, and dropped
/// last on the other thread, where `destroy` runs without an error.
#[test]
fn given_a_plugin_motion_model_shared_with_another_thread_then_it_steps_there_and_drops_last_there()
{
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };
    let start = state_at(4.0, 500.0, 0.0);
    let want = step(model.as_ref(), &start, &[], &terrain);
    let other = std::sync::Arc::clone(&model);
    let (release, released) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let got: Vec<_> = (0..50)
            .map(|_| step(other.as_ref(), &start, &[], &terrain))
            .collect();
        released.recv().expect("the main thread dropped its model");
        drop(other);
        got
    });

    let here: Vec<_> = (0..50)
        .map(|_| step(model.as_ref(), &start, &[], &terrain))
        .collect();
    drop(model);
    release.send(()).expect("the worker waits");
    let there = worker.join().expect("the worker did not panic");

    assert!(here.iter().chain(&there).all(|s| *s == want));
    assert_eq!(env.host.faults(), Vec::new());
    let errors: Vec<_> = env
        .log
        .all()
        .into_iter()
        .filter(|(_, level, _)| *level == umbra_plugin_host::LogLevel::Error)
        .collect();
    assert_eq!(errors, Vec::new(), "destroy ran on the worker");
}

// ------------------------------------------------------------------ raw-ABI terrain token rows (WI-8)
//
// ADR 0050, ADR 0019 RR-M2: a terrain token is valid only in the call that carries it, on the thread of
// that call. The raw fixture returns `UMBRA_OK` whatever the host answered, so a fault below comes from
// the host's own call record, not from the plugin's code.

mod raw_rows {
    use std::cell::Cell;

    use super::support::kinds::{
        Counting, KEPT, PROVIDER_PANIC, Panicking, RAW_MOTION, RawEnv, UNTOUCHED, queries, query,
        raw_parts, sense, state_at, step, world,
    };
    use umbra_core::{Ecef, PluginFaultPolicy, TerrainProvider, Vec3};
    use umbra_plugin_api::{UMBRA_ERR_INTERNAL, UMBRA_ERR_STATE, UMBRA_OK, UmbraVec3};
    use umbra_plugin_host::{HostMode, LogLevel};

    fn slope() -> Counting {
        Counting {
            slope: 1000.0,
            ..Counting::default()
        }
    }

    fn abi(p: Ecef) -> UmbraVec3 {
        UmbraVec3 {
            x: p.0.x,
            y: p.0.y,
            z: p.0.z,
        }
    }

    /// The fault of the raw motion model in `step`, naming a stale or foreign terrain token.
    fn assert_motion_token_fault(env: &RawEnv) {
        let faults = env.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!((faults[0].0.as_str(), faults[0].1), (RAW_MOTION, "step"));
        assert!(
            faults[0].2.contains("terrain") && faults[0].2.contains("stale or foreign"),
            "{}",
            faults[0].2
        );
    }

    #[test]
    fn given_a_terrain_token_kept_from_a_finished_step_when_used_in_a_later_step_then_refused_and_the_model_faults()
     {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let keep = env.motion("keep");
        let stale = env.motion("stale");
        let terrain = slope();
        let start = state_at(4.0, 50.0, 12.0);
        let ground = terrain.height_at(start.position.to_geodetic()).0;
        assert_eq!(terrain.calls(), 1, "the reference query");

        let kept = step(keep.as_ref(), &start, &[], &terrain);
        let refused = step(stale.as_ref(), &start, &[], &terrain);

        assert_eq!(kept, start, "the keeping step wrote nothing and is clean");
        assert_eq!(
            queries(),
            [(UMBRA_OK, ground), (UMBRA_ERR_STATE, UNTOUCHED)],
            "the kept token answers in its own call only"
        );
        assert_eq!(
            terrain.calls(),
            2,
            "the stale query never reached the provider"
        );
        assert_eq!(refused.position, start.position);
        assert_eq!(refused.velocity, Vec3::ZERO, "held (ASM-614)");
        assert_motion_token_fault(&env);
        assert!(env.host.take_abort().is_some(), "abort pending");

        let again = step(stale.as_ref(), &start, &[], &terrain);

        assert_eq!(again, refused, "still held");
        assert_eq!(
            queries().len(),
            2,
            "a faulted capability is never called again"
        );
        assert_eq!(env.faults().len(), 1);
    }

    #[test]
    fn given_a_terrain_token_kept_from_a_finished_step_when_used_outside_any_call_then_err_state() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let keep = env.motion("keep");
        let terrain = slope();
        let start = state_at(4.0, 50.0, 12.0);
        let _ = step(keep.as_ref(), &start, &[], &terrain);
        let (token, height) = KEPT
            .with(Cell::get)
            .expect("the step kept its terrain service");

        let after = query(height, token, abi(start.position));

        assert_eq!(after, (UMBRA_ERR_STATE, UNTOUCHED));
        assert_eq!(
            terrain.calls(),
            1,
            "only the in-call query reached the provider"
        );
        assert_eq!(
            env.faults(),
            Vec::new(),
            "no call to blame: the step itself was clean"
        );
    }

    /// A sensor's terrain token (another capability's call) used in a motion step is refused.
    #[test]
    fn given_a_sensors_terrain_token_when_used_in_a_motion_step_then_refused_and_only_the_motion_model_faults()
     {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::DisableCapability);
        let keeper = env.sensor("keep");
        let stale = env.motion("stale");
        let terrain = slope();
        let world = world();

        let (contacts, _) = sense(keeper.as_ref(), &world, &terrain);
        let refused = step(stale.as_ref(), &world[0].1, &[], &terrain);

        assert_eq!(contacts, Vec::new());
        assert_eq!(queries().len(), 2);
        assert_eq!(queries()[0].0, UMBRA_OK, "the sensor's own query");
        assert_eq!(queries()[1], (UMBRA_ERR_STATE, UNTOUCHED));
        assert_eq!(terrain.calls(), 1);
        assert_eq!(refused.velocity, Vec3::ZERO, "held");
        assert_motion_token_fault(&env);
    }

    /// ADR 0050, ASM-720: the step record carries `dt`, tick 0, the rate `1 / dt` and this tick's
    /// intents in emission order, written in the plugin's minor.
    #[test]
    fn a_step_carries_dt_the_derived_rate_and_the_intents_in_order() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let model = env.motion("record");
        let intents = [
            umbra_core::Intent::SetSpeed(umbra_core::MetersPerSecond(5.0)),
            umbra_core::Intent::Hold,
            umbra_core::Intent::SetHeading(umbra_core::Radians(0.25)),
        ];

        let next = step(
            model.as_ref(),
            &state_at(4.0, 50.0, 0.0),
            &intents,
            &slope(),
        );

        assert_eq!(next, state_at(4.0, 50.0, 0.0));
        let kind = |k: u32| i32::try_from(k).expect("small");
        assert_eq!(
            queries(),
            [
                (0, super::DT.0),
                (30, 0.0),
                (kind(umbra_plugin_api::UMBRA_INTENT_SET_SPEED), 5.0),
                (kind(umbra_plugin_api::UMBRA_INTENT_HOLD), 0.0),
                (kind(umbra_plugin_api::UMBRA_INTENT_SET_HEADING), 0.25),
            ]
        );
        assert_eq!(env.faults(), Vec::new());
    }

    /// The core precondition: a `dt` that is not finite and positive returns the input state without
    /// a plugin call.
    #[test]
    fn a_step_with_a_dt_that_is_not_positive_returns_the_input_without_a_call() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let model = env.motion("record");
        let start = state_at(4.0, 50.0, 12.0);

        for dt in [0.0, -1.0, f64::NAN] {
            let next = model.step(&start, &[], umbra_core::Seconds(dt), &slope());
            assert_eq!(next, start, "dt {dt}");
        }

        assert_eq!(queries(), Vec::new());
        assert_eq!(env.faults(), Vec::new());
    }

    /// ADR 0019 RR-M2: a callback from a thread without a call record is refused without any lookup;
    /// the host cannot tie it to the step, so the step itself stays clean.
    #[test]
    fn given_a_terrain_token_used_from_another_thread_during_the_step_then_refused_and_unanswered()
    {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let model = env.motion("thread");
        let terrain = slope();
        let start = state_at(4.0, 50.0, 12.0);

        let next = step(model.as_ref(), &start, &[], &terrain);

        assert_eq!(queries(), [(UMBRA_ERR_STATE, UNTOUCHED)]);
        assert_eq!(terrain.calls(), 0);
        assert_eq!(
            next, start,
            "the plugin wrote nothing: the pre-filled input state"
        );
        assert_eq!(env.faults(), Vec::new());
    }

    /// A validation host never steps a plugin motion model: no plugin call, no terrain query, the
    /// entity held (ASM-722 d).
    #[test]
    fn in_a_validation_host_a_plugin_motion_model_is_never_called_and_the_entity_is_held() {
        let env = RawEnv::load_in(
            forged!(raw_parts()),
            PluginFaultPolicy::Abort,
            HostMode::Validation,
        );
        let model = env.motion("query");
        let terrain = slope();
        let start = state_at(4.0, 50.0, 12.0);
        assert_ne!(start.velocity, Vec3::ZERO, "a held entity is visible");

        let next = step(model.as_ref(), &start, &[], &terrain);

        assert_eq!(queries(), Vec::new(), "no plugin call");
        assert_eq!(terrain.calls(), 0);
        assert_eq!(next.position, start.position);
        assert_eq!(next.orientation, start.orientation);
        assert_eq!(next.velocity, Vec3::ZERO, "held");
        assert_eq!(env.faults(), Vec::new());
    }

    /// Steps `mode` over the panicking provider and checks the coordinator's rule for a provider
    /// panic (ASM-722 b): the plugin's query gets `UMBRA_ERR_INTERNAL`, the panic resumes on the
    /// caller after the plugin call with the provider's own payload, it is logged at error level as
    /// the host's, and the plugin is not faulted (a later step on a sane terrain works).
    fn assert_provider_panic_resumes(mode: &str) {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let model = env.motion(mode);
        let start = state_at(4.0, 50.0, 12.0);

        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            step(model.as_ref(), &start, &[], &Panicking)
        }));

        let Err(payload) = caught else {
            panic!("the provider's panic must resume on the caller");
        };
        assert_eq!(
            payload.downcast_ref::<String>().map(String::as_str),
            Some(PROVIDER_PANIC),
            "the provider's own payload"
        );
        assert_eq!(queries(), [(UMBRA_ERR_INTERNAL, UNTOUCHED)], "{mode}");
        assert_eq!(env.faults(), Vec::new(), "the plugin is not blamed");
        assert!(env.host.take_abort().is_none());
        let errors: Vec<_> = env
            .log
            .all()
            .into_iter()
            .filter(|(_, level, m)| {
                *level == LogLevel::Error && m.contains("terrain provider panicked (host)")
            })
            .collect();
        assert_eq!(errors.len(), 1, "{:?}", env.log.all());

        let terrain = slope();
        let ground = terrain.height_at(start.position.to_geodetic()).0;
        let _ = step(model.as_ref(), &start, &[], &terrain);
        assert_eq!(queries()[1], (UMBRA_OK, ground), "the model still works");
        assert_eq!(env.faults(), Vec::new());
    }

    #[test]
    fn a_terrain_provider_panic_under_a_plugin_returning_ok_resumes_after_the_step_unblamed() {
        assert_provider_panic_resumes("query");
    }

    #[test]
    fn a_terrain_provider_panic_under_a_plugin_returning_the_error_resumes_after_the_step_unblamed()
    {
        assert_provider_panic_resumes("query_code");
    }
}
