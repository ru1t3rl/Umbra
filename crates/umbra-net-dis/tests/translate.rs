//! The translation layer (M6 WI-15; REQ-N-5, REQ-N-6, ADR 0034, ASM-420, ASM-427..): tables both ways
//! with the SDK mirror types, key packing and `KeySpace`, entity type tokens, markings, relative
//! timestamps, removal, Fire/Detonation/run control, descriptor forms, the exercise filter, non-finite
//! values and the orientation frame (lat 52, lon 5, heading 90 degrees).

#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_methods,
    reason = "the orientation test computes its expected angles with the standard library trigonometry, independently of the code under test (ADR 0003 governs the simulation, not this check)"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;

use umbra_net_dis::codec::{
    APPEARANCE_DEACTIVATED, Body, BurstDescriptor, Descriptor, Detonation, EntityId, EntityState,
    EntityType, EventId, Fire, Header, Pdu, StartResume, StopFreeze, decode,
};
use umbra_net_dis::config::Config;
use umbra_net_dis::translate::{
    Inbound, Reject, Skip, Stamp, TokenError, Translator, format_entity_type, key_space,
    marking_of, name_of, pack_key, parse_entity_type, unpack_key,
};
use umbra_net_dis::version::DisVersion;
use umbra_sdk::serde_json::{Value, json};
use umbra_sdk::{ControlAction, DeadReckoning, EntityState as SdkState, Euler, NetKey, NetMessage};

const AT: Stamp = Stamp {
    tick: 0,
    rate_hz: 30,
};

fn config(extra: &Value) -> Config {
    let mut params = json!({"site": 7, "application": 9});
    if let (Some(base), Some(add)) = (params.as_object_mut(), extra.as_object()) {
        base.extend(add.clone());
    }
    Config::parse("dis", &params, &BTreeMap::new()).expect("valid params")
}

fn translator(extra: &Value) -> Translator {
    Translator::new(&config(extra))
}

/// The key of entity `n` of this test's adapter (site 7, application 9).
fn own(n: u16) -> NetKey {
    pack_key(EntityId {
        site: 7,
        application: 9,
        entity: n,
    })
}

fn sdk_state<'a>(key: NetKey) -> SdkState<'a> {
    SdkState {
        key,
        entity_type: Some("1.1.225.1.1.0.0"),
        force: Some("opposing"),
        name: Some("Alpha-1"),
        position: [3_900_000.0, 400_000.0, 5_000_000.0],
        velocity: [10.0, -5.0, 2.0],
        acceleration: [0.5, 0.25, -0.125],
        angular_velocity: [0.01, 0.02, 0.03],
        orientation: Euler {
            yaw: 0.5,
            pitch: -0.25,
            roll: 0.125,
        },
        dead_reckoning: DeadReckoning::Acceleration,
    }
}

fn encode_to_pdu(t: &mut Translator, at: Stamp, m: &NetMessage<'_>) -> (DisVersion, Pdu) {
    let bytes = t.outbound(at, m).expect("sendable");
    decode(&bytes).expect("the adapter's own datagram decodes")
}

fn es_of(pdu: &Pdu) -> &EntityState {
    match &pdu.body {
        Body::EntityState(s) => s,
        other => panic!("expected an Entity State, got {other:?}"),
    }
}

fn pdu(body: Body) -> Pdu {
    Pdu {
        header: Header {
            exercise_id: 1,
            timestamp: 0,
            status: 0,
        },
        body,
    }
}

fn peer(entity: u16) -> EntityId {
    EntityId {
        site: 1,
        application: 2,
        entity,
    }
}

// ------------------------------------------------------------------- keys and the key space

#[test]
fn key_packing_is_site_application_entity() {
    let id = EntityId {
        site: 0x0102,
        application: 0x0304,
        entity: 0x0506,
    };
    assert_eq!(pack_key(id), NetKey(0x0102_0304_0506));
    assert_eq!(unpack_key(NetKey(0x0102_0304_0506)), Some(id));
}

#[test]
fn reserved_entity_numbers_and_wide_keys_do_not_unpack() {
    assert_eq!(unpack_key(NetKey(0x0001_0002_0000)), None, "entity 0");
    assert_eq!(unpack_key(NetKey(0x0001_0002_FFFF)), None, "entity 0xFFFF");
    assert_eq!(unpack_key(NetKey(1 << 48 | 5)), None, "above 48 bits");
    assert_eq!(unpack_key(NetKey(0)), None);
    assert!(
        unpack_key(NetKey(0x0001_0002_FFFE)).is_some(),
        "65534 is the last entity"
    );
    assert!(unpack_key(NetKey(0x0001_0002_0001)).is_some());
}

#[test]
fn key_space_is_the_entities_of_one_site_and_application() {
    let ks = key_space(7, 9).expect("key space");
    assert_eq!(ks.base(), 7 << 32 | 9 << 16);
    assert_eq!(ks.count(), 65_534);
    assert!(!ks.contains(NetKey(7 << 32 | 9 << 16)), "entity 0");
    assert!(ks.contains(own(1)));
    assert!(ks.contains(own(65_534)));
    assert!(
        !ks.contains(NetKey(7 << 32 | 9 << 16 | 0xFFFF)),
        "entity 65535"
    );
    assert!(
        !ks.contains(NetKey(7 << 32 | 10 << 16 | 1)),
        "another application"
    );
    // Every key of the space unpacks to this site and application.
    for entity in [1_u16, 2, 1000, 65_534] {
        let id = unpack_key(own(entity)).expect("in the space");
        assert_eq!((id.site, id.application, id.entity), (7, 9, entity));
    }
}

// --------------------------------------------------------------------------- entity types

