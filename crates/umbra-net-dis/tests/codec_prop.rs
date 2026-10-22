//! Property tests of the DIS codec (M6 WI-13, AT-8; NFR-2): arbitrary input never panics, every
//! valid Entity State PDU round-trips in both versions, and what the decoder accepts re-encodes.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use umbra_net_dis::codec::{
    Body, BurstDescriptor, ClockTime, DETONATION_LEN, DeadReckoningParameters, Descriptor,
    Detonation, ENTITY_STATE_LEN, EntityId, EntityState, EntityType, EventId, FIRE_LEN, Fire,
    HEADER_LEN, Header, Marking, Pdu, START_RESUME_LEN, STOP_FREEZE_LEN, StartResume, StopFreeze,
    decode, encode,
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

fn entity_id() -> impl Strategy<Value = EntityId> {
    any::<[u16; 3]>().prop_map(|[site, application, entity]| EntityId {
        site,
        application,
        entity,
    })
}

fn burst() -> impl Strategy<Value = BurstDescriptor> {
    (entity_type(), any::<[u16; 4]>()).prop_map(|(munition, [warhead, fuse, quantity, rate])| {
        BurstDescriptor {
            munition,
            warhead,
            fuse,
            quantity,
            rate,
        }
    })
}

/// Any descriptor form; `align` makes it fit the PDU and the version.
fn descriptor() -> impl Strategy<Value = Descriptor> {
    prop_oneof![
        burst().prop_map(Descriptor::Munition),
        entity_type().prop_map(|entity_type| Descriptor::Expendable { entity_type }),
        (entity_type(), any::<u16>(), finite_f32()).prop_map(
            |(entity_type, explosive_material, explosive_force)| {
                Descriptor::Explosion {
                    entity_type,
                    explosive_material,
                    explosive_force,
                }
            }
        ),
    ]
}

fn clock() -> impl Strategy<Value = ClockTime> {
    (any::<i32>(), any::<u32>()).prop_map(|(hour, time_past_hour)| ClockTime {
        hour,
        time_past_hour,
    })
}

fn f64x3() -> impl Strategy<Value = [f64; 3]> {
    [finite_f64(), finite_f64(), finite_f64()]
}

fn fire() -> impl Strategy<Value = Fire> {
    let ids = (entity_id(), entity_id(), entity_id(), any::<[u16; 3]>());
    let rest = (
        any::<u32>(),
        f64x3(),
        descriptor(),
        any_f32x3(),
        finite_f32(),
    );
    (ids, rest).prop_map(|((firing_entity, target_entity, munition, ev), r)| Fire {
        firing_entity,
        target_entity,
        munition,
        event: EventId {
            site: ev[0],
            application: ev[1],
            event: ev[2],
        },
        fire_mission_index: r.0,
        location: r.1,
        descriptor: r.2,
        velocity: r.3,
        range: r.4,
    })
}

fn detonation() -> impl Strategy<Value = Detonation> {
    let ids = (entity_id(), entity_id(), entity_id(), any::<[u16; 3]>());
    let rest = (any_f32x3(), f64x3(), descriptor(), any_f32x3(), any::<u8>());
    (ids, rest).prop_map(
        |((firing_entity, target_entity, munition, ev), r)| Detonation {
            firing_entity,
            target_entity,
            munition,
            event: EventId {
                site: ev[0],
                application: ev[1],
                event: ev[2],
            },
            velocity: r.0,
            location: r.1,
            descriptor: r.2,
            location_in_entity: r.3,
            result: r.4,
        },
    )
}

fn start_resume() -> impl Strategy<Value = StartResume> {
    (entity_id(), entity_id(), clock(), clock(), any::<u32>()).prop_map(
        |(originating_entity, receiving_entity, real_world_time, simulation_time, request_id)| {
            StartResume {
                originating_entity,
                receiving_entity,
                real_world_time,
                simulation_time,
                request_id,
            }
        },
    )
}

fn stop_freeze() -> impl Strategy<Value = StopFreeze> {
    (
        entity_id(),
        entity_id(),
        clock(),
        any::<[u8; 2]>(),
        any::<u32>(),
    )
        .prop_map(
            |(
                originating_entity,
                receiving_entity,
                real_world_time,
                [reason, frozen_behavior],
                request_id,
            )| {
                StopFreeze {
                    originating_entity,
                    receiving_entity,
                    real_world_time,
                    reason,
                    frozen_behavior,
                    request_id,
                }
            },
        )
}

fn body() -> impl Strategy<Value = Body> {
    prop_oneof![
        entity_state().prop_map(Body::EntityState),
        fire().prop_map(Body::Fire),
        detonation().prop_map(Body::Detonation),
        start_resume().prop_map(Body::StartResume),
        stop_freeze().prop_map(Body::StopFreeze),
    ]
}

