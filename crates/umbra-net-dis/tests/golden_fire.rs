//! Fire PDU golden packets, v7 and v6 (M6 WI-14, AT-7 Fire rows, AT-8; REQ-N-6, NFR-2). The fixtures
//! come from `gen_warfare.py`, not from this codec; the expected values are written by hand.
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod support;

use support::{
    VERSIONS, assert_golden, assert_standard_malformed,
    assert_versions_differ_only_in_version_and_status, fixtures_with_prefix, load, set_length,
};
use umbra_net_dis::codec::{
    Body, BurstDescriptor, DecodeError, Descriptor, EncodeError, EntityId, EntityType, EventId,
    FIRE_LEN, Fire, Header, Pdu, decode, encode,
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

fn munition() -> Descriptor {
    Descriptor::Munition(BurstDescriptor {
        munition: MUNITION_TYPE,
        warhead: 0x1201,
        fuse: 0x1302,
        quantity: 0x1403,
        rate: 0x1504,
    })
}

fn fire_basic() -> Fire {
    Fire {
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
        fire_mission_index: 0x0A0B_0C0D,
        location: [3_900_100.5, 1_100_200.25, 4_800_300.125],
        descriptor: munition(),
        velocity: [100.5, -20.25, 3.125],
        range: 2500.5,
    }
}

fn pdu(status: u8, fire: Fire) -> Pdu {
    Pdu {
        header: Header {
            exercise_id: 1,
            timestamp: 0x0102_0304,
            status,
        },
        body: Body::Fire(fire),
    }
}

/// The expected value; v6 has no status field.
fn expected(name: &str, version: DisVersion) -> Option<Pdu> {
    let (status, fire) = match name {
        "fire_basic" => (0, fire_basic()),
        "fire_no_target" => (
            0,
            Fire {
                target_entity: EntityId::default(),
                ..fire_basic()
            },
        ),
        "fire_status" => (0x0D, fire_basic()),
        "fire_expendable" => (
            0x11,
            Fire {
                descriptor: Descriptor::Expendable {
                    entity_type: EXPENDABLE_TYPE,
                },
                ..fire_basic()
            },
        ),
        _ => return None,
    };
    Some(pdu(
        if version == DisVersion::V7 { status } else { 0 },
        fire,
    ))
}

#[test]
fn every_fire_file_has_an_expectation() {
    for version in VERSIONS {
        let names = fixtures_with_prefix(version, &["fire_"]);
        let want: &[&str] = if version == DisVersion::V7 {
            &[
                "fire_basic",
                "fire_expendable",
                "fire_no_target",
                "fire_status",
            ]
        } else {
            &["fire_basic", "fire_no_target"]
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
        for name in fixtures_with_prefix(version, &["fire_"]) {
            assert_golden(
                version,
                &name,
                &expected(&name, version).expect("expected"),
                false,
            );
            assert_eq!(load(version, &name).len(), FIRE_LEN);
        }
    }
}

#[test]
fn v6_and_v7_differ_only_in_the_version_and_status_bytes() {
    for name in fixtures_with_prefix(DisVersion::V6, &["fire_"]) {
        assert_versions_differ_only_in_version_and_status(&name);
    }
}

#[test]
fn malformed_rows() {
    for version in VERSIONS {
        for name in fixtures_with_prefix(version, &["fire_"]) {
            assert_standard_malformed(&format!("{version:?} {name}"), &load(version, &name));
        }
    }
}

#[test]
fn the_fire_type_indicator_is_status_bit_4() {
    let bytes = load(DisVersion::V7, "fire_basic");
    for (status, form) in [
        (0x00u8, false),
        (0x10, true),
        (0x0F, false),
        (0x1F, true),
        (0x20, false),
        (0x30, true),
    ] {
        let mut b = bytes.clone();
        b[10] = status;
        let (_, p) = decode(&b).expect("decodes");
        let Body::Fire(f) = p.body else {
            panic!("fire")
        };
        assert_eq!(
            matches!(f.descriptor, Descriptor::Expendable { .. }),
            form,
            "status {status:#x}"
        );
        assert_eq!(p.header.status, status);
    }
}

#[test]
fn v6_ignores_the_status_bits() {
    let mut b = load(DisVersion::V6, "fire_basic");
    b[10] = 0x10;
    let (_, p) = decode(&b).expect("decodes");
    assert_eq!(p, expected("fire_basic", DisVersion::V6).expect("expected"));
}

#[test]
fn expendable_padding_is_ignored_on_decode_and_zero_on_encode() {
    let mut b = load(DisVersion::V7, "fire_expendable");
    b[72..80].copy_from_slice(&[0xEE; 8]);
    assert_eq!(
        decode(&b).map(|(_, p)| p),
        Ok(expected("fire_expendable", DisVersion::V7).expect("expected"))
    );
    let mut out = Vec::new();
    encode(
        &expected("fire_expendable", DisVersion::V7).expect("expected"),
        DisVersion::V7,
        &mut out,
    )
    .expect("encode");
    assert_eq!(&out[72..80], &[0; 8]);
}

#[test]
fn encode_refuses_a_descriptor_the_status_does_not_name() {
    let expendable = Fire {
        descriptor: Descriptor::Expendable {
            entity_type: EXPENDABLE_TYPE,
        },
        ..fire_basic()
    };
    let explosion = Fire {
        descriptor: Descriptor::Explosion {
            entity_type: EXPENDABLE_TYPE,
            explosive_material: 1,
            explosive_force: 1.0,
        },
        ..fire_basic()
    };
    let mismatch = |status| {
        Err(EncodeError::DescriptorStatusMismatch {
            pdu_type: 2,
            status,
        })
    };
    let mut out = vec![7];
    // v7: expendable needs bit 4, munition needs it clear, an explosion has no Fire indicator value
    assert_eq!(
        encode(&pdu(0, expendable.clone()), DisVersion::V7, &mut out),
        mismatch(0)
    );
    assert_eq!(
        encode(&pdu(0x10, fire_basic()), DisVersion::V7, &mut out),
        mismatch(0x10)
    );
    assert_eq!(
        encode(&pdu(0x10, explosion.clone()), DisVersion::V7, &mut out),
        mismatch(0x10)
    );
    assert_eq!(
        encode(&pdu(0, explosion), DisVersion::V7, &mut out),
        mismatch(0)
    );
    // v6 has only the munition form
    assert_eq!(
        encode(&pdu(0x10, expendable), DisVersion::V6, &mut out),
        Err(EncodeError::DescriptorNeedsV7 { pdu_type: 2 })
    );
    assert_eq!(out, vec![7], "an error leaves the output as it was");
    // and the matching combinations are fine, bit 5 and the low bits do not matter
    encode(&pdu(0x2F, fire_basic()), DisVersion::V7, &mut out).expect("munition with other bits");
}

#[test]
fn a_consistent_length_other_than_96_is_bad_length() {
    for length in [12usize, 95, 97, 144] {
        let mut bytes = load(DisVersion::V7, "fire_basic");
        bytes.resize(length, 0);
        let declared = u16::try_from(length).expect("small");
        set_length(&mut bytes, declared);
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::BadLength {
                pdu_type: 2,
                length: declared
            })
        );
    }
}

#[test]
fn a_fire_body_in_another_types_header_is_rejected() {
    // type 13 (Start/Resume) with family 2: wrong family; type 99: unsupported
    let mut bytes = load(DisVersion::V6, "fire_basic");
    bytes[2] = 13;
    assert_eq!(
        decode(&bytes),
        Err(DecodeError::WrongFamily {
            pdu_type: 13,
            family: 2
        })
    );
    bytes[2] = 99;
    assert_eq!(decode(&bytes), Err(DecodeError::UnsupportedPduType(99)));
}

#[test]
fn encode_uses_type_2_family_2_and_the_requested_version() {
    for (version, byte) in [(DisVersion::V6, 6u8), (DisVersion::V7, 7)] {
        let mut out = Vec::new();
        encode(&pdu(0, fire_basic()), version, &mut out).expect("encode");
        assert_eq!(&out[..4], &[byte, 1, 2, 2]);
    }
}
