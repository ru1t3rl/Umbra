//! AT-9: IDs. Requirements: REQ-C-2, ADR 0002 (accepted), ASM-017.

use cgf_core::{BehaviorId, EntityId, IdError, MAX_ID_LEN, MotionModelId, PluginId, TemplateId};
use proptest::prelude::*;

/// Every ID example that appears in the spec (ADR 0002 context).
const SPEC_EXAMPLES: &[&str] = &[
    "tank.mbt_generic",
    "ground_vehicle.base",
    "builtin.ground.kinematic",
    "builtin.fsm.patrol_and_engage",
    "builtin.visual",
    "acme.behavior.smart_tank",
    "acme.behavior.patrol",
];

/// Runs one check body for each of the four string ID types (they share one grammar).
macro_rules! for_each_id_type {
    ($ty:ident, $body:block) => {{
        type $ty = TemplateId;
        $body
    }
    {
        type $ty = BehaviorId;
        $body
    }
    {
        type $ty = MotionModelId;
        $body
    }
    {
        type $ty = PluginId;
        $body
    }};
}

/// REQ-C-2: every spec example is a valid ID for every ID type, and `as_str` returns it unchanged.
#[test]
fn every_spec_example_parses_for_every_id_type() {
    for &s in SPEC_EXAMPLES {
        for_each_id_type!(Id, {
            assert_eq!(Id::new(s).expect(s).as_str(), s);
        });
    }
}

/// REQ-C-2: `FromStr`, `TryFrom<&str>`, `TryFrom<String>`, `Display` and `AsRef<str>` agree.
#[test]
fn all_construction_paths_agree_and_display_round_trips() {
    let s = "acme.behavior.patrol";
    for_each_id_type!(Id, {
        let a = Id::new(s).unwrap();
        assert_eq!(s.parse::<Id>().unwrap(), a);
        assert_eq!(Id::try_from(s).unwrap(), a);
        assert_eq!(Id::try_from(s.to_string()).unwrap(), a);
        assert_eq!(a.to_string(), s);
        assert_eq!(a.as_ref() as &str, s);
    });
}

/// ADR 0002: the empty string is `Empty`.
#[test]
fn empty_input_is_empty_error() {
    assert_eq!(TemplateId::new(""), Err(IdError::Empty));
}

/// ADR 0002: a single segment has no namespace.
#[test]
fn single_segment_is_missing_namespace() {
    assert_eq!(
        TemplateId::new("tank"),
        Err(IdError::MissingNamespace { id: "tank".into() })
    );
}

/// ADR 0002: consecutive dots, a leading dot and a trailing dot are empty segments.
#[test]
fn empty_segments_are_rejected() {
    for s in ["a..b", ".a.b", "a.b."] {
        let r = TemplateId::new(s);
        assert!(
            matches!(&r, Err(IdError::EmptySegment { id, .. }) if id == s),
            "{s:?} -> {r:?}"
        );
    }
}

/// ADR 0002: the leading empty segment is at byte 0 (the index of an empty segment is where it begins).
#[test]
fn leading_dot_reports_index_zero() {
    assert_eq!(
        TemplateId::new(".a.b"),
        Err(IdError::EmptySegment {
            id: ".a.b".into(),
            index: 0
        })
    );
}

/// ADR 0002: characters outside `[a-z0-9_]` are `InvalidChar` with the byte index.
#[test]
fn hyphen_is_invalid_char_with_index() {
    assert_eq!(
        TemplateId::new("acme-x.y"),
        Err(IdError::InvalidChar {
            id: "acme-x.y".into(),
            ch: '-',
            index: 4
        })
    );
}

/// ADR 0002: upper case inside a segment is `InvalidChar`.
#[test]
fn uppercase_inside_segment_is_invalid_char() {
    assert_eq!(
        TemplateId::new("aB.c"),
        Err(IdError::InvalidChar {
            id: "aB.c".into(),
            ch: 'B',
            index: 1
        })
    );
}

