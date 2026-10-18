//! AT-17 (M3 acceptance 2): swapping a behavior needs only a template change.
//! Requirements: M3 acceptance 2, REQ-B-6, REQ-T-1, ADR 0010, ASM-083.
//!
//! The fixtures `tests/fixtures/behavior_swap/{a,b}/` are two copies of the patrol-and-engage
//! example that differ in exactly one template file (`templates/tank_blue.toml`) and one key (its
//! `[behavior] ref`): in `a` blue-1 uses `example.behavior.patrol_and_engage`, in `b` it uses
//! `example.behavior.follow_route` (a graph whose only node is `builtin.action.follow_route`).
//!
//! Written in step 4 of M3; both tests run since WI-16.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use core::time::Duration;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use behavior_support::{entity, fire_targets, fixture_dir, id_of, instantiate_with, load};
use common::run_to;

/// Long enough for blue-1 in `b` to finish its 800 m route at 10 m/s (about 2400 ticks) and for
/// the first shot in `a` (about tick 1200).
const SWAP_TICKS: u64 = 2700;

/// Every file under `dir`, relative path to bytes.
fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("readable directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).expect("under root").to_path_buf();
                out.insert(rel, std::fs::read(&path).expect("readable file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// M3 acceptance 2: the fixtures hold the same files; exactly one differs, the blue tank's
/// template, and it differs in exactly one line, the `ref` of `[behavior]`.
#[test]
fn fixtures_a_and_b_differ_in_exactly_one_template_file_and_one_key() {
    let a = files_under(&fixture_dir("behavior_swap/a"));
    let b = files_under(&fixture_dir("behavior_swap/b"));

    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "same file list"
    );
    let differing: Vec<&PathBuf> = a.keys().filter(|k| a[*k] != b[*k]).collect();
    assert_eq!(differing, [&PathBuf::from("templates/tank_blue.toml")]);
    let text = |m: &BTreeMap<PathBuf, Vec<u8>>| {
        String::from_utf8(m[&PathBuf::from("templates/tank_blue.toml")].clone()).expect("UTF-8")
    };
    let (ta, tb) = (text(&a), text(&b));
    let changed: Vec<(&str, &str)> = ta.lines().zip(tb.lines()).filter(|(x, y)| x != y).collect();
    assert_eq!(ta.lines().count(), tb.lines().count());
    assert_eq!(
        changed,
        [(
            "ref = \"example.behavior.patrol_and_engage\"",
            "ref = \"example.behavior.follow_route\""
        )]
    );
}

/// M3 acceptance 2: without changing code or the scenario, the swap changes what blue-1 does: in
/// `a` it fires; in `b` it never fires and drives its route to the end. Red's run is
/// the same in both (nothing in the scenario changed for it).
#[test]
fn swapped_behavior_changes_intents_with_no_code_or_scenario_change() {
    let run = |side: &str| {
        let scenario = load(&fixture_dir(&format!("behavior_swap/{side}")));
        let run = instantiate_with(&scenario, Duration::from_micros(500), None, None);
        let (blue, red) = (id_of(&run, "blue-1"), id_of(&run, "red-1"));
        (blue, red, run_to(run.engine, SWAP_TICKS, false))
    };

    let (blue_a, red_a, a) = run("a");
    let (blue_b, red_b, b) = run("b");

    assert_eq!((blue_a, red_a), (blue_b, red_b));
    let fires = |t: &common::Trace, id| {
        t.snapshots
            .iter()
            .any(|s| !fire_targets(entity(s, id)).is_empty())
    };
    assert!(fires(&a, blue_a), "a: patrol-and-engage fires");
    assert!(!fires(&b, blue_b), "b: follow_route never fires");
    let last = entity(b.snapshots.last().expect("a last tick"), blue_b);
    assert_eq!(last.route_next, Some((1, 2)), "b: the route is finished");
    for (sa, sb) in a.snapshots.iter().zip(&b.snapshots) {
        let (ra, rb) = (entity(sa, red_a), entity(sb, red_b));
        assert_eq!(
            (&ra.state, &ra.intents),
            (&rb.state, &rb.intents),
            "red is unaffected"
        );
    }
}