#[test]
fn entity_type_tokens_round_trip_and_normalise() {
    let t = parse_entity_type("1.1.225.1.1.0.0").expect("token");
    assert_eq!(
        (
            t.kind,
            t.domain,
            t.country,
            t.category,
            t.subcategory,
            t.specific,
            t.extra
        ),
        (1, 1, 225, 1, 1, 0, 0)
    );
    assert_eq!(format_entity_type(t), "1.1.225.1.1.0.0");
    let padded = parse_entity_type("01.001.0225.1.1.00.0").expect("leading zeros are read");
    assert_eq!(padded, t);
    assert_eq!(
        format_entity_type(padded),
        "1.1.225.1.1.0.0",
        "never written"
    );
    let widest = parse_entity_type("255.255.65535.255.255.255.255").expect("widest");
    assert_eq!(format_entity_type(widest), "255.255.65535.255.255.255.255");
}

#[test]
fn bad_entity_type_tokens_name_the_field() {
    assert_eq!(parse_entity_type("1.1.225"), Err(TokenError::Fields(3)));
    assert_eq!(parse_entity_type(""), Err(TokenError::Fields(1)));
    assert_eq!(
        parse_entity_type("1.1.225.1.1.0.0.0"),
        Err(TokenError::Fields(8))
    );
    for (token, index) in [
        ("a.1.225.1.1.0.0", 1),
        ("1.1.-5.1.1.0.0", 3),
        ("1.1.225.1.+1.0.0", 5),
        ("1. 1.225.1.1.0.0", 2),
        ("1.1.225.1.1.0.", 7),
        ("1.1.0x10.1.1.0.0", 3),
    ] {
        assert!(
            matches!(parse_entity_type(token), Err(TokenError::NotDecimal { index: i, .. }) if i == index),
            "{token}"
        );
    }
    for (token, index, max) in [
        ("256.1.225.1.1.0.0", 1, 255),
        ("1.1.65536.1.1.0.0", 3, 65_535),
        ("1.1.225.1.1.0.256", 7, 255),
        ("1.1.99999999999999999999.1.1.0.0", 3, 65_535),
    ] {
        assert!(
            matches!(parse_entity_type(token), Err(TokenError::Range { index: i, max: m, .. }) if i == index && m == max),
            "{token}"
        );
    }
}

// ------------------------------------------------------------------------ dead reckoning

#[test]
fn every_dead_reckoning_model_maps_both_ways() {
    let table = [
        (DeadReckoning::Static, 1_u8),
        (DeadReckoning::Velocity, 2),
        (DeadReckoning::VelocityRotation, 3),
        (DeadReckoning::AccelerationRotation, 4),
        (DeadReckoning::Acceleration, 5),
        (DeadReckoning::BodyVelocity, 6),
        (DeadReckoning::BodyVelocityRotation, 7),
        (DeadReckoning::BodyAccelerationRotation, 8),
        (DeadReckoning::BodyAcceleration, 9),
    ];
    let mut t = translator(&json!({}));
    for (model, code) in table {
        let mut s = sdk_state(own(1));
        s.dead_reckoning = model;
        let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
        assert_eq!(es_of(&sent).dead_reckoning.algorithm, code, "{model:?} out");
        let Inbound::State(back) = t.inbound(&sent).expect("inbound") else {
            panic!("a state");
        };
        assert_eq!(back.dead_reckoning, model, "code {code} in");
    }
}

#[test]
fn other_reads_as_static_and_reserved_codes_are_counted() {
    let mut t = translator(&json!({}));
    let mut es = EntityState {
        entity_id: peer(1),
        ..EntityState::default()
    };
    es.dead_reckoning.algorithm = 0;
    let Inbound::State(s) = t
        .inbound(&pdu(Body::EntityState(es.clone())))
        .expect("other")
    else {
        panic!("a state");
    };
    assert_eq!(s.dead_reckoning, DeadReckoning::Static);
    assert_eq!(t.tally().reserved_dead_reckoning, 0);
    for code in [10_u8, 77, 255] {
        es.dead_reckoning.algorithm = code;
        let Inbound::State(s) = t
            .inbound(&pdu(Body::EntityState(es.clone())))
            .expect("reserved")
        else {
            panic!("a state");
        };
        assert_eq!(s.dead_reckoning, DeadReckoning::Static, "code {code}");
    }
    assert_eq!(t.tally().reserved_dead_reckoning, 3);
}

// --------------------------------------------------------------------------- forces, marking

#[test]
fn forces_map_both_ways_with_the_default_table() {
    let mut t = translator(&json!({}));
    for (name, id) in [("friendly", 1_u8), ("opposing", 2), ("neutral", 3)] {
        let mut s = sdk_state(own(1));
        s.force = Some(name);
        let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
        assert_eq!(es_of(&sent).force_id, id, "{name} out");
        let Inbound::State(back) = t.inbound(&sent).expect("inbound") else {
            panic!("a state");
        };
        assert_eq!(back.force.as_deref(), Some(name), "id {id} in");
    }
}

#[test]
fn unknown_or_absent_forces_are_other_out_and_none_in() {
    let mut t = translator(&json!({}));
    for force in [None, Some("purple")] {
        let mut s = sdk_state(own(1));
        s.force = force;
        let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
        assert_eq!(es_of(&sent).force_id, 0, "{force:?}");
        let Inbound::State(back) = t.inbound(&sent).expect("inbound") else {
            panic!("a state");
        };
        assert_eq!(back.force, None);
    }
    let es = EntityState {
        entity_id: peer(1),
        force_id: 200,
        ..EntityState::default()
    };
    let Inbound::State(s) = t.inbound(&pdu(Body::EntityState(es))).expect("inbound") else {
        panic!("a state");
    };
    assert_eq!(s.force, None, "an id outside the table");
}

