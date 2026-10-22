//! Property tests of the DIS codec (M6 WI-13, AT-8; NFR-2): arbitrary input never panics, every
//! valid Entity State PDU round-trips in both versions, and what the decoder accepts re-encodes.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use umbra_net_dis::codec::{
    Body, DeadReckoningParameters, ENTITY_STATE_LEN, EntityId, EntityState, EntityType, HEADER_LEN,
    Header, Marking, Pdu, decode, encode,
};
use umbra_net_dis::version::DisVersion;

fn version() -> impl Strategy<Value = DisVersion> {
    prop_oneof![Just(DisVersion::V6), Just(DisVersion::V7)]
}

fn finite_f32() -> impl Strategy<Value = f32> {
    prop::num::f32::NORMAL | prop::num::f32::SUBNORMAL | prop::num::f32::ZERO
}

fn finite_f64() -> impl Strategy<Value = f64> {
    prop::num::f64::NORMAL | prop::num::f64::SUBNORMAL | prop::num::f64::ZERO
}

fn any_f32x3() -> impl Strategy<Value = [f32; 3]> {
    [finite_f32(), finite_f32(), finite_f32()]
}

fn entity_type() -> impl Strategy<Value = EntityType> {
    (any::<u8>(), any::<u8>(), any::<u16>(), any::<[u8; 4]>()).prop_map(
        |(kind, domain, country, [category, subcategory, specific, extra])| EntityType {
            kind,
            domain,
            country,
            category,
            subcategory,
            specific,
            extra,
        },
    )
}

fn entity_state() -> impl Strategy<Value = EntityState> {
    let ids = (any::<u16>(), any::<u16>(), any::<u16>(), any::<u8>());
    let kinematics = (
        any_f32x3(),
        [finite_f64(), finite_f64(), finite_f64()],
        any_f32x3(),
        any::<u32>(),
    );
    let dr = (any::<u8>(), any::<[u8; 15]>(), any_f32x3(), any_f32x3());
    let tail = (any::<u8>(), any::<[u8; 11]>(), any::<u32>());
    (ids, entity_type(), entity_type(), kinematics, dr, tail).prop_map(
        |((site, application, entity, force_id), t, alt, k, dr, tail)| EntityState {
            entity_id: EntityId {
                site,
                application,
                entity,
            },
            force_id,
            entity_type: t,
            alternative_entity_type: alt,
            linear_velocity: k.0,
            location: k.1,
            orientation: k.2,
            appearance: k.3,
            dead_reckoning: DeadReckoningParameters {
                algorithm: dr.0,
                other: dr.1,
                linear_acceleration: dr.2,
                angular_velocity: dr.3,
            },
            marking: Marking {
                character_set: tail.0,
                characters: tail.1,
            },
            capabilities: tail.2,
        },
    )
}

/// A valid PDU for `version` (v6 has no status field).
fn pdu_for(version: DisVersion) -> impl Strategy<Value = Pdu> {
    (any::<u8>(), any::<u32>(), any::<u8>(), entity_state()).prop_map(
        move |(exercise_id, timestamp, status, es)| Pdu {
            header: Header {
                exercise_id,
                timestamp,
                status: if version == DisVersion::V7 { status } else { 0 },
            },
            body: Body::EntityState(es),
        },
    )
}

fn version_and_pdu() -> impl Strategy<Value = (DisVersion, Pdu)> {
    version().prop_flat_map(|v| pdu_for(v).prop_map(move |p| (v, p)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Arbitrary bytes: an `Ok` or an `Err`, never a panic.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..600)) {
        let _ = decode(&bytes);
    }

    /// Bytes behind a plausible Entity State header with a consistent length reach the body code.
    #[test]
    fn arbitrary_bodies_behind_a_valid_header_never_panic(
        v in version(),
        body in prop::collection::vec(any::<u8>(), 0..400),
        stray in any::<bool>(),
    ) {
        let total = HEADER_LEN + body.len();
        let mut bytes = vec![v.protocol_version(), 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        let declared = u16::try_from(if stray { total + 1 } else { total }).expect("small");
        bytes[8..10].copy_from_slice(&declared.to_be_bytes());
        bytes.extend_from_slice(&body);
        let result = decode(&bytes);
        if stray {
            prop_assert!(result.is_err());
        }
    }

    /// Every one-byte mutation, truncation and extension of a valid packet is handled.
    #[test]
    fn mutated_valid_packets_never_panic(
        (v, pdu) in version_and_pdu(),
        index in 0usize..ENTITY_STATE_LEN,
        byte in any::<u8>(),
        cut in 0usize..=ENTITY_STATE_LEN,
        tail in prop::collection::vec(any::<u8>(), 0..32),
    ) {
        let mut out = Vec::new();
        encode(&pdu, v, &mut out).expect("encode");
        let mut mutated = out.clone();
        mutated[index] = byte;
        let _ = decode(&mutated);
        let _ = decode(&out[..cut]);
        out.extend_from_slice(&tail);
        let _ = decode(&out);
    }

    /// Every valid PDU round-trips exactly, and is 144 bytes.
    #[test]
    fn valid_entity_states_round_trip((v, pdu) in version_and_pdu()) {
        let mut out = Vec::new();
        encode(&pdu, v, &mut out).expect("encode");
        prop_assert_eq!(out.len(), ENTITY_STATE_LEN);
        prop_assert_eq!(decode(&out), Ok((v, pdu)));
    }

    /// Anything the decoder accepts re-encodes to a packet that decodes to the same PDU, bit for
    /// bit (also for NaN and infinities, which compare unequal as floats).
    #[test]
    fn accepted_packets_re_encode_stably(v in version(), status in any::<u8>(), body in prop::collection::vec(any::<u8>(), ENTITY_STATE_LEN - HEADER_LEN)) {
        let mut bytes = vec![v.protocol_version(), 9, 1, 1, 0, 0, 0, 0, 0, 144, 0, 0];
        bytes.extend_from_slice(&body);
        bytes[19] = 0; // no articulation parameters
        bytes[10] = status; // a v6 receiver ignores it
        let (dv, pdu) = decode(&bytes).expect("any 132-byte body is a valid Entity State");
        prop_assert_eq!(dv, v);
        let mut again = Vec::new();
        encode(&pdu, dv, &mut again).expect("encode");
        // identical but for the v6 padding bytes (they are not kept)
        prop_assert_eq!(&again[..10], &bytes[..10]);
        if v == DisVersion::V7 {
            prop_assert_eq!(again[10], bytes[10]);
        }
        prop_assert_eq!(&again[12..], &bytes[12..]);
        let (_, back) = decode(&again).expect("decode again");
        let mut twice = Vec::new();
        encode(&back, dv, &mut twice).expect("encode");
        prop_assert_eq!(twice, again);
    }
}
