//! AT-3 (convert rows): intents and values cross the ABI faithfully. Requirements: REQ-SDK-1,
//! REQ-P-6, ADR 0016, ADR 0019 (value checks). Step 4 of M4 (WI-7 stub).
#![expect(unsafe_code, reason = "plays the host side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::ref_as_ptr,
    reason = "test harness casts small ABI sizes and plays the C side"
)]

mod support;

use std::mem::size_of;

use support::{Out, Rig, err_sink, pushed, reset, s, value_sink, values, vtable};
use umbra_sdk::abi::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_INTENT_CUSTOM, UMBRA_INTENT_DESPAWN,
    UMBRA_INTENT_FIRE, UMBRA_INTENT_FOLLOW_ROUTE, UMBRA_INTENT_HOLD, UMBRA_INTENT_MOVE_TO,
    UMBRA_INTENT_SET_HEADING, UMBRA_INTENT_SET_SPEED, UMBRA_INTENT_SPAWN, UMBRA_OK,
    UMBRA_VALUE_BOOL, UMBRA_VALUE_ENTITY, UMBRA_VALUE_INTEGER, UMBRA_VALUE_NUMBER,
    UMBRA_VALUE_POSITION, UMBRA_VALUE_STRING, UmbraIntent, UmbraStr, UmbraValue, UmbraVec3,
};

/// ADR 0016 table: each SDK `Intent` variant becomes the ABI kind and fields the table names.
#[test]
fn every_intent_variant_converts() {
    reset();
    let mut rig = Rig::new(&[(1, 11)], "{}");

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_OK);
    let got = pushed();
    let kinds: Vec<u32> = got.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            UMBRA_INTENT_MOVE_TO,
            UMBRA_INTENT_FOLLOW_ROUTE,
            UMBRA_INTENT_SET_SPEED,
            UMBRA_INTENT_SET_HEADING,
            UMBRA_INTENT_HOLD,
            UMBRA_INTENT_FIRE,
            UMBRA_INTENT_SPAWN,
            UMBRA_INTENT_DESPAWN,
            UMBRA_INTENT_CUSTOM,
        ]
    );
    assert_eq!(got[0].position, [1.0, 2.0, 3.0]);
    assert!((got[2].value - 4.5).abs() < f64::EPSILON);
    assert!((got[3].value - 0.25).abs() < f64::EPSILON);
    assert_eq!((got[5].entity, got[5].id.as_str()), (7, "test.weapon.gun"));
    assert_eq!(got[6].id, "test.template.unit");
    assert_eq!(got[6].position, [9.0, 8.0, 7.0]);
    assert!(
        (got[6].value - 1.5).abs() < f64::EPSILON,
        "spawn heading travels in value"
    );
    assert_eq!(
        (got[8].id.as_str(), got[8].payload.as_str()),
        ("test.component.note", "{\"n\":1}")
    );
    for p in &got {
        assert_eq!(
            p.abi_version, UMBRA_ABI_VERSION,
            "the SDK writes its own minor"
        );
        assert_eq!(p.struct_size as usize, size_of::<UmbraIntent>());
        assert_eq!(p.item, 0);
    }
}

fn value(ty: u32) -> UmbraValue {
    UmbraValue {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<UmbraValue>() as u32,
        ty,
        boolean: 0,
        reserved_0: [0; 3],
        number: 0.0,
        integer: 0,
        string: UmbraStr {
            ptr: std::ptr::null(),
            len: 0,
        },
        entity: 0,
        position: UmbraVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    }
}

fn set_input(rig: &Rig, v: Option<&UmbraValue>) -> i32 {
    let err = err_sink();
    let p = v.map_or(std::ptr::null(), |v| v as *const UmbraValue);
    // SAFETY: live instance; valid strings, value and sink.
    unsafe { vtable().set_input.expect("set_input")(rig.items[0].instance, s("echo"), p, &err) }
}

fn output(rig: &Rig) -> i32 {
    let sink = value_sink();
    // SAFETY: live instance; valid port and sink.
    unsafe { vtable().output.expect("output")(rig.items[0].instance, s("echo"), &sink) }
}

/// REQ-P-6: every value type goes in through `set_input` and comes out of `output` unchanged.
#[test]
fn values_round_trip() {
    reset();
    let rig = Rig::new(&[(1, 11)], "{}");
    let mut boolean = value(UMBRA_VALUE_BOOL);
    boolean.boolean = 1;
    let mut number = value(UMBRA_VALUE_NUMBER);
    number.number = -2.5;
    let mut integer = value(UMBRA_VALUE_INTEGER);
    integer.integer = i64::MIN;
    let text = "hello \u{e9}";
    let mut string = value(UMBRA_VALUE_STRING);
    string.string = s(text);
    let mut entity = value(UMBRA_VALUE_ENTITY);
    entity.entity = 42;
    let mut position = value(UMBRA_VALUE_POSITION);
    position.position = UmbraVec3 {
        x: 1.0,
        y: -2.0,
        z: 3.5,
    };

    for v in [&boolean, &number, &integer, &string, &entity, &position] {
        assert_eq!(set_input(&rig, Some(v)), UMBRA_OK);
        assert_eq!(output(&rig), UMBRA_OK);
    }

    assert_eq!(
        values(),
        [
            Out::Bool(1),
            Out::Number(-2.5),
            Out::Integer(i64::MIN),
            Out::Text(text.to_owned()),
            Out::Entity(42),
            Out::Position([1.0, -2.0, 3.5]),
        ]
    );
}

