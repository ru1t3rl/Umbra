//! AT-6 (core part): the canonical state hasher. Requirements: REQ-K-4, ADR 0007.
//!
//! Reference values were produced independently of the Rust code (see the comment on each).

#![forbid(unsafe_code)]

use umbra_core::hash::fnv1a64;
use umbra_core::{Quaternion, STATE_HASH_VERSION, StateHash, StateHasher, Vec3};

/// A fresh hasher after the given writes.
fn hash_of(f: impl FnOnce(&mut StateHasher)) -> StateHash {
    let mut h = StateHasher::new();
    f(&mut h);
    h.finish()
}

/// ADR 0007: FNV-1a 64 matches the published test vectors.
///
/// Source: Fowler/Noll/Vo test suite (isthe.com/chongo/tech/comp/fnv). Verified 2026-10-17
/// by an independent Python implementation of the algorithm (offset basis `cbf29ce484222325`,
/// prime `100000001b3`), which printed exactly these three values.
#[test]
fn fnv1a64_published_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

/// ADR 0007: the version constant is part of the contract.
#[test]
fn state_hash_version_is_two() {
    assert_eq!(STATE_HASH_VERSION, 2);
}

/// ADR 0007: the stream starts with the version as a little-endian `u32`, so an empty hasher
/// equals raw FNV-1a over `02 00 00 00`.
///
/// Verification route: Python FNV-1a 64 over `bytes([2, 0, 0, 0])` printed `0x8d1ace904a398d17`.
#[test]
fn version_prefix_is_hashed() {
    let expected = fnv1a64(&[2, 0, 0, 0]);

    assert_eq!(StateHasher::new().finish(), StateHash(expected));
    assert_eq!(expected, 0x8d1a_ce90_4a39_8d17);
}

/// ADR 0007: `-0.0` is written as `+0.0`.
#[test]
fn negative_zero_hashes_like_zero() {
    assert_eq!(
        hash_of(|h| h.write_f64(-0.0)),
        hash_of(|h| h.write_f64(0.0))
    );
}

/// ADR 0007: every NaN is written as `0x7FF8_0000_0000_0000`, whatever its payload or sign.
#[test]
fn all_nan_payloads_hash_alike() {
    let quiet = f64::NAN;
    let negative = f64::from_bits(0xFFF8_0000_0000_0000);
    let payload = f64::from_bits(0x7FF8_0000_0000_1234);
    let signalling = f64::from_bits(0x7FF0_0000_0000_0001);

    let reference = hash_of(|h| h.write_f64(quiet));

    assert_eq!(hash_of(|h| h.write_f64(negative)), reference);
    assert_eq!(hash_of(|h| h.write_f64(payload)), reference);
    assert_eq!(hash_of(|h| h.write_f64(signalling)), reference);
}

/// ADR 0007: only zero sign and NaN payloads are folded; neighbouring floats differ.
#[test]
fn adjacent_floats_hash_differently() {
    let one = 1.0_f64;
    let next = f64::from_bits(one.to_bits() + 1);

    assert_ne!(
        hash_of(|h| h.write_f64(one)),
        hash_of(|h| h.write_f64(next))
    );
    assert_ne!(
        hash_of(|h| h.write_f64(0.0)),
        hash_of(|h| h.write_f64(f64::MIN_POSITIVE))
    );
}

/// ADR 0007: `write_bytes` is length-prefixed, so splitting a byte run differently differs.
#[test]
fn write_bytes_is_length_prefixed() {
    let one_run = hash_of(|h| h.write_bytes(&[1, 2]));
    let two_runs = hash_of(|h| {
        h.write_bytes(&[1]);
        h.write_bytes(&[2]);
    });

    assert_ne!(one_run, two_runs);
}

/// ADR 0007: a string is written like its UTF-8 bytes.
///
/// Assumption (recorded in the M1 running log): `write_str(s)` is `write_bytes(s.as_bytes())`.
#[test]
fn write_str_equals_write_bytes_of_utf8() {
    assert_eq!(
        hash_of(|h| h.write_str("abc")),
        hash_of(|h| h.write_bytes(b"abc"))
    );
}

