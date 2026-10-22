//! Helpers shared by the golden-packet tests.

#![expect(dead_code, reason = "each test binary uses a subset of the helpers")]

use std::fs;
use std::path::PathBuf;

use umbra_net_dis::codec::{DecodeError, HEADER_LEN, Pdu, decode, encode};
use umbra_net_dis::version::DisVersion;

pub const VERSIONS: [DisVersion; 2] = [DisVersion::V7, DisVersion::V6];

fn fixture_dir(version: DisVersion) -> PathBuf {
    let sub = match version {
        DisVersion::V6 => "v6",
        DisVersion::V7 => "v7",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dis")
        .join(sub)
}

/// Parses an annotated hex file: `#` starts a comment, the rest is hex byte pairs.
fn parse_hex(text: &str) -> Vec<u8> {
    let digits: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0, "odd number of hex digits");
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("hex digit"))
        .collect()
}

/// The bytes of fixture `name` (without `.hex`).
pub fn load(version: DisVersion, name: &str) -> Vec<u8> {
    let path = fixture_dir(version).join(format!("{name}.hex"));
    parse_hex(&fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

/// The sorted names of the fixtures whose name starts with one of `prefixes` (`""` matches all).
pub fn fixtures_with_prefix(version: DisVersion, prefixes: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_dir(version))
        .expect("fixture dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter_map(|f| f.strip_suffix(".hex").map(str::to_owned))
        .filter(|n| prefixes.iter().any(|p| n.starts_with(p)))
        .collect();
    names.sort();
    names
}

/// Overwrites the length field of a PDU.
pub fn set_length(bytes: &mut [u8], length: u16) {
    bytes[8..10].copy_from_slice(&length.to_be_bytes());
}

/// The malformed-input rows every fixed-length PDU shares (AT-8): `bytes` is a valid PDU without
/// variable records. Every truncation, any extension, any wrong length field, any wrong family and
/// any unknown version byte is a typed error, never a panic.
pub fn assert_standard_malformed(label: &str, bytes: &[u8]) {
    let len = u16::try_from(bytes.len()).expect("small");
    assert!(decode(bytes).is_ok(), "{label}: the valid packet decodes");
    for cut in 0..bytes.len() {
        let err = decode(&bytes[..cut]).expect_err(&format!("{label}: cut at {cut}"));
        if cut < HEADER_LEN {
            assert!(
                matches!(err, DecodeError::Truncated { got, .. } if got == cut),
                "{label}: {err:?}"
            );
        } else {
            assert_eq!(
                err,
                DecodeError::LengthMismatch {
                    declared: len,
                    actual: cut
                },
                "{label}"
            );
        }
    }
    for extra in [1usize, 2, 100] {
        let mut longer = bytes.to_vec();
        longer.resize(bytes.len() + extra, 0);
        assert_eq!(
            decode(&longer),
            Err(DecodeError::LengthMismatch {
                declared: len,
                actual: bytes.len() + extra
            }),
            "{label}: extended by {extra}"
        );
    }
    for declared in [0u16, 12, len - 1, len + 1, 0xFFFF] {
        let mut wrong = bytes.to_vec();
        set_length(&mut wrong, declared);
        assert_eq!(
            decode(&wrong),
            Err(DecodeError::LengthMismatch {
                declared,
                actual: bytes.len()
            }),
            "{label}: length field {declared}"
        );
    }
    let (pdu_type, family) = (bytes[2], bytes[3]);
    for other in (0..=u8::MAX).filter(|f| *f != family) {
        let mut wrong = bytes.to_vec();
        wrong[3] = other;
        assert_eq!(
            decode(&wrong),
            Err(DecodeError::WrongFamily {
                pdu_type,
                family: other
            }),
            "{label}: family {other}"
        );
    }
    for version in (0..=u8::MAX).filter(|v| !matches!(v, 6 | 7)) {
        let mut wrong = bytes.to_vec();
        wrong[0] = version;
        assert_eq!(
            decode(&wrong),
            Err(DecodeError::UnsupportedVersion(version)),
            "{label}"
        );
    }
}

/// Golden decode, and unless `decode_only` (the codec drops variable records) byte-identical
/// encode and round trip, of fixture `name` against the hand-written `expected`.
pub fn assert_golden(version: DisVersion, name: &str, expected: &Pdu, decode_only: bool) {
    let bytes = load(version, name);
    assert_eq!(
        decode(&bytes),
        Ok((version, expected.clone())),
        "{version:?} {name}: golden decode"
    );
    if decode_only {
        return;
    }
    let mut out = Vec::new();
    encode(expected, version, &mut out).expect("encode");
    assert_eq!(out, bytes, "{version:?} {name}: encode is byte-identical");
    assert_eq!(
        decode(&out),
        Ok((version, expected.clone())),
        "{version:?} {name}: round trip"
    );
}

/// The v6 and v7 files of `name` differ in the version byte and, when the v7 file has a PDU status,
/// in byte 10 (the v6 file has padding there): nowhere else.
pub fn assert_versions_differ_only_in_version_and_status(name: &str) {
    let (v7, v6) = (load(DisVersion::V7, name), load(DisVersion::V6, name));
    assert_eq!(v7.len(), v6.len(), "{name}");
    assert_eq!(v6[10], 0, "{name}: v6 padding");
    let mut want = vec![0];
    if v7[10] != 0 {
        want.push(10);
    }
    let differing: Vec<usize> = (0..v7.len()).filter(|&i| v7[i] != v6[i]).collect();
    assert_eq!(differing, want, "{name}");
}

/// The names of all fixtures of a version.
pub fn all_fixtures(version: DisVersion) -> Vec<String> {
    fixtures_with_prefix(version, &[""])
}
