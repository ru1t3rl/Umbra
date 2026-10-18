//! `validate_name`: the public segment grammar for blackboard keys and other names (ASM-079).

#![forbid(unsafe_code)]

use umbra_core::{MAX_ID_LEN, NameError, validate_name};

#[test]
fn accepts_segments() {
    for ok in ["a", "target", "t_1", "a0_b"] {
        assert_eq!(validate_name(ok), Ok(()), "{ok}");
    }
}

#[test]
fn rejects_with_specific_errors() {
    assert_eq!(validate_name(""), Err(NameError::Empty));
    assert_eq!(validate_name("1a"), Err(NameError::InvalidStart));
    assert_eq!(validate_name("_a"), Err(NameError::InvalidStart));
    assert_eq!(
        validate_name("aB"),
        Err(NameError::InvalidChar { index: 1 })
    );
    assert_eq!(
        validate_name("a b"),
        Err(NameError::InvalidChar { index: 1 })
    );
}

#[test]
fn over_long_name_echoes_a_bounded_prefix_and_never_mentions_force() {
    let long = "a".repeat(MAX_ID_LEN + 100);
    let err = validate_name(&long).err();
    let Some(NameError::TooLong { prefix, len }) = &err else {
        panic!("{err:?}");
    };
    assert!(prefix.len() <= 64);
    assert_eq!(*len, MAX_ID_LEN + 100);
    for e in [Some(NameError::Empty), Some(NameError::InvalidStart), err] {
        let text = e.map(|e| e.to_string()).unwrap_or_default();
        assert!(!text.to_lowercase().contains("force"), "{text}");
    }
}
