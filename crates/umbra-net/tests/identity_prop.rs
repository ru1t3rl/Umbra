//! WI-5 (property): the identity map agrees with a reference model on the exact result of every
//! operation and stays a bijection (REQ-N-5, ADR 0031).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;

use proptest::prelude::*;
use umbra_core::EntityId;
use umbra_core::net::{KeySpace, NetKey};
use umbra_net::{IdentityError, IdentityMap, Role};

#[derive(Debug, Clone)]
enum Op {
    Local(u64),
    Bind(u64, u64),
    Forget(u64),
}

fn eid(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero")
}

/// Keys 0..14 around a space `(base, base + count]`, so both edges and outside are hit.
fn op(base: u64) -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u64..10).prop_map(Op::Local),
        (0u64..14, 1u64..10)
            .prop_map(move |(k, e)| Op::Bind(base.wrapping_add(k).wrapping_sub(3), e)),
        (1u64..10).prop_map(Op::Forget),
    ]
}

/// Reference model: two maps and an issue counter.
struct Model {
    issued: u64,
    by_entity: BTreeMap<u64, (u64, Role)>,
    by_key: BTreeMap<u64, u64>,
}

fn run(space: KeySpace, ops: &[Op]) -> Result<(), TestCaseError> {
    let mut m = IdentityMap::new(space);
    let mut model = Model {
        issued: 0,
        by_entity: BTreeMap::new(),
        by_key: BTreeMap::new(),
    };
    for op in ops {
        match *op {
            Op::Local(e) => {
                let got = m.local_key(eid(e));
                let want = match model.by_entity.get(&e) {
                    Some(&(k, Role::Owned)) => Ok(NetKey(k)),
                    Some(&(_, Role::Remote)) => Err(IdentityError::AlreadyBound),
                    None if model.issued >= space.count => Err(IdentityError::KeysExhausted),
                    None => {
                        model.issued += 1;
                        let k = space.base + model.issued;
                        prop_assert!(!model.by_key.contains_key(&k), "model reused {k}");
                        model.by_entity.insert(e, (k, Role::Owned));
                        model.by_key.insert(k, e);
                        Ok(NetKey(k))
                    }
                };
                prop_assert_eq!(got, want);
            }
            Op::Bind(k, e) => {
                let got = m.bind_remote(NetKey(k), eid(e));
                let want = if space.contains(NetKey(k)) {
                    Err(IdentityError::OwnSpace(NetKey(k)))
                } else if model.by_key.contains_key(&k) || model.by_entity.contains_key(&e) {
                    Err(IdentityError::AlreadyBound)
                } else {
                    model.by_entity.insert(e, (k, Role::Remote));
                    model.by_key.insert(k, e);
                    Ok(())
                };
                prop_assert_eq!(got, want);
            }
            Op::Forget(e) => {
                m.forget(eid(e));
                if let Some((k, _)) = model.by_entity.remove(&e) {
                    model.by_key.remove(&k);
                }
            }
        }
        // Full state agreement, including after errors (map unchanged).
        for raw_e in 1..10 {
            let want = model.by_entity.get(&raw_e).map(|&(k, _)| NetKey(k));
            prop_assert_eq!(m.key(eid(raw_e)), want);
        }
        for raw_k in 0..40u64 {
            for k in [raw_k, space.base.wrapping_add(raw_k).wrapping_sub(3)] {
                let want = model.by_key.get(&k).map(|&e| {
                    let (_, role) = model.by_entity[&e];
                    (eid(e), role)
                });
                prop_assert_eq!(m.entity(NetKey(k)), want);
            }
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn matches_reference_model(ops in prop::collection::vec(op(10), 0..100)) {
        run(KeySpace { base: 10, count: 5 }, &ops)?;
    }

    #[test]
    fn matches_reference_model_near_u64_max(ops in prop::collection::vec(op(u64::MAX - 8), 0..100)) {
        run(KeySpace { base: u64::MAX - 8, count: 5 }, &ops)?;
    }
}
