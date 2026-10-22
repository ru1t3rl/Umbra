//! Detonation PDU golden packets, v7 and v6 (M6 WI-14, AT-7 Detonation rows, AT-8). The fixtures
//! come from `gen_warfare.py`, not from this codec; the expected values are written by hand.
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod support;

use support::{
    VERSIONS, assert_golden, assert_standard_malformed,
    assert_versions_differ_only_in_version_and_status, fixtures_with_prefix, load, set_length,
};
use umbra_net_dis::codec::{
    Body, BurstDescriptor, DETONATION_LEN, DecodeCounters, DecodeError, Descriptor, Detonation,
    EncodeError, EntityId, EntityType, EventId, Header, Pdu, decode, encode,
};
use umbra_net_dis::version::DisVersion;

const MUNITION_TYPE: EntityType = EntityType {
    kind: 2,
    domain: 2,
    country: 225,
    category: 3,
    subcategory: 4,
    specific: 5,
    extra: 6,
};
const EXPENDABLE_TYPE: EntityType = EntityType {
    kind: 3,
    domain: 2,
    country: 225,
    category: 3,
    subcategory: 4,
    specific: 5,
    extra: 6,
};
const EXPLOSION_TYPE: EntityType = EntityType {
    kind: 4,
    domain: 1,
    country: 225,
    category: 7,
    subcategory: 8,
    specific: 9,
    extra: 10,
};

fn munition() -> Descriptor {
    Descriptor::Munition(BurstDescriptor {
        munition: MUNITION_TYPE,
        warhead: 0x1201,
        fuse: 0x1302,
        quantity: 0x1403,
        rate: 0x1504,
    })
}

fn expendable() -> Descriptor {
    Descriptor::Expendable {
        entity_type: EXPENDABLE_TYPE,
    }
}

fn explosion() -> Descriptor {
    Descriptor::Explosion {
        entity_type: EXPLOSION_TYPE,
        explosive_material: 0x1234,
        explosive_force: 12.5,
    }
}

fn det_basic() -> Detonation {
    Detonation {
        firing_entity: EntityId {
            site: 1,
            application: 2,
            entity: 3,
        },
        target_entity: EntityId {
            site: 4,
            application: 5,
            entity: 6,
        },
        munition: EntityId {
            site: 7,
            application: 8,
            entity: 9,
        },
        event: EventId {
            site: 10,
            application: 11,
            event: 12,
        },
        velocity: [30.5, -4.25, 1.125],
        location: [3_900_100.5, 1_100_200.25, 4_800_300.125],
        descriptor: munition(),
        location_in_entity: [0.5, 1.5, -2.25],
        result: 5,
    }
}

fn pdu(status: u8, d: Detonation) -> Pdu {
    Pdu {
        header: Header {
            exercise_id: 1,
            timestamp: 0x0102_0304,
            status,
        },
        body: Body::Detonation(d),
    }
}

/// The expected value; v6 has no status field.
fn expected(name: &str, version: DisVersion) -> Option<Pdu> {
    let (status, d) = match name {
        "det_basic" | "det_articulated" => (0x03, det_basic()),
        "det_result_1" => (
            0x03,
            Detonation {
                result: 1,
                ..det_basic()
            },
        ),
        "det_no_target" => (
            0x03,
            Detonation {
                target_entity: EntityId::default(),
                ..det_basic()
            },
        ),
        "det_expendable" => (
            0x10,
            Detonation {
                descriptor: expendable(),
                ..det_basic()
            },
        ),
        "det_explosion" => (
            0x21,
            Detonation {
                descriptor: explosion(),
                ..det_basic()
            },
        ),
        _ => return None,
    };
    Some(pdu(if version == DisVersion::V7 { status } else { 0 }, d))
}

fn exp(name: &str, version: DisVersion) -> Pdu {
    expected(name, version).expect("expected")
}

