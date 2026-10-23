//! M7 WI-11: the exact bytes of a recording of `every_record()` are pinned in a committed file, so
//! a format change is a visible diff (REQ-K-7, ASM-621, ADR 0053).
//!
//! To change the bytes intentionally (a format change that also bumps or extends the version
//! rules, ADR 0053): run `UMBRA_UPDATE_GOLDEN=1 cargo test -p umbra-record --test golden`, review
//! the diff of `tests/golden/every_record.umbrarec.jsonl`, and record why in the change. The test
//! fails after writing, on purpose, so an update can never pass unnoticed.

#![forbid(unsafe_code)]

mod common;

use common::{every_record, header, write_bytes};

const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/every_record.umbrarec.jsonl"
);

#[test]
fn given_every_record_kind_then_the_bytes_equal_the_committed_golden() {
    let bytes = write_bytes(&header(), &every_record());

    if std::env::var_os("UMBRA_UPDATE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &bytes).unwrap_or_else(|e| panic!("writing {GOLDEN}: {e}"));
        panic!("golden updated; review the diff and re-run without UMBRA_UPDATE_GOLDEN");
    }

    let golden = std::fs::read(GOLDEN).unwrap_or_else(|e| panic!("reading {GOLDEN}: {e}"));
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&golden),
        "the recording bytes changed; see the header of this file to update the golden"
    );
}