#[test]
fn a_configured_forces_table_replaces_the_default() {
    let mut t = translator(&json!({"forces": {"blue": 1, "red": 2, "green": 9}}));
    let mut s = sdk_state(own(1));
    s.force = Some("green");
    let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
    assert_eq!(es_of(&sent).force_id, 9);
    let mut s = sdk_state(own(1));
    s.force = Some("friendly");
    let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
    assert_eq!(es_of(&sent).force_id, 0, "the default names are gone");
}

#[test]
fn marking_is_ascii_eleven_bytes_nul_padded() {
    let m = marking_of(Some("Alpha-1"));
    assert_eq!(m.character_set, 1);
    assert_eq!(&m.characters, b"Alpha-1\0\0\0\0");
    assert_eq!(marking_of(None).characters, [0; 11]);
    assert_eq!(
        &marking_of(Some("ABCDEFGHIJKLMNOP")).characters,
        b"ABCDEFGHIJK",
        "cut at 11"
    );
    assert_eq!(
        &marking_of(Some("Zoë\u{1F600}x")).characters,
        b"Zo??x\0\0\0\0\0\0"
    );
    assert_eq!(
        &marking_of(Some("a\nb\0c")).characters,
        b"a?b?c\0\0\0\0\0\0",
        "controls"
    );
    assert_eq!(name_of(&m).as_deref(), Some("Alpha-1"));
    assert_eq!(name_of(&marking_of(None)), None, "empty is none");
}

