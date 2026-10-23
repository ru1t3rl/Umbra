//! AT-16 (M7 WI-2, sensor rows): the host bridge of the sensor kind (ABI 0.2, ADR 0050; REQ-P-4,
//! REQ-P-5, REQ-B-5, REQ-K-4, ASM-612) through the SDK-built fixture plugin `test.plugin.kinds`: a
//! plugin sensor sees the candidates in the host's order with their pre-tick state, terrain line of
//! sight equals the provider's, and its draws advance the observer's own stream exactly like a
//! built-in's. The fault rows (a contact naming a non-candidate, a panic) are in `plugin_failures.rs`
//! (gate 13).
//!
//! Step 4 of M7 (WI-2), un-ignored with the bridge in WI-8; the raw-ABI rows (module `raw_rows`) are
//! WI-8's, see `motion_bridge.rs`.

#![expect(
    unsafe_code,
    reason = "the raw-ABI rows (WI-8) publish a forged plugin with `forged!`"
)]

#[macro_use]
mod support;

use support::kinds::{Wall, env, flat, ids, sense, stream, world};
use umbra_core::TerrainProvider;

#[test]
fn given_the_all_sensor_when_sensing_then_every_candidate_is_a_contact_with_its_pre_tick_state() {
    let env = env();
    let sensor = env.sensor("test.sensor.all", "{}").expect("build");
    let world = world();

    let (contacts, _) = sense(sensor.as_ref(), &world, &flat());

    assert_eq!(ids(&contacts), [2, 3, 4]);
    for (contact, (id, state)) in contacts.iter().zip(&world[1..]) {
        assert_eq!(contact.entity, *id);
        assert_eq!(contact.position, state.position);
        assert_eq!(contact.velocity, state.velocity);
        assert_eq!(
            contact.range.0,
            (state.position - world[0].1.position).norm()
        );
    }
    assert_eq!(env.host.faults(), Vec::new());
}

/// REQ-C-6, ADR 0050: line of sight through the service equals the provider's (a wall between the
/// observer and the farthest candidate blocks only that one).
#[test]
fn given_a_wall_when_the_los_sensor_senses_then_only_visible_candidates_are_contacts() {
    let env = env();
    let sensor = env.sensor("test.sensor.los", "{}").expect("build");
    let world = world();
    let wall = Wall {
        lon_lo: 4.0025_f64.to_radians(),
        lon_hi: 4.0027_f64.to_radians(),
    };
    let want: Vec<u64> = world[1..]
        .iter()
        .filter(|(_, s)| wall.line_of_sight(world[0].1.position, s.position))
        .map(|(id, _)| id.get())
        .collect();

    let (contacts, _) = sense(sensor.as_ref(), &world, &wall);

    assert_eq!(want, [2, 3], "the wall hides the candidate beyond it");
    assert_eq!(ids(&contacts), want);
}

/// REQ-K-4, ADR 0050: the draws go through the observer's own stream: the contacts are the candidates
/// whose draw is below 0.5, in candidate order, and the stream ends where the reference stream ends.
#[test]
fn given_the_coin_sensor_when_sensing_then_it_draws_from_the_observers_stream_in_candidate_order() {
    let env = env();
    let sensor = env.sensor("test.sensor.coin", "{}").expect("build");
    let world = world();
    let mut reference = stream(1);
    let want: Vec<u64> = world[1..]
        .iter()
        .filter(|_| reference.next_f64() < 0.5)
        .map(|(id, _)| id.get())
        .collect();

    let (contacts, after) = sense(sensor.as_ref(), &world, &flat());

    assert_eq!(ids(&contacts), want);
    assert_eq!(after.state(), reference.state());
}

