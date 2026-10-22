//! Entity State golden packets, v7 and v6 (M6 WI-13, AT-7 Entity State rows, AT-8; REQ-N-6, NFR-2).
//!
//! The fixtures under `tests/fixtures/dis/` were built outside this codec (`gen_es.py`, see the
//! README there); the expected values below are written by hand, not read from the files.

#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![forbid(unsafe_code)]

use umbra_net_dis::codec::{
    APPEARANCE_DEACTIVATED, Body, DeadReckoningAlgorithm, DeadReckoningParameters, DecodeError,
    ENTITY_STATE_LEN, EntityId, EntityState, EntityType, HEADER_LEN, Header, Marking, Pdu, decode,
    encode,
};
mod support;
use support::{VERSIONS, assert_standard_malformed, fixtures_with_prefix, load};
use umbra_net_dis::version::{DisVersion, HeaderStrategy, V6Header, V7Header};

fn type_225() -> EntityType {
    EntityType {
        kind: 1,
        domain: 2,
        country: 225,
        category: 3,
        subcategory: 4,
        specific: 5,
        extra: 6,
    }
}

fn padded(text: &[u8]) -> [u8; 11] {
    let mut chars = [0u8; 11];
    chars[..text.len()].copy_from_slice(text);
    chars
}

/// The value of `es_basic` as the standard's tables give it (hand-written, mirrors `gen_es.py`).
fn basic() -> Pdu {
    Pdu {
        header: Header {
            exercise_id: 1,
            timestamp: 0x0102_0304,
            status: 0,
        },
        body: Body::EntityState(EntityState {
            entity_id: EntityId {
                site: 1,
                application: 2,
                entity: 3,
            },
            force_id: 1,
            entity_type: type_225(),
            alternative_entity_type: EntityType {
                kind: 1,
                domain: 2,
                country: 222,
                category: 7,
                subcategory: 8,
                specific: 9,
                extra: 10,
            },
            linear_velocity: [10.5, -2.25, 0.75],
            location: [3_900_000.5, 1_100_000.25, 4_800_000.125],
            orientation: [0.5, 0.25, -0.125],
            appearance: 0x0021_B2C3,
            dead_reckoning: DeadReckoningParameters {
                algorithm: 2,
                other: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                linear_acceleration: [0.5, 1.5, -0.25],
                angular_velocity: [0.375, 0.0625, 0.125],
            },
            marking: Marking {
                character_set: 1,
                characters: padded(b"T72-01"),
            },
            capabilities: 0x0A0B_0C0D,
        }),
    }
}

fn with_state(f: impl FnOnce(&mut EntityState)) -> Pdu {
    let mut pdu = basic();
    match &mut pdu.body {
        Body::EntityState(es) => f(es),
        _ => unreachable!("basic() is an Entity State"),
    }
    pdu
}

fn state(pdu: Pdu) -> EntityState {
    match pdu.body {
        Body::EntityState(es) => es,
        other => panic!("not an Entity State: {other:?}"),
    }
}

/// Expected value of a fixture, `None` for a name nobody wrote an expectation for.
fn expected(name: &str, version: DisVersion) -> Option<Pdu> {
    let mut pdu = match name {
        "es_basic" | "es_articulated" => basic(),
        "es_status" => {
            let mut p = basic();
            p.header.status = 0x0B;
            p
        }
        "es_deactivated" => with_state(|es| es.appearance = 0x00A1_B2C3),
        "es_zero_type" => with_state(|es| {
            es.entity_type = EntityType::default();
            es.alternative_entity_type = EntityType::default();
        }),
        "es_marking_full" => with_state(|es| es.marking.characters = *b"ABCDEFGHIJK"),
        _ => {
            let code: u8 = name.strip_prefix("es_drm_")?.parse().ok()?;
            with_state(|es| es.dead_reckoning.algorithm = code)
        }
    };
    if version == DisVersion::V6 {
        pdu.header.status = 0; // v6 has no status field
    }
    Some(pdu)
}

fn fixtures(version: DisVersion) -> Vec<String> {
    fixtures_with_prefix(version, &["es_"])
}