#[test]
fn inbound_marking_stops_at_the_first_nul_and_replaces_bytes_that_are_not_ascii() {
    let mut m = marking_of(None);
    m.characters = *b"ab\0cdefghij";
    assert_eq!(name_of(&m).as_deref(), Some("ab"));
    m.characters = [b'x', 0xE9, 0xFF, 0x07, b'y', 0, 0, 0, 0, 0, 0];
    assert_eq!(name_of(&m).as_deref(), Some("x???y"));
    m.characters = [0, b'a', 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(name_of(&m), None, "a leading NUL is an empty name");
    m.characters = [b'k'; 11];
    assert_eq!(name_of(&m).as_deref(), Some("kkkkkkkkkkk"), "no NUL at all");
}

// ----------------------------------------------------------------------------- timestamps

#[test]
fn timestamps_are_relative_units_of_3600_over_2_pow_31_seconds() {
    // Bit 0 is 0 (relative); the upper 31 bits count units past the hour.
    let at = |tick, rate_hz| Stamp { tick, rate_hz };
    assert_eq!(at(0, 30).timestamp(), 0);
    // One tick at 30 Hz is 1/30 s = 2^31 / 108000 = 19884.1 units.
    assert_eq!(at(1, 30).timestamp(), 19_884 << 1);
    // Half an hour is exactly 2^30 units.
    assert_eq!(at(30 * 1800, 30).timestamp(), 1 << 31);
    // A full hour wraps to 0 and advances the hour of a clock time.
    assert_eq!(at(30 * 3600, 30).timestamp(), 0);
    assert_eq!(at(30 * 3600, 30).clock_time().hour, 1);
    assert_eq!(at(30 * 3600 * 5 + 30 * 1800, 30).clock_time().hour, 5);
    assert_eq!(at(30 * 1800, 30).clock_time().time_past_hour, 1 << 31);
    assert_eq!(
        at(60, 60).timestamp(),
        at(30, 30).timestamp(),
        "depends on seconds, not ticks"
    );
    assert_eq!(at(1, 0).timestamp(), 0, "a zero rate cannot divide by zero");
    assert_eq!(at(u64::MAX, 1).clock_time().hour, i32::MAX, "saturates");
}

#[test]
fn the_header_carries_the_exercise_and_the_timestamp_of_the_batch_tick() {
    let mut t = translator(&json!({"exercise": 42}));
    let (_, sent) = encode_to_pdu(
        &mut t,
        Stamp {
            tick: 90,
            rate_hz: 30,
        },
        &NetMessage::EntityState(sdk_state(own(1))),
    );
    assert_eq!(sent.header.exercise_id, 42);
    assert_eq!(
        sent.header.timestamp,
        Stamp {
            tick: 90,
            rate_hz: 30
        }
        .timestamp()
    );
    assert_eq!(sent.header.timestamp & 1, 0, "relative");
    assert_ne!(sent.header.timestamp, 0);
}

// ----------------------------------------------------------------------------- entity state

#[test]
fn an_entity_state_goes_out_as_the_mapped_pdu_in_both_versions() {
    for (extra, version) in [
        (json!({}), DisVersion::V7),
        (json!({"version": 6}), DisVersion::V6),
    ] {
        let mut t = translator(&extra);
        let (v, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(sdk_state(own(5))));
        assert_eq!(v, version);
        let es = es_of(&sent);
        assert_eq!(
            es.entity_id,
            EntityId {
                site: 7,
                application: 9,
                entity: 5
            }
        );
        assert_eq!(es.force_id, 2);
        assert_eq!(format_entity_type(es.entity_type), "1.1.225.1.1.0.0");
        assert_eq!(es.alternative_entity_type, EntityType::default());
        assert_eq!(es.linear_velocity, [10.0, -5.0, 2.0]);
        assert_eq!(es.location, [3_900_000.0, 400_000.0, 5_000_000.0]);
        assert_eq!(es.orientation, [0.5, -0.25, 0.125]);
        assert_eq!(es.appearance, 0);
        assert_eq!(es.dead_reckoning.algorithm, 5);
        assert_eq!(es.dead_reckoning.linear_acceleration, [0.5, 0.25, -0.125]);
        assert_eq!(es.dead_reckoning.angular_velocity, [0.01, 0.02, 0.03]);
        assert_eq!(name_of(&es.marking).as_deref(), Some("Alpha-1"));
    }
}

#[test]
fn an_entity_state_without_a_type_goes_out_with_the_zero_type_and_back_as_none() {
    let mut t = translator(&json!({}));
    let mut s = sdk_state(own(1));
    s.entity_type = None;
    let (_, sent) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
    assert_eq!(es_of(&sent).entity_type, EntityType::default());
    let Inbound::State(back) = t.inbound(&sent).expect("inbound") else {
        panic!("a state");
    };
    assert_eq!(back.entity_type, None);
}

#[test]
fn a_state_round_trips_through_a_peer() {
    // Our PDU, read by an adapter of another site and application: the key and every field survive.
    let mut sender = translator(&json!({}));
    let mut receiver = Translator::new(
        &Config::parse(
            "dis",
            &json!({"site": 1, "application": 2}),
            &BTreeMap::new(),
        )
        .expect("config"),
    );
    let sent = sdk_state(own(5));
    let (_, wire) = encode_to_pdu(&mut sender, AT, &NetMessage::EntityState(sent));
    let Inbound::State(got) = receiver.inbound(&wire).expect("inbound") else {
        panic!("a state");
    };
    assert_eq!(got.key, own(5));
    assert_eq!(got.entity_type.as_deref(), Some("1.1.225.1.1.0.0"));
    assert_eq!(got.force.as_deref(), Some("opposing"));
    assert_eq!(got.name.as_deref(), Some("Alpha-1"));
    assert_eq!(got.position, sent.position);
    assert_eq!(got.velocity, sent.velocity);
    assert_eq!(got.acceleration, sent.acceleration);
    assert_eq!(got.angular_velocity, [0.01_f32, 0.02, 0.03].map(f64::from));
    assert_eq!(
        got.orientation,
        Euler {
            yaw: 0.5,
            pitch: -0.25,
            roll: 0.125
        }
    );
    assert_eq!(got.dead_reckoning, DeadReckoning::Acceleration);
    assert_eq!(got.key, NetKey(7 << 32 | 9 << 16 | 5));
}

#[test]
fn keys_outside_the_adapters_space_are_not_sent_as_entity_states() {
    let mut t = translator(&json!({}));
    let foreign = [
        pack_key(peer(3)),
        pack_key(EntityId {
            site: 7,
            application: 10,
            entity: 1,
        }),
    ];
    for key in foreign {
        let r = t.outbound(AT, &NetMessage::EntityState(sdk_state(key)));
        assert_eq!(r, Err(Reject::ForeignKey(key.0)));
        let r = t.outbound(AT, &NetMessage::EntityRemoved { key });
        assert_eq!(r, Err(Reject::ForeignKey(key.0)));
    }
    let wide = NetKey(1 << 50);
    assert_eq!(
        t.outbound(AT, &NetMessage::EntityState(sdk_state(wide))),
        Err(Reject::Key(wide.0))
    );
    let reserved = NetKey(7 << 32 | 9 << 16);
    assert_eq!(
        t.outbound(AT, &NetMessage::EntityState(sdk_state(reserved))),
        Err(Reject::Key(reserved.0))
    );
}

#[test]
fn values_that_are_not_finite_on_the_wire_are_refused_not_sent() {
    let mut t = translator(&json!({}));
    let mut fast = sdk_state(own(1));
    fast.velocity = [1.0e40, 0.0, 0.0];
    assert_eq!(
        t.outbound(AT, &NetMessage::EntityState(fast)),
        Err(Reject::NotFinite("velocity"))
    );
    let mut spin = sdk_state(own(1));
    spin.orientation.yaw = f64::INFINITY;
    assert_eq!(
        t.outbound(AT, &NetMessage::EntityState(spin)),
        Err(Reject::NotFinite("orientation"))
    );
    let mut accel = sdk_state(own(1));
    accel.acceleration = [0.0, f64::NAN, 0.0];
    assert!(matches!(
        t.outbound(AT, &NetMessage::EntityState(accel)),
        Err(Reject::NotFinite("acceleration"))
    ));
    let mut ok = sdk_state(own(1));
    ok.velocity = [3.0e38, 0.0, 0.0];
    assert!(
        t.outbound(AT, &NetMessage::EntityState(ok)).is_ok(),
        "f32::MAX range is fine"
    );
}

#[test]
fn inbound_values_that_are_not_finite_are_skipped() {
    let mut t = translator(&json!({}));
    let base = EntityState {
        entity_id: peer(1),
        ..EntityState::default()
    };
    let mut cases = Vec::new();
    let mut e = base.clone();
    e.location[1] = f64::NAN;
    cases.push(e);
    let mut e = base.clone();
    e.linear_velocity[0] = f32::INFINITY;
    cases.push(e);
    let mut e = base.clone();
    e.orientation[2] = f32::NAN;
    cases.push(e);
    let mut e = base.clone();
    e.dead_reckoning.linear_acceleration[1] = f32::NEG_INFINITY;
    cases.push(e);
    let mut e = base.clone();
    e.dead_reckoning.angular_velocity[2] = f32::NAN;
    cases.push(e);
    for e in cases {
        assert_eq!(t.inbound(&pdu(Body::EntityState(e))), Err(Skip::NotFinite));
    }
    assert!(t.inbound(&pdu(Body::EntityState(base))).is_ok());
}

#[test]
fn inbound_reserved_entity_numbers_are_skipped_not_keyed() {
    let mut t = translator(&json!({}));
    for entity in [0_u16, 0xFFFF] {
        let es = EntityState {
            entity_id: peer(entity),
            ..EntityState::default()
        };
        assert_eq!(t.inbound(&pdu(Body::EntityState(es))), Err(Skip::EntityId));
    }
}

#[test]
fn the_exercise_filter_drops_other_exercises() {
    let mut t = translator(&json!({"exercise": 3}));
    let es = Body::EntityState(EntityState {
        entity_id: peer(1),
        ..EntityState::default()
    });
    let mut other = pdu(es.clone());
    other.header.exercise_id = 4;
    assert_eq!(t.inbound(&other), Err(Skip::Exercise));
    let mut mine = pdu(es);
    mine.header.exercise_id = 3;
    assert!(t.inbound(&mine).is_ok());
    // The filter covers every family.
    for body in [
        Body::Fire(Fire::default()),
        Body::Detonation(Detonation::default()),
        Body::StartResume(StartResume::default()),
        Body::StopFreeze(StopFreeze::default()),
    ] {
        let mut p = pdu(body);
        p.header.exercise_id = 9;
        assert_eq!(t.inbound(&p), Err(Skip::Exercise));
    }
}

// --------------------------------------------------------------------------------- removal

#[test]
fn a_removal_is_the_last_state_with_the_deactivated_bit() {
    let mut t = translator(&json!({}));
    let (_, live) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(sdk_state(own(5))));
    let (_, gone) = encode_to_pdu(&mut t, AT, &NetMessage::EntityRemoved { key: own(5) });
    let (live, gone) = (es_of(&live), es_of(&gone));
    assert_eq!(
        gone.appearance & APPEARANCE_DEACTIVATED,
        APPEARANCE_DEACTIVATED
    );
    assert!(gone.is_deactivated());
    assert_eq!(live.appearance, 0);
    // Everything but the appearance is what was last published.
    let mut as_live = gone.clone();
    as_live.appearance = 0;
    assert_eq!(&as_live, live);
}