/// WI-8: the bridged sensor is `Send + Sync` like every core sensor: it senses from two threads at
/// once (the calls serialise on the image's call lock, each with its own record, RNG and terrain
/// tokens), equal to a call on this thread, and is dropped last on the other thread, where `destroy`
/// runs without an error.
#[test]
fn given_a_plugin_sensor_shared_with_another_thread_then_it_senses_there_and_drops_last_there() {
    let env = env();
    let sensor = env.sensor("test.sensor.los", "{}").expect("build");
    let world = world();
    let (want, _) = sense(sensor.as_ref(), &world, &flat());
    let other = std::sync::Arc::clone(&sensor);
    let theirs = world.clone();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let got: Vec<_> = (0..50)
            .map(|_| sense(other.as_ref(), &theirs, &flat()).0)
            .collect();
        released.recv().expect("the main thread dropped its sensor");
        drop(other);
        got
    });

    let here: Vec<_> = (0..50)
        .map(|_| sense(sensor.as_ref(), &world, &flat()).0)
        .collect();
    drop(sensor);
    release.send(()).expect("the worker waits");
    let there = worker.join().expect("the worker did not panic");

    assert_eq!(ids(&want), [2, 3, 4]);
    assert!(here.iter().chain(&there).all(|c| *c == want));
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
// ADR 0050, ADR 0019 RR-M2: the terrain token of a `sense` call is valid only in that call, and only as
// a terrain token. The raw fixture returns `UMBRA_OK` whatever the host answered, so a fault below
// comes from the host's own call record.

mod raw_rows {
    use super::support::kinds::{
        Counting, PROVIDER_PANIC, Panicking, RAW_SENSOR, RawEnv, UNTOUCHED, at, queries, raw_parts,
        sense, state_at, step, stream, world,
    };
    use umbra_core::PluginFaultPolicy;
    use umbra_plugin_api::{UMBRA_ERR_INTERNAL, UMBRA_ERR_STATE, UMBRA_OK};
    use umbra_plugin_host::{HostMode, LogLevel};

    fn slope() -> Counting {
        Counting {
            slope: 1000.0,
            ..Counting::default()
        }
    }

    fn assert_sensor_token_fault(env: &RawEnv) {
        let faults = env.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!((faults[0].0.as_str(), faults[0].1), (RAW_SENSOR, "sense"));
        assert!(
            faults[0].2.contains("terrain") && faults[0].2.contains("stale or foreign"),
            "{}",
            faults[0].2
        );
    }

    /// Another service's token of the same call (the contact sink's) is not a terrain token.
    #[test]
    fn given_the_contact_sink_token_used_as_a_terrain_token_then_refused_and_the_sensor_faults() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let sensor = env.sensor("sink_token");
        let terrain = slope();

        let (contacts, _) = sense(sensor.as_ref(), &world(), &terrain);

