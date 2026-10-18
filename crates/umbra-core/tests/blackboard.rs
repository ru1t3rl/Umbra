//! M3 WI-3: blackboard, `IntentBuffer`, `Status`/`BbValue` basics.
//! Requirements: REQ-B-1, REQ-B-5, REQ-B-11, ADR 0013, ADR 0014, ASM-071.

#![forbid(unsafe_code)]

use umbra_core::{
    BbType, BbValue, Blackboard, BlackboardError, Ecef, EntityId, Intent, IntentBuffer, Vec3,
};

fn pos(x: f64) -> Ecef {
    Ecef(Vec3 { x, y: 0.0, z: 0.0 })
}

fn entity(n: u64) -> EntityId {
    EntityId::new(n).unwrap_or_else(|| unreachable!("non-zero literal"))
}

/// ASM-071: declared keys start unset.
#[test]
fn given_declared_key_when_read_then_is_unset() {
    let mut bb = Blackboard::default();
    bb.declare("target", BbType::Entity);

    assert_eq!(bb.get("target"), None);
}

/// ASM-071: set then get returns the value; unset clears it.
#[test]
fn given_declared_key_when_set_get_unset_then_round_trips() {
    let mut bb = Blackboard::default();
    bb.declare("n", BbType::Integer);

    assert_eq!(bb.set("n", BbValue::Integer(4)), Ok(()));
    assert_eq!(bb.get("n"), Some(&BbValue::Integer(4)));
    assert_eq!(bb.unset("n"), Ok(()));
    assert_eq!(bb.get("n"), None);
}

/// ASM-071: every type stores and reports its own type.
#[test]
fn given_each_type_when_set_then_stored_and_ty_matches() {
    let mut bb = Blackboard::default();
    let cases = [
        ("a", BbType::Bool, BbValue::Bool(true)),
        ("b", BbType::Number, BbValue::Number(1.5)),
        ("c", BbType::Integer, BbValue::Integer(-3)),
        ("d", BbType::String, BbValue::String("x".into())),
        ("e", BbType::Entity, BbValue::Entity(entity(7))),
        ("f", BbType::Position, BbValue::Position(pos(1.0))),
    ];
    for (key, ty, value) in cases {
        assert_eq!(value.ty(), ty);
        bb.declare(key, ty);
        assert_eq!(bb.set(key, value.clone()), Ok(()));
        assert_eq!(bb.get(key), Some(&value));
    }
}

/// ASM-071: wrong-type write is refused and keeps the old value.
#[test]
fn given_wrong_type_when_set_then_refused_and_value_kept() {
    let mut bb = Blackboard::default();
    bb.declare("n", BbType::Number);
    assert_eq!(bb.set("n", BbValue::Number(2.0)), Ok(()));

    assert_eq!(
        bb.set("n", BbValue::Integer(1)),
        Err(BlackboardError::WrongType {
            key: "n".into(),
            expected: BbType::Number,
            found: BbType::Integer
        })
    );
    assert_eq!(bb.get("n"), Some(&BbValue::Number(2.0)));
}

/// ASM-071: undeclared keys are refused on set and unset, and read as unset.
#[test]
fn given_undeclared_key_when_used_then_refused() {
    let mut bb = Blackboard::default();

    assert_eq!(
        bb.set("nope", BbValue::Bool(true)),
        Err(BlackboardError::Undeclared { key: "nope".into() })
    );
    assert_eq!(
        bb.unset("nope"),
        Err(BlackboardError::Undeclared { key: "nope".into() })
    );
    assert_eq!(bb.get("nope"), None);
}

/// ASM-071: numbers must be finite, positions too.
#[test]
fn given_non_finite_when_set_then_refused() {
    let mut bb = Blackboard::default();
    bb.declare("n", BbType::Number);
    bb.declare("p", BbType::Position);

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            bb.set("n", BbValue::Number(bad)),
            Err(BlackboardError::NonFinite { key: "n".into() })
        );
        assert_eq!(
            bb.set("p", BbValue::Position(pos(bad))),
            Err(BlackboardError::NonFinite { key: "p".into() })
        );
    }
    assert_eq!(bb.get("n"), None);
    assert_eq!(bb.get("p"), None);
}

/// ADR 0014: input keys are read-only for leaves; only `input` writes them.
#[test]
fn given_input_key_when_leaf_writes_then_read_only_and_runtime_can_write() {
    let mut bb = Blackboard::default();
    bb.declare_input("target", BbType::Entity);

    assert_eq!(
        bb.set("target", BbValue::Entity(entity(1))),
        Err(BlackboardError::ReadOnly {
            key: "target".into()
        })
    );
    assert_eq!(
        bb.unset("target"),
        Err(BlackboardError::ReadOnly {
            key: "target".into()
        })
    );
    assert_eq!(bb.input("target", Some(BbValue::Entity(entity(2)))), Ok(()));
    assert_eq!(bb.get("target"), Some(&BbValue::Entity(entity(2))));
    assert_eq!(bb.input("target", None), Ok(()));
    assert_eq!(bb.get("target"), None);
}