#[test]
fn a_removal_of_an_entity_never_published_is_an_empty_deactivated_state() {
    let mut t = translator(&json!({}));
    let (_, gone) = encode_to_pdu(&mut t, AT, &NetMessage::EntityRemoved { key: own(8) });
    let gone = es_of(&gone);
    assert_eq!(gone.entity_id.entity, 8);
    assert!(gone.is_deactivated());
    assert_eq!(gone.force_id, 0);
    assert_eq!(gone.entity_type, EntityType::default());
}

#[test]
fn a_second_removal_does_not_resurrect_the_old_state() {
    let mut t = translator(&json!({}));
    let _ = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(sdk_state(own(5))));
    let _ = encode_to_pdu(&mut t, AT, &NetMessage::EntityRemoved { key: own(5) });
    let (_, again) = encode_to_pdu(&mut t, AT, &NetMessage::EntityRemoved { key: own(5) });
    assert_eq!(es_of(&again).location, [0.0; 3], "the state was forgotten");
}

#[test]
fn an_inbound_deactivated_state_is_a_removal() {
    let mut t = translator(&json!({}));
    let es = EntityState {
        entity_id: peer(6),
        appearance: APPEARANCE_DEACTIVATED | 0x1234,
        ..EntityState::default()
    };
    assert_eq!(
        t.inbound(&pdu(Body::EntityState(es))),
        Ok(Inbound::Removed(pack_key(peer(6))))
    );
}

// ------------------------------------------------------------------------- fire, detonation

fn weapons() -> Value {
    json!({"weapons": {"acme.cannon_120": "2.1.225.1.1.0.0", "acme.cannon_120b": "2.1.225.1.1.0.0", "acme.rocket": "2.2.225.3.0.0.0"}})
}

#[test]
fn a_fire_goes_out_with_the_munition_type_of_the_weapons_table() {
    let mut t = translator(&weapons());
    let (_, sent) = encode_to_pdu(
        &mut t,
        AT,
        &NetMessage::Fire {
            shooter: own(5),
            target: Some(pack_key(peer(3))),
            weapon: "acme.rocket",
            position: [1.0, 2.0, 3.0],
        },
    );
    let Body::Fire(f) = &sent.body else {
        panic!("a Fire");
    };
    assert_eq!(f.firing_entity.entity, 5);
    assert_eq!(f.target_entity, peer(3));
    assert_eq!(f.location, [1.0, 2.0, 3.0]);
    let Descriptor::Munition(m) = f.descriptor else {
        panic!("the munition form");
    };
    assert_eq!(format_entity_type(m.munition), "2.2.225.3.0.0.0");
    assert_eq!((m.warhead, m.fuse, m.quantity, m.rate), (0, 0, 1, 0));
    assert_eq!(sent.header.status & 0x30, 0, "munition type indicator");
}

#[test]
fn a_fire_without_target_or_weapon_entry_is_sent_with_zero_ids() {
    let mut t = translator(&weapons());
    let (_, sent) = encode_to_pdu(
        &mut t,
        AT,
        &NetMessage::Fire {
            shooter: own(5),
            target: None,
            weapon: "other.unlisted",
            position: [0.0; 3],
        },
    );
    let Body::Fire(f) = &sent.body else {
        panic!("a Fire");
    };
    assert_eq!(f.target_entity, EntityId::default());
    assert_eq!(
        f.descriptor,
        Descriptor::Munition(BurstDescriptor {
            quantity: 1,
            ..BurstDescriptor::default()
        })
    );
}