        assert_eq!(contacts, Vec::new());
        assert_eq!(queries(), [(UMBRA_ERR_STATE, UNTOUCHED)]);
        assert_eq!(terrain.calls(), 0);
        assert_sensor_token_fault(&env);
    }

    /// A motion step's terrain token (another capability's finished call) used in a sense call.
    #[test]
    fn given_a_motion_steps_terrain_token_when_used_in_a_sense_call_then_refused_and_the_sensor_faults()
     {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let keeper = env.motion("keep");
        let stale = env.sensor("stale");
        let terrain = slope();

        let _ = step(keeper.as_ref(), &state_at(4.0, 50.0, 0.0), &[], &terrain);
        let (contacts, _) = sense(stale.as_ref(), &world(), &terrain);

        assert_eq!(contacts, Vec::new());
        assert_eq!(queries().len(), 2);
        assert_eq!(queries()[0].0, UMBRA_OK);
        assert_eq!(queries()[1], (UMBRA_ERR_STATE, UNTOUCHED));
        assert_eq!(terrain.calls(), 1);
        assert_sensor_token_fault(&env);
    }

    /// ADR 0050, ASM-615: the contacts of a faulting call are discarded whole, also those the sink
    /// accepted before the fault.
    #[test]
    fn given_an_accepted_contact_then_a_failing_sense_then_the_call_yields_no_contacts() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let sensor = env.sensor("push_then_fail");

        let (contacts, _) = sense(sensor.as_ref(), &world(), &slope());

        assert_eq!(
            queries(),
            [(UMBRA_OK, 0.0)],
            "the sink accepted the contact"
        );
        assert_eq!(contacts, Vec::new());
        let faults = env.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!((faults[0].0.as_str(), faults[0].1), (RAW_SENSOR, "sense"));
        assert!(
            faults[0].2.contains("UMBRA_ERR_INTERNAL"),
            "{}",
            faults[0].2
        );

        let (again, _) = sense(sensor.as_ref(), &world(), &slope());

        assert_eq!(again, Vec::new());
        assert_eq!(
            queries().len(),
            1,
            "a faulted capability is never called again"
        );
        assert_eq!(env.faults().len(), 1);
    }

    /// ADR 0050: the sense call carries the tick and rate of the core context, the observer as owned
    /// and every candidate in the kernel's order with its ownership (a remote one included).
    #[test]
    fn a_sense_call_carries_the_time_the_observer_and_the_candidates_in_order() {
        use umbra_core::{ContactBuffer, Observer, Ownership, RemoteSource, SenseContext, Target};
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let sensor = env.sensor("record");
        let world = world();
        let owned = Ownership::Owned;
        let remote = Ownership::Remote {
            source: RemoteSource::new("dis"),
        };
        let candidates: Vec<Target<'_>> = world[1..]
            .iter()
            .map(|(id, state)| {
                let ownership = if id.get() == 3 { &remote } else { &owned };
                Target::new(*id, state, None, ownership)
            })
            .collect();
        let mut rng = stream(1);
        let terrain = slope();
        let mut found = Vec::new();

        sensor.sense(
            &mut SenseContext::new(at(5), &mut rng, &terrain),
            &Observer::new(world[0].0, &world[0].1, None),
            &candidates,
            &mut ContactBuffer::new(&mut found),
        );

        assert_eq!(found, Vec::new());
        let code = |c: u32| i32::try_from(c).expect("small");
        let (o, r) = (
            code(umbra_plugin_api::UMBRA_OWNERSHIP_OWNED),
            code(umbra_plugin_api::UMBRA_OWNERSHIP_REMOTE),
        );
        assert_eq!(
            queries(),
            [(5, 30.0), (o, 1.0), (o, 2.0), (r, 3.0), (o, 4.0)]
        );
        assert_eq!(env.faults(), Vec::new());
    }

    #[test]
    fn given_a_terrain_token_used_from_another_thread_during_sense_then_refused_and_unanswered() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let sensor = env.sensor("thread");
        let terrain = slope();

        let (contacts, _) = sense(sensor.as_ref(), &world(), &terrain);

        assert_eq!(contacts, Vec::new());
        assert_eq!(queries(), [(UMBRA_ERR_STATE, UNTOUCHED)]);
        assert_eq!(terrain.calls(), 0);
        assert_eq!(env.faults(), Vec::new());
    }

    /// A validation host never calls a plugin sensor: no plugin call, no contacts (ASM-722 d).
    #[test]
    fn in_a_validation_host_a_plugin_sensor_is_never_called_and_yields_no_contacts() {
        let env = RawEnv::load_in(
            forged!(raw_parts()),
            PluginFaultPolicy::Abort,
            HostMode::Validation,
        );
        let sensor = env.sensor("push_then_fail");
        let terrain = slope();

        let (contacts, _) = sense(sensor.as_ref(), &world(), &terrain);

        assert_eq!(queries(), Vec::new(), "no plugin call (it would push)");
        assert_eq!(contacts, Vec::new());
        assert_eq!(terrain.calls(), 0);
        assert_eq!(env.faults(), Vec::new());
    }

    /// ASM-722 (b): a provider panic during `sense` resumes on the caller after the plugin call with
    /// the provider's payload, logged as the host's; the sensor is not faulted and still senses.
    #[test]
    fn a_terrain_provider_panic_during_sense_resumes_after_the_call_unblamed() {
        let env = RawEnv::load(forged!(raw_parts()), PluginFaultPolicy::Abort);
        let sensor = env.sensor("query_code");
        let world = world();

        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sense(sensor.as_ref(), &world, &Panicking)
        }));

        let Err(payload) = caught else {
            panic!("the provider's panic must resume on the caller");
        };
        assert_eq!(
            payload.downcast_ref::<String>().map(String::as_str),
            Some(PROVIDER_PANIC)
        );
        assert_eq!(queries(), [(UMBRA_ERR_INTERNAL, UNTOUCHED)]);
        assert_eq!(env.faults(), Vec::new(), "the plugin is not blamed");
        assert!(env.log.all().iter().any(|(_, level, m)| {
            *level == LogLevel::Error && m.contains("terrain provider panicked (host)")
        }));

        let (contacts, _) = sense(sensor.as_ref(), &world, &slope());
        assert_eq!(contacts, Vec::new());
        assert_eq!(queries()[1].0, UMBRA_OK, "the sensor still works");
        assert_eq!(env.faults(), Vec::new());
    }
}
