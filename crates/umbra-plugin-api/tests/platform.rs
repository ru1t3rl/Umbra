//! Scaffold contract (M4 WI-1, ADR 0019 amended R-M7, ASM-120): the ABI crate builds only for
//! 64-bit targets, where pointers, `usize` and function pointers are 8 bytes.

#![forbid(unsafe_code)]

/// The compile-time assert in `lib.rs` holds, and pointers agree with it.
#[test]
fn pointers_and_usize_are_8_bytes() {
    assert_eq!(size_of::<usize>(), 8);
    assert_eq!(size_of::<*const u8>(), 8);
    assert_eq!(size_of::<Option<extern "C" fn()>>(), 8);
}
