//! Unit tests of the recorder's pieces (M7 WI-13).

use std::path::Path;

use umbra_core::net::NetMessage;
use umbra_core::{ControlAction, StateHash};
use umbra_record::{Record, RecordError};

use super::{push_split, relative_to};

fn messages(n: usize) -> Vec<NetMessage> {
    vec![NetMessage::SimulationControl(ControlAction::Start); n]
}

/// A pusher that refuses a record of more than `limit` messages like the writer refuses a long line,
/// and keeps the accepted ones.
fn limited(
    limit: usize,
    kept: &mut Vec<(u64, usize)>,
) -> impl FnMut(Record) -> Result<(), RecordError> {
    move |record| match record {
        Record::Inbound(i) if i.messages.len() > limit => {
            Err(RecordError::LineTooLong { bytes: 2, limit: 1 })
        }
        Record::Inbound(i) => {
            kept.push((i.tick, i.messages.len()));
            Ok(())
        }
        _ => Ok(()),
    }
}

/// ASM-727 i: a batch the format cannot hold in one line becomes several records of the same
/// tick that together hold every message once, in order.
#[test]
fn an_oversize_batch_is_split_into_records_of_the_same_tick() {
    let mut kept = Vec::new();

    push_split(&mut limited(3, &mut kept), 7, "lb", &messages(10)).expect("split");

    assert!(
        kept.iter()
            .all(|&(tick, len)| tick == 7 && (1..=3).contains(&len))
    );
    assert_eq!(kept.iter().map(|&(_, len)| len).sum::<usize>(), 10);
    assert!(kept.len() >= 4, "{kept:?}");
}

#[test]
fn a_batch_that_fits_is_one_record() {
    let mut kept = Vec::new();

    push_split(&mut limited(10, &mut kept), 1, "lb", &messages(10)).expect("push");

    assert_eq!(kept, [(1, 10)]);
}

/// A single message that does not fit cannot be split further: the error surfaces.
#[test]
fn a_single_oversize_message_is_an_error() {
    let mut kept = Vec::new();

    let result = push_split(&mut limited(0, &mut kept), 1, "lb", &messages(1));

    assert!(matches!(result, Err(RecordError::LineTooLong { .. })));
}

/// The order of messages across the split is the order polled.
#[test]
fn the_split_keeps_the_polled_order() {
    let batch: Vec<NetMessage> = (0..9)
        .map(|n| NetMessage::SimulationControl(ControlAction::Step(n)))
        .collect();
    let mut seen: Vec<NetMessage> = Vec::new();
    let mut push = |record: Record| match record {
        Record::Inbound(i) if i.messages.len() > 2 => {
            Err(RecordError::LineTooLong { bytes: 2, limit: 1 })
        }
        Record::Inbound(i) => {
            seen.extend(i.messages);
            Ok(())
        }
        _ => Ok(()),
    };

    push_split(&mut push, 3, "lb", &batch).expect("split");

    assert_eq!(seen, batch);
}

#[test]
fn a_path_is_made_relative_to_a_directory() {
    let rel = |p: &str, b: &str| relative_to(Path::new(p), Path::new(b));

    assert_eq!(rel("/a/b/c.toml", "/a/b"), Some("c.toml".into()));
    assert_eq!(rel("/a/b/c.toml", "/a/x/y"), Some("../../b/c.toml".into()));
    assert_eq!(rel("/a/b", "/a/b"), Some("".into()));
}

fn core() -> super::Core {
    super::Core {
        writer: None,
        state: super::RecordingState::Active,
        file: "unused".into(),
        version: umbra_core::StateHashVersion::V2,
        hash_every: 10,
        records: 0,
        last_tick: 0,
        failure: None,
        force_hash: false,
        hashed: None,
        boundary: None,
        next_seq: 0,
    }
}

fn test_header() -> umbra_record::Header {
    umbra_record::Header {
        umbra: "0".to_owned(),
        scenario: umbra_record::ScenarioRef {
            path: "s".into(),
            relative_path: None,
            files: Vec::new(),
        },
        seed: 1,
        tick_rate_hz: 30,
        tick_rate_source: "scenario".to_owned(),
        state_hash_version: 2,
        hash_every: 1,
        on_plugin_fault: "abort".to_owned(),
        plugins: Vec::new(),
        network_adapters: Vec::new(),
    }
}

/// ASM-626: `seq` counts commands within one boundary and restarts at the next.
#[test]
fn seq_counts_within_a_boundary_and_restarts() {
    let mut core = core();
    core.writer =
        Some(umbra_record::Writer::from_sink(Vec::new(), &test_header(), 8).expect("writer"));

    core.command(5, umbra_record::RecordedCommand::Start);
    assert_eq!((core.boundary, core.next_seq), (Some(5), 0));
    core.command(5, umbra_record::RecordedCommand::Pause);
    assert_eq!(core.next_seq, 1);
    core.command(6, umbra_record::RecordedCommand::Resume);
    assert_eq!((core.boundary, core.next_seq), (Some(6), 0));
}

/// A refused record moves neither the boundary nor `seq`.
#[test]
fn a_command_that_is_not_accepted_does_not_advance_the_sequence() {
    let mut core = core();

    core.command(5, umbra_record::RecordedCommand::Start);

    assert_eq!((core.boundary, core.next_seq), (None, 0));
}

/// ASM-624: the cadence hashes every `hash_every`-th tick; a forced hash is taken once.
#[test]
fn the_cadence_and_a_forced_hash_decide_when_a_hash_is_taken() {
    let mut core = core();
    let mut taken = Vec::new();
    let mut tick = |core: &mut super::Core, n: u64| {
        core.after_tick(n, |_| {
            taken.push(n);
            StateHash(n)
        });
    };
    tick(&mut core, 3);
    core.force_hash = true;
    tick(&mut core, 4);
    tick(&mut core, 5);
    tick(&mut core, 10);

    assert_eq!(taken, [4, 10]);
}

/// Minor 2: once the recording failed or finished, nothing moves `last_tick` or asks for a hash.
#[test]
fn a_failed_or_finished_recording_does_not_move() {
    for state in [
        super::RecordingState::Failed,
        super::RecordingState::Finished,
    ] {
        let mut core = core();
        core.state = state;
        core.failure = Some("broke".to_owned());

        core.command(9, umbra_record::RecordedCommand::Start);
        core.inbound(9, "lb", &messages(1));
        core.fault(9, "lb", "poll");
        core.hash(9, StateHash(1));

        assert_eq!((core.last_tick, core.force_hash), (0, false), "{state:?}");
        assert_eq!(core.records, 0);
    }
}

/// A sink that takes the header and then fails every write: the failure path without `/dev/full`.
struct FailsAfterTheHeader(bool);

impl std::io::Write for FailsAfterTheHeader {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if std::mem::replace(&mut self.0, true) {
            Err(std::io::Error::other("sink broke"))
        } else {
            Ok(buf.len())
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// ASM-625: a write error marks the recording failed and `finish` reports it, without touching the
/// simulation.
#[test]
fn a_failing_sink_marks_the_recording_failed() {
    let header = test_header();
    let mut core = core();
    core.writer =
        Some(umbra_record::Writer::from_sink(FailsAfterTheHeader(false), &header, 4).expect("w"));

    let finished = core.finish(3, "completed", |_| StateHash(7));

    assert!(matches!(finished, Err(RecordError::Failed(m)) if m.contains("sink broke")));
    assert_eq!(core.status().state, super::RecordingState::Failed);
}