#[test]
fn event_ids_count_up_from_one_and_wrap_past_zero() {
    let mut t = translator(&weapons());
    let fire = |t: &mut Translator| -> EventId {
        let (_, sent) = encode_to_pdu(
            t,
            AT,
            &NetMessage::Fire {
                shooter: own(5),
                target: None,
                weapon: "acme.rocket",
                position: [0.0; 3],
            },
        );
        let Body::Fire(f) = sent.body else {
            panic!("a Fire");
        };
        f.event
    };
    let first = fire(&mut t);
    assert_eq!(
        first,
        EventId {
            site: 7,
            application: 9,
            event: 1
        }
    );
    assert_eq!(fire(&mut t).event, 2);
    // Detonations draw from the same counter.
    let (_, det) = encode_to_pdu(
        &mut t,
        AT,
        &NetMessage::Detonation {
            shooter: None,
            target: None,
            position: [0.0; 3],
        },
    );
    let Body::Detonation(d) = det.body else {
        panic!("a Detonation");
    };
    assert_eq!(d.event.event, 3);
    for _ in 4..65_535 {
        fire(&mut t);
    }
    assert_eq!(fire(&mut t).event, 65_535);
    assert_eq!(fire(&mut t).event, 1, "0 is skipped");
}

#[test]
fn a_detonation_goes_out_as_a_detonation_result_at_the_position() {
    let mut t = translator(&weapons());
    let (_, sent) = encode_to_pdu(
        &mut t,
        AT,
        &NetMessage::Detonation {
            shooter: Some(own(5)),
            target: Some(pack_key(peer(3))),
            position: [4.0, 5.0, 6.0],
        },
    );
    let Body::Detonation(d) = &sent.body else {
        panic!("a Detonation");
    };
    assert_eq!(d.firing_entity.entity, 5);
    assert_eq!(d.target_entity, peer(3));
    assert_eq!(d.location, [4.0, 5.0, 6.0]);
    assert_eq!(d.result, 5);
    assert_eq!(
        d.descriptor,
        Descriptor::Munition(BurstDescriptor::default())
    );
}

#[test]
fn an_inbound_fire_maps_the_munition_to_the_smallest_weapon_id() {
    let mut t = translator(&weapons());
    let fire = Fire {
        firing_entity: peer(4),
        target_entity: peer(5),
        location: [7.0, 8.0, 9.0],
        descriptor: Descriptor::Munition(BurstDescriptor {
            munition: parse_entity_type("2.1.225.1.1.0.0").expect("token"),
            ..BurstDescriptor::default()
        }),
        ..Fire::default()
    };
    assert_eq!(
        t.inbound(&pdu(Body::Fire(fire.clone()))),
        Ok(Inbound::Fire {
            shooter: pack_key(peer(4)),
            target: Some(pack_key(peer(5))),
            weapon: "acme.cannon_120".to_owned(),
            position: [7.0, 8.0, 9.0],
        }),
        "two weapons share the type: the smaller id wins"
    );
    let mut no_target = fire;
    no_target.target_entity = EntityId::default();
    let Ok(Inbound::Fire { target, .. }) = t.inbound(&pdu(Body::Fire(no_target))) else {
        panic!("a fire");
    };
    assert_eq!(target, None);
}

#[test]
fn an_inbound_fire_of_an_unlisted_munition_or_without_shooter_is_skipped() {
    let mut t = translator(&weapons());
    let unlisted = Fire {
        firing_entity: peer(4),
        descriptor: Descriptor::Munition(BurstDescriptor {
            munition: parse_entity_type("2.9.9.9.9.9.9").expect("token"),
            ..BurstDescriptor::default()
        }),
        ..Fire::default()
    };
    assert_eq!(
        t.inbound(&pdu(Body::Fire(unlisted))),
        Err(Skip::UnmappedWeapon)
    );
    let no_shooter = Fire {
        descriptor: Descriptor::Munition(BurstDescriptor {
            munition: parse_entity_type("2.2.225.3.0.0.0").expect("token"),
            ..BurstDescriptor::default()
        }),
        ..Fire::default()
    };
    assert_eq!(t.inbound(&pdu(Body::Fire(no_shooter))), Err(Skip::EntityId));
    // No weapons table at all: every Fire is unmapped.
    let mut bare = translator(&json!({}));
    let any = Fire {
        firing_entity: peer(4),
        ..Fire::default()
    };
    assert_eq!(
        bare.inbound(&pdu(Body::Fire(any))),
        Err(Skip::UnmappedWeapon)
    );
}

#[test]
fn an_expendable_fire_is_looked_up_by_its_type_like_a_munition() {
    let mut t = translator(&json!({"weapons": {"acme.chaff": "3.1.225.1.0.0.0"}}));
    let mut fire = pdu(Body::Fire(Fire {
        firing_entity: peer(4),
        descriptor: Descriptor::Expendable {
            entity_type: parse_entity_type("3.1.225.1.0.0.0").expect("token"),
        },
        ..Fire::default()
    }));
    fire.header.status = 0x10; // Fire Type Indicator: expendable
    let Ok(Inbound::Fire { weapon, .. }) = t.inbound(&fire) else {
        panic!("a fire");
    };
    assert_eq!(weapon, "acme.chaff");
}

#[test]
fn every_detonation_descriptor_form_is_a_weapon_effect_at_its_location() {
    let mut t = translator(&weapons());
    let forms = [
        Descriptor::Munition(BurstDescriptor {
            munition: parse_entity_type("2.1.225.1.1.0.0").expect("token"),
            warhead: 4000,
            fuse: 1000,
            quantity: 5,
            rate: 600,
        }),
        Descriptor::Expendable {
            entity_type: parse_entity_type("3.1.225.1.0.0.0").expect("token"),
        },
        Descriptor::Explosion {
            entity_type: parse_entity_type("5.0.0.0.0.0.0").expect("token"),
            explosive_material: 1100,
            explosive_force: 250.5,
        },
    ];
    for descriptor in forms {
        let det = Detonation {
            firing_entity: peer(4),
            target_entity: peer(9),
            location: [10.0, 20.0, 30.0],
            descriptor,
            result: 5,
            ..Detonation::default()
        };
        assert_eq!(
            t.inbound(&pdu(Body::Detonation(det))),
            Ok(Inbound::Detonation {
                shooter: Some(pack_key(peer(4))),
                target: Some(pack_key(peer(9))),
                position: [10.0, 20.0, 30.0],
            }),
            "{descriptor:?}"
        );
    }
}