/// ADR 0014: `input` validates type and finiteness, and only works on input keys.
#[test]
fn given_input_when_wrong_type_non_finite_or_not_input_then_refused() {
    let mut bb = Blackboard::default();
    bb.declare_input("range", BbType::Number);
    bb.declare("plain", BbType::Number);

    assert_eq!(
        bb.input("range", Some(BbValue::Bool(true))),
        Err(BlackboardError::WrongType {
            key: "range".into(),
            expected: BbType::Number,
            found: BbType::Bool
        })
    );
    assert_eq!(
        bb.input("range", Some(BbValue::Number(f64::NAN))),
        Err(BlackboardError::NonFinite {
            key: "range".into()
        })
    );
    assert_eq!(
        bb.input("plain", Some(BbValue::Number(1.0))),
        Err(BlackboardError::NotInput {
            key: "plain".into()
        })
    );
    assert_eq!(
        bb.input("missing", None),
        Err(BlackboardError::Undeclared {
            key: "missing".into()
        })
    );
}

/// REQ-B-11: clones are independent (instances never share a blackboard).
#[test]
fn given_cloned_blackboard_when_one_writes_then_other_unchanged() {
    let mut a = Blackboard::default();
    a.declare("n", BbType::Integer);
    let b = a.clone();

    assert_eq!(a.set("n", BbValue::Integer(1)), Ok(()));

    assert_eq!(b.get("n"), None);
}

/// REQ-B-1: the intent buffer only appends; earlier intents are untouched.
#[test]
fn given_intent_buffer_when_pushing_then_appends_in_order() {
    let mut v = vec![Intent::Hold];
    let mut buf = IntentBuffer::new(&mut v);
    assert_eq!(buf.len(), 1);
    assert!(!buf.is_empty());

    buf.push(Intent::FollowRoute);
    buf.push(Intent::Despawn);
    assert_eq!(buf.len(), 3);

    assert_eq!(v, vec![Intent::Hold, Intent::FollowRoute, Intent::Despawn]);
}

/// REQ-B-1: an empty vec gives an empty buffer.
#[test]
fn given_empty_vec_when_wrapped_then_buffer_is_empty() {
    let mut v = Vec::new();
    assert!(IntentBuffer::new(&mut v).is_empty());
}

/// ASM-071: redeclaring replaces type, value and read-only flag, in both directions.
#[test]
fn given_redeclare_when_declared_again_then_type_value_and_flag_replaced() {
    let mut bb = Blackboard::default();
    bb.declare("k", BbType::Integer);
    assert_eq!(bb.set("k", BbValue::Integer(1)), Ok(()));

    bb.declare_input("k", BbType::Number);
    assert_eq!(bb.get("k"), None, "value cleared");
    assert_eq!(
        bb.set("k", BbValue::Number(1.0)),
        Err(BlackboardError::ReadOnly { key: "k".into() })
    );
    assert_eq!(bb.input("k", Some(BbValue::Number(2.0))), Ok(()));

    bb.declare("k", BbType::Bool);
    assert_eq!(bb.get("k"), None, "value cleared");
    assert_eq!(bb.set("k", BbValue::Bool(true)), Ok(()));
    assert_eq!(
        bb.set("k", BbValue::Number(1.0)),
        Err(BlackboardError::WrongType {
            key: "k".into(),
            expected: BbType::Bool,
            found: BbType::Number
        })
    );
    assert_eq!(
        bb.input("k", None),
        Err(BlackboardError::NotInput { key: "k".into() })
    );
}

/// ADR 0014: a non-finite position is refused by `input` too.
#[test]
fn given_non_finite_position_when_input_then_refused() {
    let mut bb = Blackboard::default();
    bb.declare_input("p", BbType::Position);

    assert_eq!(
        bb.input("p", Some(BbValue::Position(pos(f64::INFINITY)))),
        Err(BlackboardError::NonFinite { key: "p".into() })
    );
    assert_eq!(bb.get("p"), None);
}

/// ADR 0014: after `input`, leaf writes are refused and the value is kept; before it, unset.
#[test]
fn given_input_key_when_set_or_unset_after_input_then_read_only_and_value_kept() {
    let mut bb = Blackboard::default();
    bb.declare_input("n", BbType::Integer);
    assert_eq!(bb.get("n"), None);

    assert_eq!(bb.input("n", Some(BbValue::Integer(5))), Ok(()));

    let ro = Err(BlackboardError::ReadOnly { key: "n".into() });
    assert_eq!(bb.set("n", BbValue::Integer(6)), ro);
    assert_eq!(bb.unset("n"), ro);
    assert_eq!(bb.get("n"), Some(&BbValue::Integer(5)));
}

/// Error messages show types in lowercase.
#[test]
fn given_wrong_type_error_when_displayed_then_lowercase_type_names() {
    let e = BlackboardError::WrongType {
        key: "k".into(),
        expected: BbType::Number,
        found: BbType::Entity,
    };

    assert_eq!(e.to_string(), "blackboard key `k` holds number, not entity");
}

/// REQ-B-1: `IntentBuffer` debug output shows only the length.
#[test]
fn given_intent_buffer_when_debug_printed_then_only_len() {
    let mut v = vec![Intent::Hold];
    let buf = IntentBuffer::new(&mut v);

    assert_eq!(format!("{buf:?}"), "IntentBuffer { len: 1 }");
}
