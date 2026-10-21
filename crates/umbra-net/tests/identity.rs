//! WI-5: the identity map issues, binds and forgets keys (REQ-N-5, REQ-K-5, ADR 0031, ASM-274).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use umbra_core::EntityId;
use umbra_core::net::{KeySpace, NetKey};
use umbra_net::{IdentityError, IdentityMap, Role};

fn eid(raw: u64) -> EntityId {
    EntityId::new(raw).expect("non-zero")
}

fn map(base: u64, count: u64) -> IdentityMap {
    IdentityMap::new(KeySpace { base, count })
}

/// Keys are `base + 1, base + 2, ...` in call order, and a repeat call returns the same key.
#[test]
fn local_keys_follow_call_order() {
    let mut m = map(100, 5);
    assert_eq!(m.local_key(eid(9)), Ok(NetKey(101)));
    assert_eq!(m.local_key(eid(3)), Ok(NetKey(102)));
    assert_eq!(m.local_key(eid(9)), Ok(NetKey(101)));
    assert_eq!(m.local_key(eid(7)), Ok(NetKey(103)));
    assert_eq!(m.entity(NetKey(102)), Some((eid(3), Role::Owned)));
    assert_eq!(m.key(eid(7)), Some(NetKey(103)));
}

/// The (count+1)-th entity gets `KeysExhausted`; entities that already hold a key keep it.
#[test]
fn keys_exhausted_at_count() {
    let mut m = map(0, 2);
    assert_eq!(m.local_key(eid(1)), Ok(NetKey(1)));
    assert_eq!(m.local_key(eid(2)), Ok(NetKey(2)));
    assert_eq!(m.local_key(eid(3)), Err(IdentityError::KeysExhausted));
    assert_eq!(m.local_key(eid(1)), Ok(NetKey(1)));
    assert_eq!(m.key(eid(3)), None);
}

/// A zero-sized space issues nothing, and a space at the top of `u64` never overflows.
#[test]
fn empty_and_extreme_spaces() {
    assert_eq!(
        map(5, 0).local_key(eid(1)),
        Err(IdentityError::KeysExhausted)
    );
    let mut m = map(u64::MAX - 1, u64::MAX);
    assert_eq!(m.local_key(eid(1)), Ok(NetKey(u64::MAX)));
    assert_eq!(m.local_key(eid(2)), Err(IdentityError::KeysExhausted));
}

/// A forgotten owned entity frees nothing: the next entity gets a fresh key, and the exhausted
/// space stays exhausted.
#[test]
fn keys_are_never_reused_after_forget() {
    let mut m = map(10, 3);
    assert_eq!(m.local_key(eid(1)), Ok(NetKey(11)));
    m.forget(eid(1));
    assert_eq!(m.key(eid(1)), None);
    assert_eq!(m.entity(NetKey(11)), None);
    assert_eq!(m.local_key(eid(2)), Ok(NetKey(12)));
    assert_eq!(m.local_key(eid(3)), Ok(NetKey(13)));
    assert_eq!(m.local_key(eid(4)), Err(IdentityError::KeysExhausted));
    // The forgotten entity, issued again, is a new binding and finds the space exhausted.
    assert_eq!(m.local_key(eid(1)), Err(IdentityError::KeysExhausted));
}

/// Keys inside the own space are refused, at both edges; just outside is accepted.
#[test]
fn bind_remote_refuses_own_space() {
    let mut m = map(100, 5);
    for raw in [101, 105] {
        assert_eq!(
            m.bind_remote(NetKey(raw), eid(1)),
            Err(IdentityError::OwnSpace(NetKey(raw)))
        );
    }
    assert_eq!(m.key(eid(1)), None);
    assert_eq!(m.bind_remote(NetKey(100), eid(1)), Ok(()));
    assert_eq!(m.bind_remote(NetKey(106), eid(2)), Ok(()));
    assert_eq!(m.entity(NetKey(106)), Some((eid(2), Role::Remote)));
    assert_eq!(m.key(eid(1)), Some(NetKey(100)));
}

/// A key or an entity that is already mapped cannot be bound again, and the first binding survives.
#[test]
fn bind_remote_refuses_duplicates() {
    let mut m = map(100, 5);
    assert_eq!(m.bind_remote(NetKey(7), eid(1)), Ok(()));
    assert_eq!(
        m.bind_remote(NetKey(7), eid(2)),
        Err(IdentityError::AlreadyBound)
    );
    assert_eq!(
        m.bind_remote(NetKey(8), eid(1)),
        Err(IdentityError::AlreadyBound)
    );
    assert_eq!(m.entity(NetKey(7)), Some((eid(1), Role::Remote)));
    assert_eq!(m.entity(NetKey(8)), None);
    assert_eq!(m.key(eid(2)), None);
    // An owned entity cannot be bound as a ghost, nor a ghost issued a local key.
    assert_eq!(m.local_key(eid(3)), Ok(NetKey(101)));
    assert_eq!(
        m.bind_remote(NetKey(9), eid(3)),
        Err(IdentityError::AlreadyBound)
    );
    assert_eq!(m.local_key(eid(1)), Err(IdentityError::AlreadyBound));
}

/// Forgetting a ghost frees its key and entity for a new binding; forgetting an unknown entity is a no-op.
#[test]
fn forget_releases_remote_bindings() {
    let mut m = map(100, 5);
    assert_eq!(m.bind_remote(NetKey(7), eid(1)), Ok(()));
    m.forget(eid(42));
    assert_eq!(m.key(eid(1)), Some(NetKey(7)));
    m.forget(eid(1));
    assert_eq!(m.entity(NetKey(7)), None);
    assert_eq!(m.bind_remote(NetKey(7), eid(2)), Ok(()));
    assert_eq!(m.bind_remote(NetKey(8), eid(1)), Ok(()));
}