/// ADR 0002: non-ASCII is rejected (byte index of its first byte).
#[test]
fn non_ascii_is_invalid_char() {
    assert_eq!(
        TemplateId::new("a.b\u{fc}"),
        Err(IdError::InvalidChar {
            id: "a.b\u{fc}".into(),
            ch: '\u{fc}',
            index: 3
        })
    );
}

/// ADR 0002, ASM-025: an upper-case first char is `InvalidChar` at byte 0.
#[test]
fn uppercase_segment_start_is_rejected() {
    assert_eq!(
        TemplateId::new("Tank.base"),
        Err(IdError::InvalidChar {
            id: "Tank.base".into(),
            ch: 'T',
            index: 0
        })
    );
}

/// ASM-025: `EmptySegment.index` is where the empty segment begins.
#[test]
fn empty_segment_index_is_where_it_begins() {
    assert_eq!(
        TemplateId::new("a..b"),
        Err(IdError::EmptySegment {
            id: "a..b".into(),
            index: 2
        })
    );
    assert_eq!(
        TemplateId::new("a.b."),
        Err(IdError::EmptySegment {
            id: "a.b.".into(),
            index: 4
        })
    );
}

/// ASM-025: an invalid character wins over a missing namespace.
#[test]
fn invalid_char_takes_precedence_over_missing_namespace() {
    assert!(matches!(
        TemplateId::new("Tank"),
        Err(IdError::InvalidChar {
            ch: 'T',
            index: 0,
            ..
        })
    ));
}

/// ASM-025: a digit at the very start is `InvalidSegmentStart` at byte 0.
#[test]
fn digit_first_segment_start_reports_index_zero() {
    assert_eq!(
        TemplateId::new("1a.b"),
        Err(IdError::InvalidSegmentStart {
            id: "1a.b".into(),
            index: 0
        })
    );
}

/// ASM-025: the `TooLong` prefix is cut at a char boundary (a 2-byte char straddles byte 64).
#[test]
fn too_long_prefix_respects_char_boundaries() {
    // 63 ASCII bytes, then a 2-byte char occupying bytes 63..65, then padding past 128.
    let s = format!("{}\u{fc}{}", "a".repeat(63), "b".repeat(100));

    let Err(IdError::TooLong { id, len, max }) = TemplateId::new(&s) else {
        panic!("expected TooLong");
    };

    assert_eq!((len, max), (s.len(), 128));
    assert_eq!(id, "a".repeat(63));
}

/// Error messages quote the input with `{:?}`, so control characters cannot forge extra lines.
#[test]
fn newline_in_input_does_not_make_multiline_message() {
    for s in ["a.b\nc", "a\n.b", "a.b\n", "\n", "a.b\x1b[31m", "a.b\rc"] {
        let msg = TemplateId::new(s).unwrap_err().to_string();
        assert!(!msg.contains(['\n', '\r', '\x1b']), "{msg:?}");
    }
    let long = format!("a.{}\n", "b".repeat(200));
    assert!(
        !TemplateId::new(&long)
            .unwrap_err()
            .to_string()
            .contains('\n')
    );
}

/// ADR 0002: a segment must start with `a-z`, a digit does not qualify.
#[test]
fn digit_segment_start_is_invalid_segment_start() {
    assert_eq!(
        TemplateId::new("a.1b"),
        Err(IdError::InvalidSegmentStart {
            id: "a.1b".into(),
            index: 2
        })
    );
}

/// ADR 0002: an underscore cannot start a segment but may appear later in it.
#[test]
fn underscore_start_is_rejected_but_underscore_inside_is_valid() {
    assert!(matches!(
        TemplateId::new("a._b"),
        Err(IdError::InvalidSegmentStart { index: 2, .. })
    ));
    assert!(TemplateId::new("a.b_c1_").is_ok());
}

fn id_of_len(n: usize) -> String {
    // "a." + (n - 2) times 'b': valid grammar, exact byte length n.
    format!("a.{}", "b".repeat(n - 2))
}

/// ADR 0002: 128 bytes is the maximum and is accepted.
#[test]
fn id_of_exactly_max_len_is_valid() {
    assert!(TemplateId::new(&id_of_len(MAX_ID_LEN)).is_ok());
}