#[test]
fn a_detonation_without_ids_has_no_shooter_or_target() {
    let mut t = translator(&json!({}));
    let det = Detonation {
        location: [1.0, 1.0, 1.0],
        firing_entity: peer(0),
        target_entity: peer(0xFFFF),
        ..Detonation::default()
    };
    assert_eq!(
        t.inbound(&pdu(Body::Detonation(det))),
        Ok(Inbound::Detonation {
            shooter: None,
            target: None,
            position: [1.0, 1.0, 1.0]
        })
    );
    let nan = Detonation {
        location: [f64::NAN, 0.0, 0.0],
        ..Detonation::default()
    };
    assert_eq!(t.inbound(&pdu(Body::Detonation(nan))), Err(Skip::NotFinite));
}

// ---------------------------------------------------------------------------- run control

#[test]
fn run_control_goes_out_as_start_resume_and_stop_freeze() {
    let mut t = translator(&json!({}));
    let clock = Stamp {
        tick: 30 * 3600 + 30,
        rate_hz: 30,
    };
    for action in [ControlAction::Start, ControlAction::Resume] {
        let (_, sent) = encode_to_pdu(&mut t, clock, &NetMessage::SimulationControl(action));
        let Body::StartResume(s) = &sent.body else {
            panic!("a Start/Resume for {action:?}");
        };
        assert_eq!(
            s.originating_entity,
            EntityId {
                site: 7,
                application: 9,
                entity: 0
            }
        );
        assert_eq!(
            s.receiving_entity,
            EntityId {
                site: 0xFFFF,
                application: 0xFFFF,
                entity: 0xFFFF
            }
        );
        assert_eq!(s.real_world_time, clock.clock_time());
        assert_eq!(s.simulation_time, clock.clock_time());
        assert_eq!(s.real_world_time.hour, 1);
    }
    for (action, reason) in [(ControlAction::Pause, 2_u8), (ControlAction::Stop, 3)] {
        let (_, sent) = encode_to_pdu(&mut t, clock, &NetMessage::SimulationControl(action));
        let Body::StopFreeze(s) = &sent.body else {
            panic!("a Stop/Freeze for {action:?}");
        };
        assert_eq!(s.reason, reason, "{action:?}");
        assert_eq!(s.real_world_time, clock.clock_time());
    }
}

#[test]
fn request_ids_count_up_per_translator() {
    let mut a = translator(&json!({}));
    let mut b = translator(&json!({}));
    let request = |t: &mut Translator| {
        let (_, sent) = encode_to_pdu(t, AT, &NetMessage::SimulationControl(ControlAction::Start));
        let Body::StartResume(s) = sent.body else {
            panic!("a Start/Resume");
        };
        s.request_id
    };
    assert_eq!(
        [request(&mut a), request(&mut a), request(&mut a)],
        [1, 2, 3]
    );
    assert_eq!(request(&mut b), 1, "no state is shared between instances");
}

#[test]
fn inbound_run_control_maps_to_actions() {
    let mut t = translator(&json!({}));
    assert_eq!(
        t.inbound(&pdu(Body::StartResume(StartResume::default()))),
        Ok(Inbound::Control(ControlAction::Start))
    );
    for (reason, action) in [
        (2_u8, ControlAction::Pause),
        (3, ControlAction::Stop),
        (1, ControlAction::Stop),
        (9, ControlAction::Stop),
    ] {
        let stop = StopFreeze {
            reason,
            ..StopFreeze::default()
        };
        assert_eq!(
            t.inbound(&pdu(Body::StopFreeze(stop))),
            Ok(Inbound::Control(action)),
            "reason {reason}"
        );
    }
}

// ---------------------------------------------------------------------------- orientation

/// The local north, east and down unit vectors in ECEF at geodetic latitude and longitude (the
/// standard ENU/NED basis, computed here from the angles alone).
fn ned_basis(lat_deg: f64, lon_deg: f64) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let north = [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()];
    let east = [-lon.sin(), lon.cos(), 0.0];
    let down = [-lat.cos() * lon.cos(), -lat.cos() * lon.sin(), -lat.sin()];
    (north, east, down)
}

/// WGS-84 geodetic to ECEF.
fn ecef(lat_deg: f64, lon_deg: f64, h: f64) -> [f64; 3] {
    let (a, f) = (6_378_137.0_f64, 1.0 / 298.257_223_563);
    let e2 = f * (2.0 - f);
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    [
        (n + h) * lat.cos() * lon.cos(),
        (n + h) * lat.cos() * lon.sin(),
        (n * (1.0 - e2) + h) * lat.sin(),
    ]
}

type Mat = [[f64; 3]; 3];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// `Rz(psi) * Ry(theta) * Rx(phi)`: the body to world rotation of the z-y'-x'' sequence.
fn zyx(psi: f64, theta: f64, phi: f64) -> Mat {
    let rz = [
        [psi.cos(), -psi.sin(), 0.0],
        [psi.sin(), psi.cos(), 0.0],
        [0.0, 0.0, 1.0],
    ];
    let ry = [
        [theta.cos(), 0.0, theta.sin()],
        [0.0, 1.0, 0.0],
        [-theta.sin(), 0.0, theta.cos()],
    ];
    let rx = [
        [1.0, 0.0, 0.0],
        [0.0, phi.cos(), -phi.sin()],
        [0.0, phi.sin(), phi.cos()],
    ];
    mul(&mul(&rz, &ry), &rx)
}

