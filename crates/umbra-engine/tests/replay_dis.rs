//! AT-9 (M7): the network row of replay. Two engines exchange entities over real DIS UDP on
//! `127.0.0.1` (the `dis_pair` scenarios through the real `dis` plugin), each records its run with
//! its inbound messages, and each recording replays with no sockets to the identical per-tick
//! hashes. Live re-runs differ in arrival ticks, which is why the recording is needed.
//! Requirements: REQ-K-7, REQ-K-4, REQ-N-1, REQ-N-3, REQ-N-6, ADR 0030, ADR 0053, ADR 0054,
//! ASM-627, ASM-628.
//!
//! Written in M7 WI-1 against the recorder and replay stubs; the rows are WI-21's evidence and are
//! ignored until then (flakiness runs: 10 sequential, 4 parallel). Ports come from binding
//! `127.0.0.1:0` (ASM-246). Each engine runs in real time on its own 8 MiB thread; no test sleeps
//! to synchronise.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod m7_support;
mod plugin_support;

use std::net::UdpSocket;
use std::path::{Path, PathBuf};

use m7_support::{
    Recorded, first_divergence, launcher_registry, plugin_lock, record_on, replay_file,
};
use umbra_engine::StdWallClock;
use umbra_record::{Reader, Record};

const TICKS: u64 = 150;

fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// A scratch copy of `dis_pair` with its two ports replaced by free ones; returns the A and B
/// scenario files and the ports.
fn scratch_pair() -> (PathBuf, PathBuf, [u16; 2]) {
    let root = plugin_support::scratch_copy(&plugin_support::dis_pair_example_dir(), "replay-dis");
    let ports = [free_port(), free_port()];
    for side in ["a", "b"] {
        let file = root.join(side).join("scenario.toml");
        let text = std::fs::read_to_string(&file).expect("read");
        let text = text
            .replace("127.0.0.1:47001", &format!("127.0.0.1:{}", ports[0]))
            .replace("127.0.0.1:47002", &format!("127.0.0.1:{}", ports[1]));
        std::fs::write(&file, text).expect("write");
    }
    (
        root.join("a/scenario.toml"),
        root.join("b/scenario.toml"),
        ports,
    )
}

/// Runs both sides in real time on their own threads, each with a recorder.
fn run_pair(plugins: &Path, a: &Path, b: &Path) -> (Recorded, Recorded) {
    let run = |scenario: PathBuf, plugins: PathBuf| {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                record_on(
                    &launcher_registry(),
                    &scenario,
                    &[plugins],
                    Some(TICKS),
                    None,
                    Box::new(StdWallClock::new()),
                )
            })
            .expect("spawn the node thread")
    };
    let (ta, tb) = (
        run(a.to_path_buf(), plugins.to_path_buf()),
        run(b.to_path_buf(), plugins.to_path_buf()),
    );
    (ta.join().expect("node a"), tb.join().expect("node b"))
}

fn inbound_ticks(file: &Path) -> Vec<u64> {
    let (_, reader) = Reader::open(file).expect("open");
    reader
        .map(|r| r.expect("record"))
        .filter_map(|r| match r {
            Record::Inbound(i) => Some(i.tick),
            _ => None,
        })
        .collect()
}

/// AT-9: each side of a live DIS pair replays to identical per-tick hashes without sockets: both
/// ports are held by the test while the replays run, so a replay that bound or sent anything would
/// fail.
#[test]
#[ignore = "M7 WI-21"]
fn each_side_of_a_live_dis_pair_replays_to_identical_hashes_without_sockets() {
    let _guard = plugin_lock();
    let plugins = plugin_support::dis();
    let (a, b, ports) = scratch_pair();
    let (live_a, live_b) = run_pair(&plugins, &a, &b);
    assert!(
        !inbound_ticks(&live_a.file).is_empty(),
        "side a received the peer's entities"
    );
    assert!(
        !inbound_ticks(&live_b.file).is_empty(),
        "side b received the peer's entities"
    );
    let held: Vec<UdpSocket> = ports
        .iter()
        .map(|p| UdpSocket::bind(("127.0.0.1", *p)).expect("the ports are free after the live run"))
        .collect();

    for (name, live) in [("a", &live_a), ("b", &live_b)] {
        let (replayed, summary) = replay_file(
            &launcher_registry(),
            &live.file,
            std::slice::from_ref(&plugins),
        )
        .unwrap_or_else(|e| panic!("replay of side {name}: {e}"));

        assert_eq!(
            first_divergence(&replayed, &live.hashes),
            None,
            "side {name}"
        );
        assert_eq!(summary.ticks, TICKS, "side {name}");
        assert_eq!(
            summary.replayed_adapters,
            ["dis".to_owned()],
            "side {name}: the DIS adapter was fed from the recording"
        );
    }
    drop(held);
}

/// AT-9, ADR 0030: live arrival ticks over UDP may differ from run to run, so a run is reproducible
/// only through its recording. Whatever the arrival ticks were, the recording of every live run
/// replays to that run's hashes (this test does not assert that two live runs differ).
#[test]
#[ignore = "M7 WI-21"]
fn every_live_run_of_the_pair_replays_to_its_own_hashes_whatever_the_arrival_ticks() {
    let _guard = plugin_lock();
    let plugins = plugin_support::dis();
    let (a, b, _) = scratch_pair();

    for round in 0..2 {
        let (live_a, live_b) = run_pair(&plugins, &a, &b);
        for (name, live) in [("a", &live_a), ("b", &live_b)] {
            let (replayed, _) = replay_file(
                &launcher_registry(),
                &live.file,
                std::slice::from_ref(&plugins),
            )
            .unwrap_or_else(|e| panic!("round {round}, side {name}: {e}"));

            assert_eq!(
                first_divergence(&replayed, &live.hashes),
                None,
                "round {round}, side {name}"
            );
        }
    }
}