/// ADR 0002: 129 bytes is `TooLong` and reports the real length and the maximum.
#[test]
fn id_of_129_bytes_is_too_long() {
    let r = TemplateId::new(&id_of_len(129));

    assert!(
        matches!(
            &r,
            Err(IdError::TooLong {
                len: 129,
                max: 128,
                ..
            })
        ),
        "{r:?}"
    );
}

/// ADR 0002: error text stays bounded for huge input (only a prefix of the ID is reported).
#[test]
fn too_long_error_message_is_bounded() {
    let err = TemplateId::new(&id_of_len(100_000)).unwrap_err();

    assert!(matches!(err, IdError::TooLong { len: 100_000, .. }));
    assert!(
        err.to_string().len() < 512,
        "message is {} bytes",
        err.to_string().len()
    );
}

/// REQ-C-2: `EntityId` is never 0 (ASM-017).
#[test]
fn entity_id_zero_is_none() {
    assert_eq!(EntityId::new(0), None);
}

/// REQ-C-2: non-zero raw values round-trip, including the extremes.
#[test]
fn entity_id_round_trips_raw_value() {
    assert_eq!(EntityId::new(1).map(EntityId::get), Some(1));
    assert_eq!(EntityId::new(u64::MAX).map(EntityId::get), Some(u64::MAX));
}

/// REQ-C-2: `EntityId` displays as decimal.
#[test]
fn entity_id_displays_as_decimal() {
    assert_eq!(EntityId::new(1234).unwrap().to_string(), "1234");
}

/// ASM-017: the `NonZeroU64` niche makes `Option<EntityId>` free.
#[test]
fn option_entity_id_has_no_size_overhead() {
    assert_eq!(size_of::<Option<EntityId>>(), size_of::<u64>());
}

/// REQ-K-4: ordering is byte-lexicographic so IDs are deterministic `BTreeMap` keys.
#[test]
fn ids_order_bytewise() {
    let mut ids: Vec<TemplateId> = ["b.a", "a.b_c", "a.b", "a.bc", "a.b0"]
        .iter()
        .map(|s| TemplateId::new(s).unwrap())
        .collect();

    ids.sort();

    let sorted: Vec<&str> = ids.iter().map(TemplateId::as_str).collect();
    assert_eq!(sorted, ["a.b", "a.b0", "a.b_c", "a.bc", "b.a"]);
}

fn valid_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,10}(\\.[a-z][a-z0-9_]{0,10}){1,5}"
}

/// Reference predicate for `^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$` and at most 128 bytes.
fn oracle_accepts(s: &str) -> bool {
    let segment_ok = |seg: &str| {
        let mut chars = seg.chars();
        chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    s.len() <= MAX_ID_LEN && s.split('.').count() >= 2 && s.split('.').all(segment_ok)
}

proptest! {
    /// ADR 0002: the validator accepts exactly what the reference grammar accepts.
    #[test]
    fn validator_matches_reference_grammar(s in "[a-zA-Z0-9_.\\-]{0,20}") {
        prop_assert_eq!(TemplateId::new(&s).is_ok(), oracle_accepts(&s), "{:?}", s);
    }

    /// REQ-C-2: any grammar-valid string parses and displays back unchanged.
    #[test]
    fn display_then_parse_round_trips(s in valid_id()) {
        let id = TemplateId::new(&s).expect("valid by construction");

        prop_assert_eq!(id.to_string(), s.clone());
        prop_assert_eq!(id.to_string().parse::<TemplateId>().unwrap(), id);
    }

    /// REQ-K-4: `Ord` on IDs equals `Ord` on their strings.
    #[test]
    fn id_order_equals_string_order(a in valid_id(), b in valid_id()) {
        let (ia, ib) = (TemplateId::new(&a).unwrap(), TemplateId::new(&b).unwrap());

        prop_assert_eq!(ia.cmp(&ib), a.as_bytes().cmp(b.as_bytes()));
    }

    /// REQ-C-2: arbitrary text never panics the validator.
    #[test]
    fn arbitrary_input_never_panics(s in ".{0,200}") {
        let _ = TemplateId::new(&s);
    }
}