#[test]
fn every_golden_file_is_covered_by_a_test() {
    for version in VERSIONS {
        let names = fixtures(version);
        // 6 named edge cases (v7 has es_status too) plus the ten DRM codes.
        assert_eq!(
            names.len(),
            if version == DisVersion::V7 { 16 } else { 15 },
            "{version:?}"
        );
        for name in &names {
            assert!(
                expected(name, version).is_some(),
                "no expectation for {version:?} {name}"
            );
        }
    }
}

#[test]
fn golden_decode_matches_the_expected_value() {
    for version in VERSIONS {
        for name in fixtures(version) {
            let bytes = load(version, &name);
            let (got_version, pdu) =
                decode(&bytes).unwrap_or_else(|e| panic!("{version:?} {name}: {e}"));
            assert_eq!(got_version, version, "{name}");
            assert_eq!(Some(pdu), expected(&name, version), "{version:?} {name}");
        }
    }
}

#[test]
fn encode_is_byte_identical_to_the_golden_file() {
    for version in VERSIONS {
        for name in fixtures(version) {
            if name == "es_articulated" {
                continue; // the codec drops articulation parameters, so it cannot reproduce them
            }
            let pdu = expected(&name, version).expect("expected");
            let mut out = Vec::new();
            encode(&pdu, version, &mut out).expect("encode");
            assert_eq!(out, load(version, &name), "{version:?} {name}");
            assert_eq!(out.len(), ENTITY_STATE_LEN);
        }
    }
}

#[test]
fn round_trip_through_both_versions() {
    for version in VERSIONS {
        for name in fixtures(version) {
            if name == "es_articulated" {
                continue;
            }
            let pdu = expected(&name, version).expect("expected");
            let mut out = Vec::new();
            encode(&pdu, version, &mut out).expect("encode");
            assert_eq!(decode(&out), Ok((version, pdu)), "{version:?} {name}");
        }
    }
}

#[test]
fn articulation_parameters_are_validated_and_dropped() {
    for version in VERSIONS {
        let bytes = load(version, "es_articulated");
        assert_eq!(bytes.len(), ENTITY_STATE_LEN + 32);
        let (_, pdu) = decode(&bytes).expect("decode");
        assert_eq!(Some(pdu), expected("es_articulated", version));
    }
}

#[test]
fn standard_malformed_rows() {
    for version in VERSIONS {
        assert_standard_malformed(&format!("{version:?} es_basic"), &load(version, "es_basic"));
    }
}

#[test]
fn marking_padding_is_nul_and_survives() {
    let bytes = load(DisVersion::V7, "es_basic");
    assert_eq!(&bytes[128..140], b"\x01T72-01\0\0\0\0\0");
}

#[test]
fn deactivated_appearance_is_bit_23() {
    let (_, pdu) = decode(&load(DisVersion::V7, "es_deactivated")).expect("decode");
    let es = state(pdu);
    assert!(es.is_deactivated());
    assert_eq!(
        es.appearance & APPEARANCE_DEACTIVATED,
        APPEARANCE_DEACTIVATED
    );
    let (_, active) = decode(&load(DisVersion::V7, "es_basic")).expect("decode");
    let es = state(active);
    assert!(!es.is_deactivated());
}

#[test]
fn every_dead_reckoning_code_is_named_and_round_trips() {
    let names = [
        DeadReckoningAlgorithm::Other,
        DeadReckoningAlgorithm::Static,
        DeadReckoningAlgorithm::Fpw,
        DeadReckoningAlgorithm::Rpw,
        DeadReckoningAlgorithm::Rvw,
        DeadReckoningAlgorithm::Fvw,
        DeadReckoningAlgorithm::Fpb,
        DeadReckoningAlgorithm::Rpb,
        DeadReckoningAlgorithm::Rvb,
        DeadReckoningAlgorithm::Fvb,
    ];
    for (code, name) in (0u8..).zip(names) {
        assert_eq!(DeadReckoningAlgorithm::from_code(code), name);
        assert_eq!(name.code(), code);
        let (_, pdu) = decode(&load(DisVersion::V7, &format!("es_drm_{code}"))).expect("decode");
        let es = state(pdu);
        assert_eq!(
            DeadReckoningAlgorithm::from_code(es.dead_reckoning.algorithm),
            name
        );
    }
    for code in 10..=u8::MAX {
        assert_eq!(
            DeadReckoningAlgorithm::from_code(code),
            DeadReckoningAlgorithm::Reserved(code)
        );
        assert_eq!(DeadReckoningAlgorithm::from_code(code).code(), code);
    }
}

