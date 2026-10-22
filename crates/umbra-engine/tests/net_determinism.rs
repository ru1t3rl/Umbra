//! AT-14 (gate 4): two engines over one loopback hub, stepped, hash alike on every tick of both
//! engines across runs, across every clock mode, and with a mid-run pause. Requirements: REQ-K-4,
//! D-5, ADR 0030, ASM-224, ASM-240. The first divergent tick names the engine.
//!
//! Step 4 of M6, passing since WI-9. WI-12 (ASM-410) adds several plugin network adapters at once:
//! engine A runs the loopback `lb` plus two SDK-built plugin adapters from distinct plugin images,
//! `left` (`test.network.net_ok` of `test.plugin.fixture`) and `right` (`test.network.peer_ok` of
//! `test.plugin.net_peer`), both mirrors (`umbra-test-plugin/src/network.rs`); engine B runs `lb`.

#![forbid(unsafe_code)]

mod net_support;
mod plugin_support;

use std::path::PathBuf;

use umbra_core::StateHash;
use umbra_engine::{ClockMode, TimeScale};
use umbra_net::LoopbackHub;

use net_support::{Peer, distinct, is_remote_from, registry_with, start, start_with_plugins};

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
fn two_engines_over_loopback_are_identical_with_a_mid_run_pause() {
    let (baseline, _) = run(None, false);

    let (paused, _) = run(None, true);

    assert_same("with pause", &paused, &baseline);
}

// ------------------------------------------------- several plugin adapters at once (WI-12, ASM-410)

/// One plugin directory with the fixture (feature `network`) and the peer (feature `network_peer`),
/// two distinct plugin images.
fn plugin_dirs() -> Vec<PathBuf> {
    let fixture = plugin_support::library("umbra-test-plugin", &["network"]);
    let peer = plugin_support::library("umbra-test-plugin", &["network_peer"]);
    vec![plugin_support::plugins_dir(&[
        ("fixture", &fixture),
        ("peer", &peer),
    ])]
}

/// What a plugin run reports: both traces, A's adapter statuses after `MID` ticks and at the end,
/// and the sources of each engine's ghosts at the end.
struct PluginRun {
    trace: Trace,
    mid: Vec<umbra_net::AdapterStatus>,
    end: Vec<umbra_net::AdapterStatus>,
    ghosts_a: Vec<&'static str>,
    ghosts_b: Vec<&'static str>,
}

const MID: u64 = 200;

/// The adapter names of the ghosts of `p`, sorted.
fn ghost_sources(p: &Peer) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = p
        .ghosts()
        .iter()
        .map(|g| {
            ["lb", "left", "right"]
                .into_iter()
                .find(|s| is_remote_from(g, s))
                .unwrap_or("other")
        })
        .collect();
    out.sort_unstable();
    out
}

#[expect(clippy::expect_used, reason = "test helper")]
/// Engine A on `net_pair/<dir>` (loopback plus the two plugin adapters) and engine B on `b/` over
/// one loopback hub, stepped alternately for `ROUNDS` ticks.
fn run_plugins(dir: &str) -> PluginRun {
    let hub = LoopbackHub::with_capture(4096);
    let (mut a, _) = Peer::new(start_with_plugins(
        dir,
        &registry_with(&[("test.net.loopback", &hub, 1000)]),
        &plugin_dirs(),
        None,
    ));
    let (mut b, _) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub, 2000)]),
        None,
    ));
    let status = |p: &Peer| p.engine.network().expect("link").status();
    let mut mid = Vec::new();
    for n in 1..=ROUNDS {
        a.tick();
        b.tick();
        if n == MID {
            mid = status(&a);
        }
    }
    PluginRun {
        end: status(&a),
        mid,
        ghosts_a: ghost_sources(&a),
        ghosts_b: ghost_sources(&b),
        trace: (a.hashes, b.hashes),
    }
}

fn named<'a>(s: &'a [umbra_net::AdapterStatus], name: &str) -> &'a umbra_net::AdapterStatus {
    s.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no adapter {name}"))
}

#[test]
fn two_plugin_network_adapters_and_the_loopback_at_once_are_identical_across_runs() {
    let first = run_plugins("plugins_a");
    let second = run_plugins("plugins_a");

    assert_eq!(first.trace.0.len() as u64, ROUNDS);
    assert!(
        distinct(&first.trace.0) > 100,
        "the hash changes as the simulation runs"
    );
    assert_eq!(
        first.ghosts_a,
        ["lb", "left", "right"],
        "A mirrors B's tank over the loopback and its own tank through each plugin adapter"
    );
    assert_eq!(
        first.ghosts_b,
        ["lb"],
        "B sees only A's own tank: A republishes no ghost (ASM-410, REQ-K-5)"
    );
    for name in ["lb", "left", "right"] {
        let s = named(&first.end, name);
        assert!(!s.faulted && s.received > 0, "{name}: {s:?}");
    }
    assert_same("plugins run 2 vs run 1", &second.trace, &first.trace);
}

#[test]
fn a_faulting_plugin_adapter_under_disable_capability_leaves_the_others_running() {
    let first = run_plugins("plugins_fault");
    let second = run_plugins("plugins_fault");

    assert_eq!(
        first.trace.0.len() as u64,
        ROUNDS,
        "the run went on to the end"
    );
    let right = named(&first.end, "right");
    assert!(right.faulted && !right.enabled, "{right:?}");
    assert_eq!(
        first.ghosts_a,
        ["lb", "left"],
        "the faulted adapter's ghost timed out; the others' stay"
    );
    for name in ["lb", "left"] {
        let (mid, end) = (named(&first.mid, name), named(&first.end, name));
        assert!(!end.faulted, "{name}: {end:?}");
        assert!(
            end.received > mid.received,
            "{name} kept receiving after the fault ({} -> {})",
            mid.received,
            end.received
        );
    }
    assert_same("fault run 2 vs run 1", &second.trace, &first.trace);
}
