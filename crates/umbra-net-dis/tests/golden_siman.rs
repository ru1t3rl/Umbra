//! Start/Resume and Stop/Freeze golden packets, v7 and v6 (M6 WI-14, AT-7 simulation management
//! rows, AT-8). The fixtures come from `gen_warfare.py`; the expected values are written by hand.
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod support;

use support::{
    VERSIONS, assert_golden, assert_standard_malformed,
    assert_versions_differ_only_in_version_and_status, fixtures_with_prefix, load, set_length,
};
use umbra_net_dis::codec::{
    Body, ClockTime, DecodeError, EntityId, Header, Pdu, START_RESUME_LEN, STOP_FREEZE_LEN,
    StartResume, StopFreeze, decode, encode,
};
use umbra_net_dis::version::DisVersion;

const PREFIXES: [&str; 2] = ["start_resume_", "stop_freeze_"];

fn pdu(status: u8, body: Body) -> Pdu {
    Pdu {
        header: Header {
            exercise_id: 1,
            timestamp: 0x0102_0304,
            status,
        },
        body,
    }
}

fn start() -> StartResume {
    StartResume {
        originating_entity: EntityId {
            site: 1,
            application: 2,
            entity: 3,
        },
        receiving_entity: EntityId {
            site: 4,
            application: 5,
            entity: 6,
        },
        real_world_time: ClockTime {
            hour: 13,
            time_past_hour: 0x0102_0304,
        },
        simulation_time: ClockTime {
            hour: 7,
            time_past_hour: 0x0A0B_0C0D,
        },
        request_id: 0x1122_3344,
    }
}

fn stop() -> StopFreeze {
    StopFreeze {
        originating_entity: EntityId {
            site: 1,
            application: 2,
            entity: 3,
        },
        receiving_entity: EntityId {
            site: 4,
            application: 5,
            entity: 6,
        },
        real_world_time: ClockTime {
            hour: 13,
            time_past_hour: 0x0102_0304,
        },
        reason: 3,
        frozen_behavior: 6,
        request_id: 0x1122_3344,
    }
}

fn expected(name: &str, version: DisVersion) -> Option<Pdu> {
    let (status, body) = match name {
        "start_resume_basic" => (0x06, Body::StartResume(start())),
        "stop_freeze_basic" => (0x01, Body::StopFreeze(stop())),
        "stop_freeze_recess" => (
            0x01,
            Body::StopFreeze(StopFreeze {
                reason: 2,
                ..stop()
            }),
        ),
        _ => return None,
    };
    Some(pdu(
        if version == DisVersion::V7 { status } else { 0 },
        body,
    ))
}

#[test]
fn every_simulation_management_file_has_an_expectation() {
    for version in VERSIONS {
        let names = fixtures_with_prefix(version, &PREFIXES);
        assert_eq!(names.len(), 3, "{version:?}");
        for name in names {
            assert!(
                expected(&name, version).is_some(),
                "no expectation for {version:?} {name}"
            );
        }
    }
}

#[test]
fn golden_decode_encode_and_round_trip() {
    for version in VERSIONS {
        for name in fixtures_with_prefix(version, &PREFIXES) {
            assert_golden(
                version,
                &name,
                &expected(&name, version).expect("expected"),
                false,
            );
        }
    }
}

#[test]
fn lengths_are_44_and_40() {
    for version in VERSIONS {
        assert_eq!(load(version, "start_resume_basic").len(), START_RESUME_LEN);
        assert_eq!(load(version, "stop_freeze_basic").len(), STOP_FREEZE_LEN);
    }
}

#[test]
fn v6_and_v7_differ_only_in_the_version_byte() {
    for name in fixtures_with_prefix(DisVersion::V7, &PREFIXES) {
        assert_versions_differ_only_in_version_and_status(&name);
    }
}

#[test]
fn malformed_rows() {
    for version in VERSIONS {
        for name in ["start_resume_basic", "stop_freeze_basic"] {
            assert_standard_malformed(&format!("{version:?} {name}"), &load(version, name));
        }
    }
}

#[test]
fn a_consistent_wrong_length_is_bad_length() {
    for (name, pdu_type) in [("start_resume_basic", 13u8), ("stop_freeze_basic", 14)] {
        let valid = load(DisVersion::V7, name).len();
        for length in [12, valid - 1, valid + 1, valid + 16] {
            let mut bytes = load(DisVersion::V7, name);
            bytes.resize(length, 0);
            let declared = u16::try_from(length).expect("small");
            set_length(&mut bytes, declared);
            assert_eq!(
                decode(&bytes),
                Err(DecodeError::BadLength {
                    pdu_type,
                    length: declared
                })
            );
        }
    }
}

#[test]
fn stop_freeze_padding_is_ignored_on_decode_and_zero_on_encode() {
    let mut bytes = load(DisVersion::V7, "stop_freeze_basic");
    bytes[34] = 0xAB;
    bytes[35] = 0xCD;
    assert_eq!(
        decode(&bytes).map(|(_, p)| p),
        Ok(expected("stop_freeze_basic", DisVersion::V7).expect("expected"))
    );
    let mut out = Vec::new();
    encode(
        &expected("stop_freeze_basic", DisVersion::V7).expect("expected"),
        DisVersion::V7,
        &mut out,
    )
    .expect("encode");
    assert_eq!(&out[32..36], &[3, 6, 0, 0]);
}

#[test]
fn types_13_and_14_belong_to_family_5() {
    for (name, pdu_type) in [("start_resume_basic", 13u8), ("stop_freeze_basic", 14)] {
        let bytes = load(DisVersion::V6, name);
        assert_eq!((bytes[2], bytes[3]), (pdu_type, 5));
    }
}