#[test]
fn v6_and_v7_headers_differ_only_in_the_version_and_status_bytes() {
    let mut pdu = basic();
    pdu.header.status = 0x0B;
    let (mut v6, mut v7) = (Vec::new(), Vec::new());
    encode(&pdu, DisVersion::V6, &mut v6).expect("v6");
    encode(&pdu, DisVersion::V7, &mut v7).expect("v7");
    assert_eq!(v6.len(), v7.len());
    let differing: Vec<usize> = (0..v6.len()).filter(|&i| v6[i] != v7[i]).collect();
    assert_eq!(differing, vec![0, 10]);
    assert_eq!((v6[0], v7[0]), (6, 7));
    assert_eq!((v6[10], v7[10]), (0, 0x0B));
    // and the golden pair of the unmarked packet differs in the version byte alone
    let (g7, g6) = (
        load(DisVersion::V7, "es_basic"),
        load(DisVersion::V6, "es_basic"),
    );
    let differing: Vec<usize> = (0..g7.len()).filter(|&i| g7[i] != g6[i]).collect();
    assert_eq!(differing, vec![0]);
}

#[test]
fn header_strategies_are_the_only_status_difference() {
    assert_eq!(V6Header.version(), DisVersion::V6);
    assert_eq!(V7Header.version(), DisVersion::V7);
    assert_eq!(V6Header.encode_status(0xFF), [0, 0]);
    assert_eq!(V7Header.encode_status(0xFF), [0xFF, 0]);
    assert_eq!(V6Header.decode_status([0xAA, 0xBB]), 0);
    assert_eq!(V7Header.decode_status([0xAA, 0xBB]), 0xAA);
    assert_eq!(DisVersion::from_protocol_version(6), Some(DisVersion::V6));
    assert_eq!(DisVersion::from_protocol_version(7), Some(DisVersion::V7));
    for byte in (0..=u8::MAX).filter(|b| !matches!(b, 6 | 7)) {
        assert_eq!(DisVersion::from_protocol_version(byte), None);
    }
    assert_eq!(DisVersion::V6.header().version(), DisVersion::V6);
    assert_eq!(DisVersion::V7.header().version(), DisVersion::V7);
}

#[test]
fn v6_padding_bytes_are_ignored_on_decode() {
    let mut bytes = load(DisVersion::V6, "es_basic");
    bytes[10] = 0xEE;
    bytes[11] = 0xDD;
    let (_, pdu) = decode(&bytes).expect("decode");
    assert_eq!(pdu.header.status, 0);
}

#[test]
fn encode_appends_and_keeps_existing_bytes() {
    let mut out = vec![0xDE, 0xAD];
    encode(&basic(), DisVersion::V7, &mut out).expect("encode");
    assert_eq!(&out[..2], &[0xDE, 0xAD]);
    assert_eq!(out[2..], load(DisVersion::V7, "es_basic")[..]);
    let n = out.len();
    encode(&basic(), DisVersion::V6, &mut out).expect("encode");
    assert_eq!(out[n..], load(DisVersion::V6, "es_basic")[..]);
}

// ---- malformed datagrams (AT-8): errors, never panics

#[test]
fn every_truncation_is_an_error() {
    for version in VERSIONS {
        let bytes = load(version, "es_basic");
        for cut in 0..bytes.len() {
            let err = decode(&bytes[..cut]).expect_err(&format!("{version:?} cut at {cut}"));
            if cut < HEADER_LEN {
                assert!(
                    matches!(err, DecodeError::Truncated { got, .. } if got == cut),
                    "{err:?}"
                );
            } else {
                assert_eq!(
                    err,
                    DecodeError::LengthMismatch {
                        declared: 144,
                        actual: cut
                    }
                );
            }
        }
    }
}