/// Makes a generated PDU valid for `version`: v6 has no status field and only the munition form;
/// in v7 the status bits name the descriptor form (a Fire has no explosion form).
fn align(version: DisVersion, pdu: &mut Pdu) {
    let (descriptor, mask) = match &mut pdu.body {
        Body::Fire(f) => {
            if let Descriptor::Explosion { entity_type, .. } = f.descriptor {
                f.descriptor = Descriptor::Expendable { entity_type };
            }
            (Some(&mut f.descriptor), 0x10u8)
        }
        Body::Detonation(d) => (Some(&mut d.descriptor), 0x30),
        _ => (None, 0),
    };
    if version == DisVersion::V6 {
        pdu.header.status = 0;
        if let Some(d) = descriptor
            && !matches!(d, Descriptor::Munition(_))
        {
            *d = Descriptor::default();
        }
        return;
    }
    if let Some(d) = descriptor {
        let indicator = match d {
            Descriptor::Munition(_) => 0u8,
            Descriptor::Expendable { .. } => 1,
            Descriptor::Explosion { .. } => 2,
        };
        pdu.header.status = (pdu.header.status & !mask) | (indicator << 4);
    }
}

/// A valid PDU for `version` (v6 has no status field).
fn pdu_for(version: DisVersion) -> impl Strategy<Value = Pdu> {
    (any::<u8>(), any::<u32>(), any::<u8>(), body()).prop_map(
        move |(exercise_id, timestamp, status, body)| {
            let mut pdu = Pdu {
                header: Header {
                    exercise_id,
                    timestamp,
                    status,
                },
                body,
            };
            align(version, &mut pdu);
            pdu
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

    /// Bytes behind a plausible header of any supported type with a consistent length reach the body code.
    #[test]
    fn arbitrary_bodies_behind_a_valid_header_never_panic(
        v in version(),
        body in prop::collection::vec(any::<u8>(), 0..400),
        stray in any::<bool>(),
        (pdu_type, family) in prop_oneof![Just((1u8, 1u8)), Just((2, 2)), Just((3, 2)), Just((13, 5)), Just((14, 5))],
    ) {
        let total = HEADER_LEN + body.len();
        let mut bytes = vec![v.protocol_version(), 1, pdu_type, family, 0, 0, 0, 0, 0, 0, 0, 0];
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
        index in 0usize..STOP_FREEZE_LEN,
        byte in any::<u8>(),
        cut in 0usize..=STOP_FREEZE_LEN,
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

    /// Every valid PDU of every family round-trips exactly, with its fixed length.
    #[test]
    fn valid_pdus_round_trip((v, pdu) in version_and_pdu()) {
        let mut out = Vec::new();
        encode(&pdu, v, &mut out).expect("encode");
        let want = match pdu.body {
            Body::EntityState(_) => ENTITY_STATE_LEN,
            Body::Fire(_) => FIRE_LEN,
            Body::Detonation(_) => DETONATION_LEN,
            Body::StartResume(_) => START_RESUME_LEN,
            Body::StopFreeze(_) => STOP_FREEZE_LEN,
            _ => unreachable!("every body of this crate is listed"),
        };
        prop_assert_eq!(out.len(), want);
        prop_assert_eq!(usize::from(u16::from_be_bytes([out[8], out[9]])), want);
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

    /// The fixed-length families the decoder accepts re-encode to the same bytes (padding zero), and
    /// a reserved Detonation indicator is the one typed rejection.
    #[test]
    fn accepted_other_families_re_encode_identically(
        v in version(),
        status in any::<u8>(),
        which in 0usize..4,
        body in prop::collection::vec(any::<u8>(), 92),
    ) {
        let (pdu_type, family, len) = [(2u8, 2u8, FIRE_LEN), (3, 2, DETONATION_LEN), (13, 5, START_RESUME_LEN), (14, 5, STOP_FREEZE_LEN)][which];
        let status = if v == DisVersion::V7 { status } else { 0 };
        let mut bytes = vec![v.protocol_version(), 9, pdu_type, family, 1, 2, 3, 4, 0, 0, status, 0];
        bytes.extend_from_slice(&body[..len - HEADER_LEN]);
        bytes[8..10].copy_from_slice(&u16::try_from(len).expect("small").to_be_bytes());
        let indicator = (status >> 4) & 3;
        let descriptor_at = if pdu_type == 2 { 64 } else { 72 };
        match pdu_type {
            2 if indicator & 1 == 1 => bytes[descriptor_at + 8..descriptor_at + 16].fill(0),
            3 => {
                bytes[101] = 0; // no parameter records
                bytes[102] = 0;
                bytes[103] = 0; // padding
                match indicator {
                    1 => bytes[descriptor_at + 8..descriptor_at + 16].fill(0),
                    2 => bytes[descriptor_at + 10..descriptor_at + 12].fill(0),
                    _ => {}
                }
            }
            14 => {
                bytes[34] = 0;
                bytes[35] = 0; // padding
            }
            _ => {}
        }
        if pdu_type == 3 && indicator == 3 {
            prop_assert!(decode(&bytes).is_err());
            return Ok(());
        }
        let (dv, pdu) = decode(&bytes).expect("a fixed-length body is a valid PDU");
        prop_assert_eq!(dv, v);
        let mut again = Vec::new();
        encode(&pdu, dv, &mut again).expect("encode");
        prop_assert_eq!(again, bytes);
    }
}
