//! AT-19 (WI-11, ASM-194): the text of a library error in a problem body does not quote an absolute
//! path. The engine side is `umbra-engine/tests/handle_paths.rs`. Also the `limit` rule of the shared
//! query extractor, reached through the entities route (ASM-193 a).

#![forbid(unsafe_code)]

mod common;

use common::{Harness, write_scenario_variant};
use serde_json::json;
use umbra_api_types::ProblemCode;

#[tokio::test]
async fn a_plugin_load_error_quotes_no_absolute_path() {
    let h = Harness::start("problem-paths-plugin");
    // A plugin directory inside the root that does not exist: the plugin host's error names it.
    let path = write_scenario_variant(
        &h.root,
        "withplugins",
        "template_paths = [\"templates\"]",
        "template_paths = [\"templates\"]\nplugin_paths = [\"no_such_plugins\"]",
    );
    let reply = h.load_body(&json!({"path": path})).await;
    let problem = reply.assert_problem(ProblemCode::ScenarioLoadFailed);
    let root = h.root.to_string_lossy().into_owned();
    assert!(
        !reply.text().contains(&root),
        "the body quotes the root: {}",
        reply.text()
    );
    // The scrubbed message still says what failed, with the path relative to the root.
    assert!(
        problem.detail.contains("no_such_plugins"),
        "detail: {}",
        problem.detail
    );
}

#[tokio::test]
async fn a_limit_out_of_range_is_422_and_a_malformed_one_is_400() {
    let h = Harness::start("problem-limit");
    h.load_skeleton("stepped").await;
    for limit in ["0", "1001", "100000"] {
        h.entities(&format!("limit={limit}"))
            .await
            .assert_problem(ProblemCode::ValidationFailed);
    }
    for limit in ["abc", "-1", ""] {
        h.entities(&format!("limit={limit}"))
            .await
            .assert_problem(ProblemCode::BadRequest);
    }
}