/// ADR 0007: integers are fixed width little-endian, so widths are not interchangeable.
#[test]
fn integer_widths_are_not_interchangeable() {
    assert_ne!(hash_of(|h| h.write_u32(1)), hash_of(|h| h.write_u64(1)));
    assert_ne!(hash_of(|h| h.write_u8(1)), hash_of(|h| h.write_u32(1)));
}

/// ADR 0007: `write_vec3` is x, y, z as three `f64`; `write_quaternion` is w, x, y, z.
#[test]
fn composite_writers_are_the_documented_sequences() {
    let v = Vec3 {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let q = Quaternion {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    let vec_hash = hash_of(|h| h.write_vec3(v));
    let vec_manual = hash_of(|h| {
        h.write_f64(1.0);
        h.write_f64(2.0);
        h.write_f64(3.0);
    });
    let quat_hash = hash_of(|h| h.write_quaternion(q));
    let quat_manual = hash_of(|h| {
        h.write_f64(1.0);
        h.write_f64(0.0);
        h.write_f64(0.0);
        h.write_f64(0.0);
    });

    assert_eq!(vec_hash, vec_manual);
    assert_eq!(quat_hash, quat_manual);
}

/// ADR 0007: the encoding is pinned by a fixed sequence of writes. Integers and exactly
/// representable floats only, so the value is identical on every OS.
///
/// Verification route: an independent Python encoder (version `u32` LE; `u8`; `u32` LE; `u64` LE;
/// `f64` as 8 LE bytes with `-0.0 -> 0` and NaN -> `0x7FF8...`; bytes as `u64` LE length then the
/// bytes) fed to FNV-1a 64 printed `0x4023c60d9f54beb0`.
#[test]
fn golden_encoding_hash() {
    let got = hash_of(|h| {
        h.write_u8(7);
        h.write_u32(0xDEAD_BEEF);
        h.write_u64(0x0123_4567_89AB_CDEF);
        h.write_f64(1.5);
        h.write_f64(-0.0);
        h.write_f64(f64::NAN);
        h.write_str("abc");
        h.write_bytes(&[1, 2, 3]);
        h.write_vec3(Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        });
        h.write_quaternion(Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        });
    });

    assert_eq!(got, StateHash(0x4023_c60d_9f54_beb0));
}

/// ADR 0007: single-write goldens (same Python encoder as `golden_encoding_hash`).
#[test]
fn single_write_goldens() {
    assert_eq!(hash_of(|h| h.write_u8(1)), StateHash(0xfe1a_152e_1fca_bc62));
    assert_eq!(
        hash_of(|h| h.write_u32(1)),
        StateHash(0x46c2_da3b_e7c3_1176)
    );
    assert_eq!(
        hash_of(|h| h.write_u64(1)),
        StateHash(0x1ff3_f111_c7bd_8bd6)
    );
    assert_eq!(
        hash_of(|h| h.write_f64(0.0)),
        StateHash(0x3eee_b81a_d2ac_d5f7)
    );
    assert_eq!(
        hash_of(|h| h.write_bytes(b"abc")),
        StateHash(0x2812_9758_9ba4_e2fc)
    );
}

/// ADR 0007: `finish` does not consume or alter the state (calling it twice agrees).
#[test]
fn finish_twice_gives_the_same_hash() {
    let mut h = StateHasher::new();
    h.write_u64(99);

    assert_eq!(h.finish(), h.finish());
}

/// ADR 0007: a hash displays as exactly 16 lowercase hex digits, zero padded.
#[test]
fn display_is_16_lowercase_hex() {
    assert_eq!(StateHash(0xABCD).to_string(), "000000000000abcd");
    assert_eq!(StateHash(u64::MAX).to_string(), "ffffffffffffffff");
    assert_eq!(StateHash(0).to_string(), "0000000000000000");
}
