//! Every golden file is used by some test (M6 WI-14 acceptance check): each fixture belongs to a
//! family whose golden test demands an expectation for every file with its prefix, and each one is a
//! valid PDU that survives the generic malformed rows.
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod support;

use std::path::Path;

use support::{VERSIONS, all_fixtures, assert_standard_malformed, fixtures_with_prefix, load};
use umbra_net_dis::codec::decode;

/// The prefix of the files each `golden_*.rs` test iterates (`golden_es.rs` `es_`, `golden_fire.rs`
/// `fire_`, `golden_detonation.rs` `det_`, `golden_siman.rs` `start_resume_` and `stop_freeze_`).
const FAMILY_PREFIXES: [&str; 5] = ["es_", "fire_", "det_", "start_resume_", "stop_freeze_"];

#[test]
fn every_fixture_belongs_to_a_golden_test_family() {
    for version in VERSIONS {
        let all = all_fixtures(version);
        let covered = fixtures_with_prefix(version, &FAMILY_PREFIXES);
        assert_eq!(
            all, covered,
            "{version:?}: a fixture no golden test iterates"
        );
    }
}

#[test]
fn every_fixture_decodes_in_its_own_version() {
    for version in VERSIONS {
        for name in all_fixtures(version) {
            let (got, _) = decode(&load(version, &name)).expect("decodes");
            assert_eq!(got, version, "{name}");
            if !name.ends_with("_articulated") {
                assert_standard_malformed(&format!("{version:?} {name}"), &load(version, &name));
            }
        }
    }
}

/// The fixture directories hold only `*.hex` files: no stray file or subdirectory a test would skip
/// (the README and the generator and cross-check scripts live one level up).
#[test]
fn fixture_directories_hold_only_hex_files() {
    let top = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dis");
    for sub in ["v6", "v7"] {
        for entry in std::fs::read_dir(top.join(sub)).expect("fixture dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            assert!(
                entry.file_type().expect("type").is_file(),
                "{sub}/{name} is not a file"
            );
            assert!(
                Path::new(&name).extension().is_some_and(|e| e == "hex"),
                "{sub}/{name} is not a .hex fixture"
            );
        }
    }
    let mut top_level: Vec<String> = std::fs::read_dir(&top)
        .expect("dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    top_level.sort();
    assert_eq!(
        top_level,
        [
            "README.md",
            "gen_es.py",
            "gen_warfare.py",
            "v6",
            "v7",
            "xcheck_opendis.py"
        ],
        "unexpected file next to the fixtures"
    );
}