/// ADR 0019: a null value unsets the input; `output` then calls the sink once with null.
#[test]
fn null_value_unsets_and_output_reports_unset_once() {
    reset();
    let rig = Rig::new(&[(1, 11)], "{}");
    let mut n = value(UMBRA_VALUE_NUMBER);
    n.number = 1.0;
    assert_eq!(set_input(&rig, Some(&n)), UMBRA_OK);

    assert_eq!(set_input(&rig, None), UMBRA_OK);
    assert_eq!(output(&rig), UMBRA_OK);

    assert_eq!(values(), [Out::Unset]);
}

/// ADR 0019 value checks: a value that breaks the `BbValue` invariants is refused before the trait.
#[test]
fn invalid_values_are_rejected_before_the_trait() {
    reset();
    let rig = Rig::new(&[(1, 11)], "{}");
    let mut bad_tag = value(99);
    bad_tag.number = 1.0;
    let mut bad_bool = value(UMBRA_VALUE_BOOL);
    bad_bool.boolean = 2;
    let mut nan = value(UMBRA_VALUE_NUMBER);
    nan.number = f64::NAN;
    let zero_entity = value(UMBRA_VALUE_ENTITY);
    let mut bad_text = value(UMBRA_VALUE_STRING);
    static BAD: [u8; 2] = [0xFF, 0xFE];
    bad_text.string = UmbraStr {
        ptr: BAD.as_ptr(),
        len: 2,
    };
    let mut inf_pos = value(UMBRA_VALUE_POSITION);
    inf_pos.position.x = f64::INFINITY;

    for (name, v) in [
        ("unknown tag", &bad_tag),
        ("boolean 2", &bad_bool),
        ("non-finite number", &nan),
        ("entity 0", &zero_entity),
        ("invalid UTF-8", &bad_text),
        ("non-finite position", &inf_pos),
    ] {
        assert_eq!(
            set_input(&rig, Some(v)),
            UMBRA_ERR_INVALID_ARGUMENT,
            "{name}"
        );
    }
    assert_eq!(output(&rig), UMBRA_OK);
    assert_eq!(values(), [Out::Unset], "nothing was stored");
}

fn contact(entity: u64, force: &'static str, range_m: f64) -> umbra_sdk::abi::UmbraContact {
    umbra_sdk::abi::UmbraContact {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: size_of::<umbra_sdk::abi::UmbraContact>() as u32,
        entity,
        position: UmbraVec3 {
            x: 10.0,
            y: 20.0,
            z: 30.0,
        },
        velocity: UmbraVec3 {
            x: 1.0,
            y: 0.0,
            z: -1.0,
        },
        force: s(force),
        range_m,
    }
}

/// REQ-P-6 (WI-7): the entity view a tick sees equals the host's view: kinematics, force, route
/// (waypoints, next index, complete) and contacts in order; empty forces are `None`.
#[test]
fn entity_view_converts_route_and_contacts() {
    reset();
    let mut rig = Rig::new(&[(5, 55)], "{}");
    let route = [
        UmbraVec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        UmbraVec3 {
            x: 4.0,
            y: 5.0,
            z: 6.0,
        },
    ];
    let contacts = [contact(8, "red", 120.5), contact(9, "", 300.0)];
    let v = &mut rig.views[0];
    v.force = s("blue");
    v.velocity = UmbraVec3 {
        x: 0.5,
        y: 0.25,
        z: 0.0,
    };
    v.orientation = umbra_sdk::abi::UmbraQuat {
        w: 0.5,
        x: 0.5,
        y: -0.5,
        z: 0.5,
    };
    v.has_route = 1;
    v.route_complete = 0;
    v.route_next = 1;
    v.route = route.as_ptr();
    v.route_len = route.len();
    v.contacts = contacts.as_ptr();
    v.contact_count = contacts.len();

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_OK);
    let got = support::seen();
    assert_eq!(got.len(), 1);
    let seen = &got[0];
    assert_eq!(seen.id, 5);
    assert_eq!(seen.position, [1.0, 2.0, 3.0]);
    assert_eq!(seen.velocity, [0.5, 0.25, 0.0]);
    assert_eq!(seen.orientation, [0.5, 0.5, -0.5, 0.5]);
    assert_eq!(seen.force.as_deref(), Some("blue"));
    assert_eq!(
        seen.route,
        Some((vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]], 1, false))
    );
    assert_eq!(
        seen.contacts,
        [
            (
                8,
                [10.0, 20.0, 30.0],
                [1.0, 0.0, -1.0],
                Some("red".to_owned()),
                120.5
            ),
            (9, [10.0, 20.0, 30.0], [1.0, 0.0, -1.0], None, 300.0),
        ]
    );
}

/// REQ-P-6 (WI-7): `has_route == 0` is "no route", whatever the arrays say; no force is `None`.
#[test]
fn entity_without_route_or_force_has_none() {
    reset();
    let mut rig = Rig::new(&[(4, 44)], "{}");

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_OK);
    let seen = &support::seen()[0];
    assert_eq!((seen.force.clone(), seen.route.clone()), (None, None));
    assert_eq!(seen.contacts, Vec::new());
}
