//! AT-14 (gate 4): two engines over one loopback hub, stepped, hash alike on every tick of both
//! engines across runs, across every clock mode, and with a mid-run pause. Requirements: REQ-K-4,
//! D-5, ADR 0030, ASM-224, ASM-240. The first divergent tick names the engine.
//!
//! Step 4 of M6: written before WI-9; ignored with it (`Engine::attach_network` is a stub).

#![forbid(unsafe_code)]

mod net_support;

use umbra_core::StateHash;
use umbra_engine::{ClockMode, TimeScale};
use umbra_net::LoopbackHub;

use net_support::{Peer, distinct, registry_with, start};

const ROUNDS: u64 = 600;
const PAUSE_AT: u64 = 100;

type Trace = (Vec<StateHash>, Vec<StateHash>);

#[expect(clippy::expect_used, reason = "test helper")]
/// Both engines' adapter counters and the number of messages the hub has seen.
fn network_state(
    a: &Peer,
    b: &Peer,
    hub: &std::sync::Arc<LoopbackHub>,
) -> (
    Vec<umbra_net::AdapterStatus>,
    Vec<umbra_net::AdapterStatus>,
    usize,
) {
    let status = |p: &Peer| p.engine.network().expect("link").status();
    (status(a), status(b), hub.captured().len())
}

fn run(clock: Option<ClockMode>, pause: bool) -> (Trace, usize) {
    let hub = LoopbackHub::with_capture(4096);
    let (mut a, _) = Peer::new(start(
        "a",
        &registry_with(&[("test.net.loopback", &hub, 1000)]),
        clock,
    ));
    let (mut b, _) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub, 2000)]),
        clock,
    ));
    for n in 0..ROUNDS {
        if pause && n == PAUSE_AT {
            a.engine.pause();
            b.engine.pause();
            let before = network_state(&a, &b, &hub);
            a.tick();
            b.tick();
            assert_eq!(
                network_state(&a, &b, &hub),
                before,
                "nothing is polled or published while paused (ASM-240)"
            );
            assert_eq!(a.hashes.len() as u64, PAUSE_AT, "no tick while paused");
            assert_eq!(b.hashes.len() as u64, PAUSE_AT, "no tick while paused");
            a.engine.resume();
            b.engine.resume();
        }
        a.tick();
        b.tick();
    }
    let ghosts = a.ghosts().len() + b.ghosts().len();
    ((a.hashes, b.hashes), ghosts)
}

fn assert_same(label: &str, got: &Trace, want: &Trace) {
    for (name, g, w) in [("A", &got.0, &want.0), ("B", &got.1, &want.1)] {
        assert_eq!(g.len(), w.len(), "{label}: engine {name} tick count");
        if let Some(i) = g.iter().zip(w).position(|(x, y)| x != y) {
            panic!(
                "{label}: engine {name} first diverges at tick {} ({} vs {})",
                i + 1,
                g[i],
                w[i]
            );
        }
    }
}

#[test]
#[ignore = "WI-9"]
fn two_engines_over_loopback_are_identical_across_runs() {
    let (first, ghosts) = run(None, false);
    let (second, _) = run(None, false);

    assert_eq!(
        ghosts, 2,
        "each engine mirrors the other's tank, so the network took part"
    );
    assert!(
        distinct(&first.0) > 100,
        "the hash changes as the simulation runs"
    );
    assert_same("run 2 vs run 1", &second, &first);
}

#[test]
#[ignore = "WI-9"]
fn two_engines_over_loopback_are_identical_in_every_clock_mode() {
    let (baseline, _) = run(Some(ClockMode::Stepped), false);
    let modes = [
        ("afap", ClockMode::AsFastAsPossible),
        ("realtime", ClockMode::RealTime),
        (
            "scaled 4x",
            ClockMode::Scaled(TimeScale::new(4.0).expect("valid scale")),
        ),
    ];

    for (label, mode) in modes {
        let (trace, _) = run(Some(mode), false);

        assert_same(label, &trace, &baseline);
    }
}

#[test]
#[ignore = "WI-9"]
fn two_engines_over_loopback_are_identical_with_a_mid_run_pause() {
    let (baseline, _) = run(None, false);

    let (paused, _) = run(None, true);

    assert_same("with pause", &paused, &baseline);
}