#[test]
fn an_extended_datagram_is_a_length_mismatch() {
    for version in VERSIONS {
        let mut bytes = load(version, "es_basic");
        bytes.push(0);
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::LengthMismatch {
                declared: 144,
                actual: 145
            })
        );
        bytes.extend_from_slice(&[0; 100]);
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::LengthMismatch {
                declared: 144,
                actual: 245
            })
        );
    }
}

#[test]
fn a_wrong_length_field_is_rejected() {
    for declared in [0u16, 12, 143, 145, 160, 0xFFFF] {
        let mut bytes = load(DisVersion::V7, "es_basic");
        bytes[8..10].copy_from_slice(&declared.to_be_bytes());
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::LengthMismatch {
                declared,
                actual: 144
            }),
            "{declared}"
        );
    }
}

#[test]
fn a_consistent_length_that_is_not_valid_for_entity_state_is_bad_length() {
    let full = load(DisVersion::V7, "es_basic");
    // a datagram that says it is 100 bytes long and is: too short for an Entity State PDU
    let mut short = full[..100].to_vec();
    short[8..10].copy_from_slice(&100u16.to_be_bytes());
    assert_eq!(
        decode(&short),
        Err(DecodeError::BadLength {
            pdu_type: 1,
            length: 100
        })
    );
    // header-only PDU
    let mut header = full[..HEADER_LEN].to_vec();
    header[8..10].copy_from_slice(&12u16.to_be_bytes());
    assert_eq!(
        decode(&header),
        Err(DecodeError::BadLength {
            pdu_type: 1,
            length: 12
        })
    );
    // 144 bytes but claims one articulation parameter
    let mut claims = full.clone();
    claims[19] = 1;
    assert_eq!(
        decode(&claims),
        Err(DecodeError::BadLength {
            pdu_type: 1,
            length: 144
        })
    );
    // 160 bytes with no articulation parameter claimed
    let mut extra = full.clone();
    extra.extend_from_slice(&[0; 16]);
    extra[8..10].copy_from_slice(&160u16.to_be_bytes());
    assert_eq!(
        decode(&extra),
        Err(DecodeError::BadLength {
            pdu_type: 1,
            length: 160
        })
    );
    // 255 articulation parameters claimed with the matching length is valid but must not overflow
    let mut max = full.clone();
    max.resize(144 + 255 * 16, 0);
    max[19] = 255;
    let max_len = u16::try_from(max.len()).expect("fits");
    max[8..10].copy_from_slice(&max_len.to_be_bytes());
    assert!(decode(&max).is_ok());
}

#[test]
fn a_wrong_protocol_family_is_rejected() {
    for family in [0u8, 2, 5, 255] {
        let mut bytes = load(DisVersion::V7, "es_basic");
        bytes[3] = family;
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::WrongFamily {
                pdu_type: 1,
                family
            })
        );
    }
}

#[test]
fn unknown_versions_and_pdu_types_are_distinct_errors() {
    for version_byte in [0u8, 1, 5, 8, 255] {
        let mut bytes = load(DisVersion::V7, "es_basic");
        bytes[0] = version_byte;
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::UnsupportedVersion(version_byte))
        );
    }
    for pdu_type in [0u8, 4, 12, 15, 200] {
        let mut bytes = load(DisVersion::V6, "es_basic");
        bytes[2] = pdu_type;
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::UnsupportedPduType(pdu_type))
        );
    }
}

#[test]
fn empty_and_tiny_inputs_are_truncated() {
    assert_eq!(
        decode(&[]),
        Err(DecodeError::Truncated { needed: 1, got: 0 })
    );
    assert!(matches!(decode(&[7]), Err(DecodeError::Truncated { .. })));
}

#[test]
fn error_messages_are_readable() {
    let text = DecodeError::LengthMismatch {
        declared: 144,
        actual: 145,
    }
    .to_string();
    assert_eq!(text, "header declares 144 bytes but the datagram has 145");
}
