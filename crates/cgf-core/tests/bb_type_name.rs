//! `BbType::from_name` is the inverse of `Display` (ASM-071, WI-12 review n2).

#![forbid(unsafe_code)]

use cgf_core::BbType;

#[test]
fn from_name_inverts_display() {
    for ty in [
        BbType::Bool,
        BbType::Number,
        BbType::Integer,
        BbType::String,
        BbType::Entity,
        BbType::Position,
    ] {
        assert_eq!(BbType::from_name(&ty.to_string()), Some(ty));
    }
    assert_eq!(BbType::from_name("boolean"), None);
    assert_eq!(BbType::from_name("Bool"), None);
    assert_eq!(BbType::from_name(""), None);
}