#[test]
fn heading_east_at_lat_52_lon_5_round_trips_through_the_world_frame_euler_angles() {
    let (lat, lon) = (52.0, 5.0);
    // Body axes of a level entity heading east (FRD): forward = east, right = south, down = down.
    let (north, east, down) = ned_basis(lat, lon);
    let south = north.map(|c| -c);
    let columns = [east, south, down];
    // The z-y-x Euler angles of that rotation, worked out by hand: psi = 90 + lon (the forward axis
    // lies at azimuth 90 + lon from ECEF x in the equatorial plane), theta = 0 (forward is
    // horizontal in the equatorial plane), phi = -(90 + lat) (the down axis tilts from ECEF -z).
    let (psi, theta, phi) = (
        (90.0 + lon).to_radians(),
        0.0_f64,
        (-(90.0 + lat)).to_radians(),
    );
    // The angles reproduce the basis: this is what makes them *the* world-frame angles of the
    // pose, independent of the translator.
    let r = zyx(psi, theta, phi);
    for (j, col) in columns.iter().enumerate() {
        for (i, expected) in col.iter().enumerate() {
            assert!((r[i][j] - expected).abs() < 1e-12, "R[{i}][{j}]");
        }
    }

    let mut t = translator(&json!({}));
    let mut s = sdk_state(own(1));
    s.position = ecef(lat, lon, 100.0);
    s.orientation = Euler {
        yaw: psi,
        pitch: theta,
        roll: phi,
    };
    let (_, wire) = encode_to_pdu(&mut t, AT, &NetMessage::EntityState(s));
    let angles = es_of(&wire).orientation.map(f64::from);
    // On the wire: psi, theta, phi, the expected values to f32 precision.
    for (got, want) in angles.iter().zip([psi, theta, phi]) {
        assert!((got - want).abs() < 1e-6, "{angles:?}");
    }
    // The wire angles still describe the pose heading east.
    let wire_r = zyx(angles[0], angles[1], angles[2]);
    for (j, col) in columns.iter().enumerate() {
        for (i, expected) in col.iter().enumerate() {
            assert!((wire_r[i][j] - expected).abs() < 1e-6, "wire R[{i}][{j}]");
        }
    }
    // And back.
    let Inbound::State(back) = t.inbound(&wire).expect("inbound") else {
        panic!("a state");
    };
    assert!((back.orientation.yaw - psi).abs() < 1e-6);
    assert!(back.orientation.pitch.abs() < 1e-6);
    assert!((back.orientation.roll - phi).abs() < 1e-6);
    assert!(
        (back.position[2] - s.position[2]).abs() < 1e-9,
        "the position is copied"
    );
}

// ------------------------------------------------------------ own run control and own shooters

#[test]
fn run_control_that_this_adapter_sent_is_dropped_when_it_comes_back() {
    // Our own Start/Resume and Stop/Freeze, looped back by broadcast or multicast.
    let mut t = translator(&json!({}));
    for action in [
        ControlAction::Start,
        ControlAction::Resume,
        ControlAction::Pause,
        ControlAction::Stop,
    ] {
        let (_, wire) = encode_to_pdu(&mut t, AT, &NetMessage::SimulationControl(action));
        assert_eq!(t.inbound(&wire), Err(Skip::OwnControl), "{action:?}");
    }
    // The same PDUs from another application of the same site, or another site, are real control.
    let mut peer = Translator::new(
        &Config::parse(
            "dis",
            &json!({"site": 7, "application": 10}),
            &BTreeMap::new(),
        )
        .expect("config"),
    );
    let (_, wire) = encode_to_pdu(
        &mut peer,
        AT,
        &NetMessage::SimulationControl(ControlAction::Pause),
    );
    assert_eq!(t.inbound(&wire), Ok(Inbound::Control(ControlAction::Pause)));
    let mut other_site = Translator::new(
        &Config::parse(
            "dis",
            &json!({"site": 8, "application": 9}),
            &BTreeMap::new(),
        )
        .expect("config"),
    );
    let (_, wire) = encode_to_pdu(
        &mut other_site,
        AT,
        &NetMessage::SimulationControl(ControlAction::Start),
    );
    assert_eq!(t.inbound(&wire), Ok(Inbound::Control(ControlAction::Start)));
}

#[test]
fn a_fire_or_detonation_by_a_shooter_outside_the_key_space_is_not_sent() {
    let mut t = translator(&weapons());
    let foreign = pack_key(peer(3));
    assert_eq!(
        t.outbound(
            AT,
            &NetMessage::Fire {
                shooter: foreign,
                target: None,
                weapon: "acme.rocket",
                position: [0.0; 3]
            }
        ),
        Err(Reject::ForeignKey(foreign.0))
    );
    assert_eq!(
        t.outbound(
            AT,
            &NetMessage::Detonation {
                shooter: Some(foreign),
                target: None,
                position: [0.0; 3]
            }
        ),
        Err(Reject::ForeignKey(foreign.0))
    );
    // A detonation without a shooter is fine, and so is an own shooter.
    for shooter in [None, Some(own(5))] {
        let m = NetMessage::Detonation {
            shooter,
            target: Some(foreign),
            position: [0.0; 3],
        };
        assert!(t.outbound(AT, &m).is_ok(), "{shooter:?}");
    }
    // A rejected message used no event ID.
    let (_, ok) = encode_to_pdu(
        &mut t,
        AT,
        &NetMessage::Fire {
            shooter: own(5),
            target: None,
            weapon: "acme.rocket",
            position: [0.0; 3],
        },
    );
    let Body::Fire(f) = ok.body else {
        panic!("a Fire")
    };
    assert_eq!(f.event.event, 3, "two detonations were sent before it");
}
