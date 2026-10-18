//! AT-18 (M3 acceptance 4): the M2 compatibility proof, process rows. The scripted checklist items
//! (byte-identical fixtures, unchanged M1/M2 test files, the M2 test-name baseline) run in
//! `scripts/m2-compat.sh`, which build-engineer runs in WI-16 and WI-18.
//! Requirements: M3 acceptance 4, A-001 constraints 1-5, REQ-T-6, ADR 0003 (same-OS scope).

#![forbid(unsafe_code)]

mod common;

use std::path::Path;

use common::{cli, example, final_hash, stderr, stdout};

/// The final state hash recorded at the M2 close-out (full local gate at 673a963).
#[cfg(target_os = "linux")]
const M2_WALKING_SKELETON_HASH: &str = "849026656b7b42d2";

/// A-001 constraint 3: the M2 example scenario still runs to the hash recorded at the M2
/// close-out, whatever M3 changes in the kernel and the engine. Linux only (ADR 0003: bit
/// identity is a same-OS promise).
#[cfg(target_os = "linux")]
#[test]
fn walking_skeleton_final_hash_is_the_m2_value() {
    let scenario = example();

    let out = cli(&["run", scenario.as_str()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(
        final_hash(&stdout(&out)).as_deref(),
        Some(M2_WALKING_SKELETON_HASH)
    );
}

/// A-001 constraint 3: `validate` on the M2 example prints what it printed in M2.
#[test]
fn walking_skeleton_still_validates_with_four_files() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/walking_skeleton");

    let out = cli(&["validate", dir.to_string_lossy().as_ref()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().last(), Some("ok: 4 files checked"));
}

/// Spec 7.1: the example template of the spec validates against the registry the CLI uses (the
/// standard registry: `builtin.visual`, `builtin.fsm.patrol_and_engage` with its `aggression`
/// parameter, the `network.dis` table). In M2 this was `unknown_id` for the sensor and behavior.
#[test]
fn spec_7_1_template_validates_with_standard_registry() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spec_7_1");

    let out = cli(&["validate", dir.to_string_lossy().as_ref()]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {} stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out).lines().last(), Some("ok: 2 files checked"));
}