#[test]
fn every_detonation_file_has_an_expectation() {
    for version in VERSIONS {
        let names = fixtures_with_prefix(version, &["det_"]);
        let want: &[&str] = if version == DisVersion::V7 {
            &[
                "det_articulated",
                "det_basic",
                "det_expendable",
                "det_explosion",
                "det_no_target",
                "det_result_1",
            ]
        } else {
            &[
                "det_articulated",
                "det_basic",
                "det_no_target",
                "det_result_1",
            ]
        };
        assert_eq!(names, want, "{version:?}");
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
        for name in fixtures_with_prefix(version, &["det_"]) {
            // the codec drops parameter records, so the articulated file is decode-only
            assert_golden(
                version,
                &name,
                &exp(&name, version),
                name == "det_articulated",
            );
        }
    }
}

#[test]
fn parameter_records_are_validated_and_dropped() {
    for version in VERSIONS {
        let bytes = load(version, "det_articulated");
        assert_eq!(bytes.len(), DETONATION_LEN + 32);
        let mut out = Vec::new();
        encode(&exp("det_articulated", version), version, &mut out).expect("encode");
        assert_eq!(out.len(), DETONATION_LEN, "the encoder writes no records");
    }
}

#[test]
fn v6_and_v7_differ_only_in_the_version_and_status_bytes() {
    for name in fixtures_with_prefix(DisVersion::V6, &["det_"]) {
        assert_versions_differ_only_in_version_and_status(&name);
    }
}

#[test]
fn malformed_rows() {
    for version in VERSIONS {
        for name in fixtures_with_prefix(version, &["det_"]) {
            if name != "det_articulated" {
                assert_standard_malformed(&format!("{version:?} {name}"), &load(version, &name));
            }
        }
    }
}

#[test]
fn the_detonation_type_indicator_is_status_bits_4_and_5() {
    let bytes = load(DisVersion::V7, "det_basic");
    for (status, form) in [
        (0x00u8, 0),
        (0x0F, 0),
        (0x10, 1),
        (0x1F, 1),
        (0x20, 2),
        (0x2A, 2),
        (0xC0, 0),
        (0xD0, 1),
        (0xE0, 2),
    ] {
        let mut b = bytes.clone();
        b[10] = status;
        let (_, p) = decode(&b).expect("decodes");
        let Body::Detonation(d) = p.body else {
            panic!("detonation")
        };
        let got = match d.descriptor {
            Descriptor::Munition(_) => 0,
            Descriptor::Expendable { .. } => 1,
            Descriptor::Explosion { .. } => 2,
        };
        assert_eq!(got, form, "status {status:#x}");
        assert_eq!(p.header.status, status);
    }
}

#[test]
fn the_reserved_indicator_is_a_typed_error_counted_as_malformed() {
    for status in [0x30u8, 0x3F, 0xF0] {
        let mut b = load(DisVersion::V7, "det_basic");
        b[10] = status;
        assert_eq!(
            decode(&b),
            Err(DecodeError::ReservedDescriptorForm {
                pdu_type: 3,
                indicator: 3
            }),
            "status {status:#x}"
        );
        let mut counters = DecodeCounters::default();
        assert!(counters.decode(&b).is_none());
        assert_eq!(counters.malformed, 1);
    }
}

#[test]
fn v6_ignores_the_status_bits_and_reads_the_munition_form() {
    for status in [0x10u8, 0x20, 0x30] {
        let mut b = load(DisVersion::V6, "det_basic");
        b[10] = status;
        let (_, p) = decode(&b).expect("v6 ignores byte 10");
        assert_eq!(p, exp("det_basic", DisVersion::V6), "status {status:#x}");
    }
}

#[test]
fn non_munition_padding_is_ignored_on_decode_and_zero_on_encode() {
    let mut e = load(DisVersion::V7, "det_expendable");
    e[80..88].copy_from_slice(&[0xEE; 8]);
    assert_eq!(
        decode(&e).map(|(_, p)| p),
        Ok(exp("det_expendable", DisVersion::V7))
    );
    let mut x = load(DisVersion::V7, "det_explosion");
    x[82..84].copy_from_slice(&[0xEE; 2]);
    assert_eq!(
        decode(&x).map(|(_, p)| p),
        Ok(exp("det_explosion", DisVersion::V7))
    );
    for name in ["det_expendable", "det_explosion"] {
        let mut out = Vec::new();
        encode(&exp(name, DisVersion::V7), DisVersion::V7, &mut out).expect("encode");
        assert_eq!(out, load(DisVersion::V7, name), "{name}");
    }
}

