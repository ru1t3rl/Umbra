//! M7 WI-1: the `umbra-record` skeleton builds and pins the format identity (ADR 0053), and the
//! behaviors that WI-11 implements are named here as ignored acceptance tests (REQ-K-7, ASM-621,
//! ASM-625). The full round-trip, property and error suites are written in WI-11.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::path::PathBuf;

use umbra_core::StateHash;
use umbra_record::{
    DEFAULT_QUEUE, EndRecord, FORMAT, FORMAT_VERSION, Header, Reader, Record, RecordError,
    ScenarioRef, Writer,
};

fn header() -> Header {
    Header {
        umbra: "0.1.0".to_owned(),
        scenario: ScenarioRef {
            path: PathBuf::from("/s/scenario.toml"),
            relative_path: None,
            files: Vec::new(),
        },
        seed: u64::MAX,
        tick_rate_hz: 30,
        tick_rate_source: "scenario".to_owned(),
        state_hash_version: 2,
        hash_every: 1,
        on_plugin_fault: "disable_capability".to_owned(),
        plugins: Vec::new(),
        network_adapters: Vec::new(),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("umbra-record-format");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

/// ADR 0053: the format identity is part of the file contract.
#[test]
fn given_the_crate_then_format_identity_is_umbra_recording_version_1() {
    assert_eq!(FORMAT, "umbra.recording");
    assert_eq!(FORMAT_VERSION, 1);
    assert_eq!(DEFAULT_QUEUE, 4096);
}

/// ASM-621: a written recording reads back bit-exactly (header with a `u64::MAX` seed, a hash and
/// an end record).
#[test]
#[ignore = "M7 WI-11"]
fn given_a_written_recording_when_read_then_header_and_records_are_equal() {
    let path = scratch("roundtrip.umbrarec.jsonl");
    let records = vec![
        Record::Hash {
            tick: 1,
            hash: StateHash(7),
        },
        Record::End(EndRecord {
            tick: 1,
            reason: "completed".to_owned(),
            hash: StateHash(7),
        }),
    ];
    let writer = Writer::create(&path, &header(), DEFAULT_QUEUE).expect("create");
    for r in &records {
        writer.push(r.clone()).expect("push");
    }
    writer.finish().expect("finish");

    let (got_header, reader) = Reader::open(&path).expect("open");
    let got: Result<Vec<Record>, RecordError> = reader.collect();

    assert_eq!(got_header, header());
    assert_eq!(got.expect("records"), records);
}

/// ASM-621: two recordings of the same inputs are byte-identical.
#[test]
#[ignore = "M7 WI-11"]
fn given_the_same_inputs_when_written_twice_then_files_are_byte_identical() {
    let write = |name: &str| {
        let path = scratch(name);
        let writer = Writer::create(&path, &header(), DEFAULT_QUEUE).expect("create");
        writer
            .push(Record::Hash {
                tick: 1,
                hash: StateHash(9),
            })
            .expect("push");
        writer.finish().expect("finish");
        std::fs::read(path).expect("read")
    };

    assert_eq!(write("a.umbrarec.jsonl"), write("b.umbrarec.jsonl"));
}
