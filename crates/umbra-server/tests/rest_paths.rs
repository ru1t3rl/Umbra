//! AT-19 (server rows, WI-11): path confinement as the API shows it. Requirements: REQ-X-8, NFR-4,
//! ADR 0026 (amended, D-b). The engine side is in `umbra-engine/tests/confine.rs`.
//!
//! Every test is ignored until the router (WI-11) exists.

#![forbid(unsafe_code)]

mod common;

use common::{Harness, copy_dir, examples_dir, walkthrough, write_scenario_variant};
use serde_json::json;
use umbra_api_types::ProblemCode;

#[tokio::test]
#[ignore = "WI-11"]
async fn missing_path_outside_root_is_403_not_404() {
    let h = Harness::start("paths-missing-outside");
    for path in [
        "../nowhere/scenario.toml",
        "walking_skeleton/../../nowhere.toml",
        "/definitely/not/here/scenario.toml",
    ] {
        h.load_body(&json!({"path": path}))
            .await
            .assert_problem(ProblemCode::PathNotAllowed);
    }
}

#[tokio::test]
#[ignore = "WI-11"]
async fn missing_path_inside_root_is_422_scenario_invalid() {
    let h = Harness::start("paths-missing-inside");
    let problem = h
        .load_body(&json!({"path": "walking_skeleton/nope.toml"}))
        .await
        .assert_problem(ProblemCode::ScenarioInvalid);
    assert!(
        problem.diagnostics.is_some_and(|d| !d.is_empty()),
        "the loader's io diagnostic is carried"
    );
}

#[tokio::test]
#[ignore = "WI-11"]
async fn absolute_path_inside_root_accepted() {
    let h = Harness::start("paths-absolute-inside");
    let absolute = h.root.join("walking_skeleton/scenario.toml");
    let info = h
        .load_body(&json!({"path": absolute.to_string_lossy()}))
        .await
        .assert_json(201);
    assert_eq!(
        info["path"], "walking_skeleton/scenario.toml",
        "shown relative to the root"
    );
}

#[tokio::test]
#[ignore = "WI-11"]
async fn directory_is_422_validation_failed() {
    let h = Harness::start("paths-directory");
    h.load_body(&json!({"path": "walking_skeleton"}))
        .await
        .assert_problem(ProblemCode::ValidationFailed);
}

#[tokio::test]
#[ignore = "WI-11"]
async fn template_dir_outside_root_is_403_path_not_allowed() {
    // Extra: the confinement covers what the scenario reads, not only the request path.
    let h = Harness::start("paths-template-outside");
    let outside = h
        .root
        .parent()
        .expect("parent")
        .join("paths-template-outside-templates");
    copy_dir(&examples_dir().join("walking_skeleton/templates"), &outside);
    let path = write_scenario_variant(
        &h.root,
        "escaping",
        "template_paths = [\"templates\"]",
        "template_paths = [\"../../paths-template-outside-templates\"]",
    );
    let reply = h.load_body(&json!({"path": path})).await;
    reply.assert_problem(ProblemCode::PathNotAllowed);
    assert!(!reply.text().contains(&*outside.to_string_lossy()));
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "WI-11"]
async fn symlink_out_of_root_is_403_path_not_allowed() {
    // Extra: a symlink inside the root that leads out of it.
    let h = Harness::start("paths-symlink");
    let outside = h
        .root
        .parent()
        .expect("parent")
        .join("paths-symlink-outside");
    copy_dir(&examples_dir().join("walking_skeleton"), &outside);
    std::os::unix::fs::symlink(&outside, h.root.join("link")).expect("symlink");
    h.load_body(&json!({"path": "link/scenario.toml"}))
        .await
        .assert_problem(ProblemCode::PathNotAllowed);
}

#[tokio::test]
#[ignore = "WI-11"]
async fn no_response_contains_an_absolute_path_outside_the_root() {
    let h = Harness::start("paths-no-leak");
    let replies = walkthrough(&h).await;
    assert!(replies.len() > 20, "the walkthrough ran");
    let root = h.root.to_string_lossy().into_owned();
    let tmp = env!("CARGO_TARGET_TMPDIR");
    let manifest = env!("CARGO_MANIFEST_DIR");
    for (i, reply) in replies.iter().enumerate() {
        let text = reply.text();
        for forbidden in [root.as_str(), tmp, manifest] {
            assert!(
                !text.contains(forbidden),
                "response {i} (status {}) contains the absolute path {forbidden}: {text}",
                reply.status
            );
        }
    }
}
