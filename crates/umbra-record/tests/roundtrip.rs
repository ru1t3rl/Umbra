//! M7 WI-11: every record kind round-trips bit-exactly through the writer and the reader
//! (REQ-K-7, ASM-621, ADR 0053).

#![forbid(unsafe_code)]

mod common;

use common::{every_record, header, read_bytes, write_bytes};
use umbra_core::net::{NetKey, NetMessage};
use umbra_core::{Ecef, StateHash};
use umbra_record::{InboundRecord, Record};

#[test]
fn given_every_record_kind_when_round_tripped_then_equal() {
    let records = every_record();

    let (got_header, got) = read_bytes(write_bytes(&header(), &records)).expect("read");

    assert_eq!(got_header, header());
    assert_eq!(got, records);
}

#[test]
fn given_a_round_trip_then_rewriting_gives_the_same_bytes() {
    let bytes = write_bytes(&header(), &every_record());

    let (h, records) = read_bytes(bytes.clone()).expect("read");

    assert_eq!(write_bytes(&h, &records), bytes);
}

#[test]
fn given_subnormals_negative_zero_and_extremes_then_f64_bits_survive() {
    let values = [
        5e-324,
        -5e-324,
        f64::MIN_POSITIVE / 2.0,
        f64::MIN_POSITIVE,
        -0.0,
        f64::MAX,
        f64::MIN,
        0.1 + 0.2,
        std::f64::consts::PI,
        1.0 / 3.0,
        6_378_137.000_000_01,
    ];
    let messages: Vec<NetMessage> = values
        .iter()
        .map(|&v| NetMessage::Detonation {
            shooter: None,
            target: None,
            position: Ecef(umbra_core::Vec3::new(v, -v, v * 0.5)),
        })
        .collect();
    let record = Record::Inbound(InboundRecord {
        tick: 1,
        adapter: "a".to_owned(),
        messages,
    });

    let (_, got) = read_bytes(write_bytes(&header(), std::slice::from_ref(&record))).expect("read");

    let Some(Record::Inbound(inbound)) = got.first() else {
        panic!("expected an inbound record, got {got:?}");
    };
    for (i, message) in inbound.messages.iter().enumerate() {
        let NetMessage::Detonation { position, .. } = message else {
            panic!("expected a detonation");
        };
        let v = values[i];
        assert_eq!(position.0.x.to_bits(), v.to_bits(), "x of {v:e}");
        assert_eq!(position.0.y.to_bits(), (-v).to_bits(), "y of {v:e}");
        assert_eq!(position.0.z.to_bits(), (v * 0.5).to_bits(), "z of {v:e}");
    }
}

#[test]
fn given_non_finite_floats_then_they_read_back_as_non_finite() {
    let record = Record::Inbound(InboundRecord {
        tick: 1,
        adapter: "a".to_owned(),
        messages: vec![NetMessage::Detonation {
            shooter: None,
            target: None,
            position: Ecef(umbra_core::Vec3::new(
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            )),
        }],
    });

    let (_, got) = read_bytes(write_bytes(&header(), &[record])).expect("read");

    let Some(Record::Inbound(inbound)) = got.first() else {
        panic!("expected an inbound record");
    };
    let Some(NetMessage::Detonation { position, .. }) = inbound.messages.first() else {
        panic!("expected a detonation");
    };
    assert!(position.0.x.is_nan());
    assert_eq!(position.0.y, f64::INFINITY);
    assert_eq!(position.0.z, f64::NEG_INFINITY);
}

#[test]
fn given_u64_max_keys_seed_and_hashes_then_they_are_exact() {
    let records = vec![
        Record::Inbound(InboundRecord {
            tick: u64::MAX,
            adapter: "a".to_owned(),
            messages: vec![NetMessage::EntityRemoved {
                key: NetKey(u64::MAX),
            }],
        }),
        Record::Hash {
            tick: u64::MAX,
            hash: StateHash(u64::MAX),
        },
    ];

    let (h, got) = read_bytes(write_bytes(&header(), &records)).expect("read");

    assert_eq!(h.seed, u64::MAX);
    assert_eq!(got, records);
}

#[test]
fn given_a_recording_then_each_line_is_one_json_object_ending_in_a_newline() {
    let bytes = write_bytes(&header(), &every_record());

    let text = String::from_utf8(bytes).expect("utf-8");

    assert!(text.ends_with('\n'));
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("json line");
        assert!(value.is_object());
    }
    assert!(text.starts_with(
        r#"{"type":"header","format":"umbra.recording","version":1,"encoding":"json","umbra":"0.1.0","scenario":"#
    ));
}

#[test]
fn given_each_dead_reckoning_model_then_the_written_name_is_the_documented_one() {
    let names = [
        "static",
        "velocity",
        "velocity_rotation",
        "acceleration_rotation",
        "acceleration",
        "body_velocity",
        "body_velocity_rotation",
        "body_acceleration_rotation",
        "body_acceleration",
    ];
    let messages: Vec<NetMessage> = common::MODELS
        .into_iter()
        .map(|m| NetMessage::EntityState(common::entity_state(m)))
        .collect();
    let record = Record::Inbound(InboundRecord {
        tick: 1,
        adapter: "a".to_owned(),
        messages,
    });

    let text = String::from_utf8(write_bytes(&header(), &[record])).expect("utf-8");

    let line: serde_json::Value =
        serde_json::from_str(text.lines().nth(1).expect("line")).expect("json");
    let written: Vec<&str> = line["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| m["dead_reckoning"].as_str().expect("name"))
        .collect();
    assert_eq!(written, names);
}
