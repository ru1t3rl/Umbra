//! AT-14 (WI-2): intent set v1. Requirements: REQ-B-2, ADR 0016, ASM-082.

#![forbid(unsafe_code)]

use umbra_core::intent::INTENT_SET_VERSION;
use umbra_core::{
    ComponentId, Ecef, EntityId, Intent, MetersPerSecond, Radians, TemplateId, Vec3, WeaponId,
};

fn ecef() -> Ecef {
    Ecef(Vec3::new(1.0, 2.0, 3.0))
}

#[test]
fn intent_set_version_is_one() {
    assert_eq!(INTENT_SET_VERSION, 1);
}

#[test]
fn every_variant_constructs_and_compares() {
    let target = EntityId::new(7).expect("non-zero");
    let all = vec![
        Intent::MoveTo {
            destination: ecef(),
        },
        Intent::SetSpeed(MetersPerSecond(5.0)),
        Intent::Hold,
        Intent::FollowRoute,
        Intent::SetHeading(Radians(1.5)),
        Intent::Fire {
            target,
            weapon: WeaponId::new("acme.gun").expect("valid"),
        },
        Intent::Spawn {
            template: TemplateId::new("tank.mbt_generic").expect("valid"),
            position: ecef(),
            heading: Radians(0.5),
        },
        Intent::Despawn,
        Intent::Custom {
            kind: ComponentId::new("acme.custom").expect("valid"),
            payload: serde_json::json!({"a": 1}),
        },
    ];
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            assert_eq!(a == b, i == j, "{a:?} vs {b:?}");
        }
        assert_eq!(a, &a.clone());
    }
}

#[test]
fn weapon_id_shares_the_id_grammar() {
    assert!(WeaponId::new("acme.gun").is_ok());
    assert!("builtin.cannon".parse::<WeaponId>().is_ok());
    assert!(WeaponId::new("").is_err());
    assert!(WeaponId::new("nonamespace").is_err());
    assert!(WeaponId::new("Bad.Case").is_err());
    assert_eq!(WeaponId::new("a.b").expect("valid").as_str(), "a.b");
}
