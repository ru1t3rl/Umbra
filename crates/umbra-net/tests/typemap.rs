//! WI-5: entity type to template mapping and its precedence (REQ-N-4, ADR 0031, ASM-232, ASM-236).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use umbra_core::TemplateId;
use umbra_core::net::NetEntityType;
use umbra_net::{TypeMap, TypeMapWarning};

fn tid(s: &str) -> TemplateId {
    TemplateId::new(s).expect("valid template id")
}

fn ty(s: &str) -> NetEntityType {
    NetEntityType::new(s)
}

/// A declared token resolves to its template.
#[test]
fn declaration_resolves() {
    let (m, w) = TypeMap::new(&[(tid("tank.mbt"), ty("tank"))], &[], None);
    assert_eq!(w, Vec::new());
    assert_eq!(m.resolve(Some(&ty("tank"))), Some(&tid("tank.mbt")));
}

/// Precedence: the `type_map` override beats the declaration, which beats the default.
#[test]
fn override_beats_declaration_beats_default() {
    let declared = [(tid("tank.mbt"), ty("tank")), (tid("apc.m113"), ty("apc"))];
    let overrides = [(ty("tank"), tid("tank.t72"))];
    let (m, w) = TypeMap::new(&declared, &overrides, Some(tid("core.generic")));
    assert_eq!(w, Vec::new());
    assert_eq!(m.resolve(Some(&ty("tank"))), Some(&tid("tank.t72")));
    assert_eq!(m.resolve(Some(&ty("apc"))), Some(&tid("apc.m113")));
    assert_eq!(m.resolve(Some(&ty("heli"))), Some(&tid("core.generic")));
}

/// An override also maps a token no template declares.
#[test]
fn override_adds_new_token() {
    let (m, _) = TypeMap::new(&[], &[(ty("heli"), tid("heli.uh60"))], None);
    assert_eq!(m.resolve(Some(&ty("heli"))), Some(&tid("heli.uh60")));
}

/// Unknown and absent tokens use the default; without one they are unmapped.
#[test]
fn unmapped_is_none_and_default_covers_absent() {
    let declared = [(tid("tank.mbt"), ty("tank"))];
    let (none, _) = TypeMap::new(&declared, &[], None);
    assert_eq!(none.resolve(Some(&ty("heli"))), None);
    assert_eq!(none.resolve(None), None);
    let (with, _) = TypeMap::new(&declared, &[], Some(tid("core.generic")));
    assert_eq!(with.resolve(None), Some(&tid("core.generic")));
    assert_eq!(with.resolve(Some(&ty("heli"))), Some(&tid("core.generic")));
}

/// The smallest template id wins whatever the declaration order, with one warning per token
/// naming the others ascending.
#[test]
fn duplicate_declarations_keep_smallest_with_warning() {
    for declared in [
        [
            (tid("b.tank"), ty("tank")),
            (tid("a.tank"), ty("tank")),
            (tid("c.tank"), ty("tank")),
        ],
        [
            (tid("c.tank"), ty("tank")),
            (tid("b.tank"), ty("tank")),
            (tid("a.tank"), ty("tank")),
        ],
    ] {
        let (m, w) = TypeMap::new(&declared, &[], None);
        assert_eq!(m.resolve(Some(&ty("tank"))), Some(&tid("a.tank")));
        assert_eq!(
            w,
            vec![TypeMapWarning::DuplicateDeclaration {
                entity_type: ty("tank"),
                kept: tid("a.tank"),
                ignored: vec![tid("b.tank"), tid("c.tank")],
            }]
        );
    }
}

/// An override of a duplicated token still wins; the duplicate warning stays (it concerns the declarations).
#[test]
fn override_wins_over_duplicates() {
    let declared = [(tid("a.tank"), ty("tank")), (tid("b.tank"), ty("tank"))];
    let (m, w) = TypeMap::new(&declared, &[(ty("tank"), tid("z.tank"))], None);
    assert_eq!(m.resolve(Some(&ty("tank"))), Some(&tid("z.tank")));
    assert_eq!(w.len(), 1);
}