#[test]
fn encode_refuses_a_descriptor_the_status_does_not_name() {
    let mut out = vec![7];
    let mismatch = |status| {
        Err(EncodeError::DescriptorStatusMismatch {
            pdu_type: 3,
            status,
        })
    };
    let with = |descriptor| Detonation {
        descriptor,
        ..det_basic()
    };
    // the indicator values must not be swapped: 1 is expendable, 2 is explosion
    assert_eq!(
        encode(&pdu(0x10, with(explosion())), DisVersion::V7, &mut out),
        mismatch(0x10)
    );
    assert_eq!(
        encode(&pdu(0x20, with(expendable())), DisVersion::V7, &mut out),
        mismatch(0x20)
    );
    assert_eq!(
        encode(&pdu(0x00, with(expendable())), DisVersion::V7, &mut out),
        mismatch(0x00)
    );
    assert_eq!(
        encode(&pdu(0x20, det_basic()), DisVersion::V7, &mut out),
        mismatch(0x20)
    );
    // the reserved indicator names no form
    assert_eq!(
        encode(&pdu(0x30, det_basic()), DisVersion::V7, &mut out),
        mismatch(0x30)
    );
    // v6 has only the munition form
    for d in [expendable(), explosion()] {
        assert_eq!(
            encode(&pdu(0, with(d)), DisVersion::V6, &mut out),
            Err(EncodeError::DescriptorNeedsV7 { pdu_type: 3 })
        );
    }
    assert_eq!(out, vec![7], "an error leaves the output as it was");
    encode(&pdu(0x10, with(expendable())), DisVersion::V7, &mut out).expect("matching");
    encode(&pdu(0x2F, with(explosion())), DisVersion::V7, &mut out)
        .expect("matching with other bits");
}

#[test]
fn record_count_and_length_must_agree() {
    let basic = load(DisVersion::V7, "det_basic");
    // shorter than the fixed part, with a matching length field
    for length in [12usize, 103] {
        let mut bytes = basic.clone();
        bytes.resize(length, 0);
        let declared = u16::try_from(length).expect("small");
        set_length(&mut bytes, declared);
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::BadLength {
                pdu_type: 3,
                length: declared
            })
        );
    }
    // 16 extra bytes but no record claimed, and a record claimed but absent
    let mut extra = basic.clone();
    extra.extend_from_slice(&[0; 16]);
    set_length(&mut extra, 120);
    assert_eq!(
        decode(&extra),
        Err(DecodeError::BadLength {
            pdu_type: 3,
            length: 120
        })
    );
    let mut claimed = basic.clone();
    claimed[101] = 1;
    assert_eq!(
        decode(&claimed),
        Err(DecodeError::BadLength {
            pdu_type: 3,
            length: 104
        })
    );
    // two records claimed but the length holds one record and 8 stray bytes
    let mut stray = load(DisVersion::V7, "det_articulated");
    stray.truncate(DETONATION_LEN + 24);
    set_length(&mut stray, 128);
    assert_eq!(stray[101], 2);
    assert_eq!(
        decode(&stray),
        Err(DecodeError::BadLength {
            pdu_type: 3,
            length: 128
        })
    );
    // 255 records with the matching length is valid (no overflow)
    let mut max = load(DisVersion::V7, "det_basic");
    max.resize(DETONATION_LEN + 255 * 16, 0);
    max[101] = 255;
    let declared = u16::try_from(max.len()).expect("fits");
    set_length(&mut max, declared);
    assert_eq!(
        decode(&max).map(|(_, p)| p),
        Ok(exp("det_basic", DisVersion::V7))
    );
}

#[test]
fn padding_is_ignored_on_decode_and_zero_on_encode() {
    let mut bytes = load(DisVersion::V6, "det_basic");
    bytes[102] = 0xAB;
    bytes[103] = 0xCD;
    assert_eq!(
        decode(&bytes).map(|(_, p)| p),
        Ok(exp("det_basic", DisVersion::V6))
    );
    let mut out = Vec::new();
    encode(&exp("det_basic", DisVersion::V6), DisVersion::V6, &mut out).expect("encode");
    assert_eq!(&out[100..104], &[5, 0, 0, 0]);
}
