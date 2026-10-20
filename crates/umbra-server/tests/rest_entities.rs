//! AT-7 (WI-13): entities, spawn and remove, templates and components over REST. Requirements:
//! REQ-X-1, REQ-X-5, REQ-T-3, REQ-K-5, ADR 0026 (m4, m5). The scenario is `walking_skeleton`: three
//! entities with ids 1 (`base-1`, template `ground_vehicle.base`, group `alpha`), 2 (`tank-1`,
//! `tank.mbt_generic`, `alpha`) and 3 (`truck-1`, `truck.cargo`, `bravo`), near 52 N 4 E.
//!
//! Every test is ignored until the router (WI-11) and the entity handlers (WI-13) exist; the
//! stream-reading tests also need WI-15.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use axum::http::Method;
use common::{Harness, WsClient};
use serde_json::{Value, json};
use umbra_api_types::ProblemCode;

fn ids(page: &Value) -> Vec<u64> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|e| e["id"].as_u64().expect("id"))
        .collect()
}

async fn loaded(tag: &str) -> Harness {
    let h = Harness::start(tag);
    h.load_skeleton("stepped").await;
    h
}

fn spawn_body(name: &str) -> Value {
    json!({"template": "truck.cargo", "lat_deg": 52.5, "lon_deg": 4.5, "name": name})
}

#[tokio::test]
#[ignore = "WI-13"]
async fn list_is_keyset_paged_with_after_id_and_next_after_id() {
    let h = loaded("ent-paging").await;
    let first = h.entities("limit=2").await.assert_json(200);
    assert_eq!(ids(&first), [1, 2]);
    assert_eq!(first["next_after_id"], 2);
    assert_eq!(first["total"], 3);
    assert_eq!(first["tick"], 0);
    let second = h.entities("limit=2&after_id=2").await.assert_json(200);
    assert_eq!(ids(&second), [3]);
    assert!(
        second
            .as_object()
            .expect("object")
            .contains_key("next_after_id"),
        "the cursor is present on the last page"
    );
    assert!(second["next_after_id"].is_null());
    // The default page holds everything here.
    assert_eq!(ids(&h.entities("").await.assert_json(200)), [1, 2, 3]);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn filters_by_name_force_group_template_ownership() {
    let h = loaded("ent-filters").await;
    let blue = h
        .post_json(
            "/api/v1/entities",
            &json!({"template": "truck.cargo", "lat_deg": 52.5, "lon_deg": 4.5, "name": "blue-1", "force": "blue"}),
        )
        .await
        .assert_json(201);
    let blue_id = blue["summary"]["id"].as_u64().expect("id");
    let cases = [
        ("name=tank-1", vec![2]),
        ("group=alpha", vec![1, 2]),
        ("group=bravo", vec![3]),
        ("template=truck.cargo", vec![3, blue_id]),
        ("force=blue", vec![blue_id]),
        ("ownership=owned", vec![1, 2, 3, blue_id]),
        ("ownership=remote", vec![]),
        ("group=alpha&template=truck.cargo", vec![]),
    ];
    for (query, expected) in cases {
        let page = h.entities(query).await.assert_json(200);
        assert_eq!(ids(&page), expected, "{query}");
    }
}

#[tokio::test]
#[ignore = "WI-13"]
async fn ids_accept_repeated_and_comma_forms() {
    let h = loaded("ent-ids").await;
    let repeated = h.entities("ids=1&ids=3").await.assert_json(200);
    let comma = h.entities("ids=1,3").await.assert_json(200);
    assert_eq!(ids(&repeated), [1, 3]);
    assert_eq!(ids(&comma), [1, 3]);
    assert_eq!(repeated["items"], comma["items"]);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn bbox_four_params_all_or_none() {
    let h = loaded("ent-bbox").await;
    let full = "min_lat_deg=51.99&min_lon_deg=3.9&max_lat_deg=52.01&max_lon_deg=4.1";
    assert_eq!(ids(&h.entities(full).await.assert_json(200)), [1]);
    for partial in [
        "min_lat_deg=51.99",
        "min_lat_deg=51.99&min_lon_deg=3.9&max_lat_deg=52.01",
        "min_lon_deg=3.9&max_lat_deg=52.01&max_lon_deg=4.1",
    ] {
        h.entities(partial)
            .await
            .assert_problem(ProblemCode::BadRequest);
    }
    assert_eq!(
        ids(&h.entities("").await.assert_json(200)).len(),
        3,
        "none: no filter"
    );
}

#[tokio::test]
#[ignore = "WI-13"]
async fn bbox_filter_including_antimeridian() {
    let h = loaded("ent-antimeridian").await;
    let east = h
        .post_json(
            "/api/v1/entities",
            &json!({"template": "truck.cargo", "lat_deg": 0.0, "lon_deg": 179.9, "name": "east"}),
        )
        .await
        .assert_json(201);
    let west = h
        .post_json(
            "/api/v1/entities",
            &json!({"template": "truck.cargo", "lat_deg": 0.0, "lon_deg": -179.9, "name": "west"}),
        )
        .await
        .assert_json(201);
    let east_id = east["summary"]["id"].as_u64().expect("id");
    let west_id = west["summary"]["id"].as_u64().expect("id");
    // min_lon > max_lon: the box wraps over the antimeridian.
    let across = "min_lat_deg=-1&max_lat_deg=1&min_lon_deg=170&max_lon_deg=-170";
    assert_eq!(
        ids(&h.entities(across).await.assert_json(200)),
        [east_id, west_id]
    );
    // The wrapped box that spans everything but 10..100 E keeps the entities at 4 E and on the
    // antimeridian, and nothing else.
    let wide = "min_lat_deg=-90&max_lat_deg=90&min_lon_deg=100&max_lon_deg=10";
    assert_eq!(
        ids(&h.entities(wide).await.assert_json(200)),
        [1, 2, 3, east_id, west_id]
    );
    // A plain box on the other side does not wrap.
    let plain = "min_lat_deg=-1&max_lat_deg=1&min_lon_deg=-170&max_lon_deg=170";
    assert_eq!(
        ids(&h.entities(plain).await.assert_json(200)),
        Vec::<u64>::new()
    );
}

#[tokio::test]
#[ignore = "WI-13"]
async fn limit_over_1000_is_400() {
    let h = loaded("ent-limit").await;
    h.entities("limit=1001")
        .await
        .assert_problem(ProblemCode::BadRequest);
    assert_eq!(h.entities("limit=1000").await.status, 200);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn get_entity_detail() {
    let h = loaded("ent-detail").await;
    let detail = h.get("/api/v1/entities/2").await.assert_json(200);
    let summary = &detail["summary"];
    assert_eq!(summary["id"], 2);
    assert_eq!(summary["name"], "tank-1");
    assert_eq!(summary["template"], "tank.mbt_generic");
    assert_eq!(summary["group"], "alpha");
    assert_eq!(summary["ownership"], "owned");
    assert!(summary["lat_deg"].as_f64().expect("lat") > 52.0);
    assert!(
        !detail["motion_model"]
            .as_str()
            .expect("motion model")
            .is_empty(),
        "{detail}"
    );
    assert!(detail["contacts"].is_array());
}

#[tokio::test]
#[ignore = "WI-13"]
async fn unknown_entity_is_404() {
    let h = loaded("ent-unknown").await;
    h.get("/api/v1/entities/999")
        .await
        .assert_problem(ProblemCode::EntityNotFound);
}

#[tokio::test]
#[ignore = "WI-15"]
async fn spawn_is_201_and_visible_in_list_and_stream() {
    let h = loaded("ent-spawn").await;
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    let primed = ws.next_frame().await;
    assert_eq!(primed["entities"].as_array().expect("entities").len(), 3);

    let detail = h
        .post_json("/api/v1/entities", &spawn_body("walk-1"))
        .await
        .assert_json(201);
    assert_eq!(detail["summary"]["id"], 4);
    assert_eq!(detail["summary"]["name"], "walk-1");
    assert_eq!(detail["summary"]["template"], "truck.cargo");

    assert_eq!(ids(&h.entities("").await.assert_json(200)), [1, 2, 3, 4]);
    let frame = ws
        .frame_until(|f| f["entities"].as_array().is_some_and(|e| e.len() == 4))
        .await;
    assert_eq!(
        frame["cause"], "control",
        "a spawn publishes a control frame"
    );
}

#[tokio::test]
#[ignore = "WI-13"]
async fn spawn_unknown_template_is_404() {
    let h = loaded("ent-spawn-unknown").await;
    let mut body = spawn_body("x");
    body["template"] = json!("no.such_template");
    h.post_json("/api/v1/entities", &body)
        .await
        .assert_problem(ProblemCode::TemplateNotFound);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn spawn_bad_id_is_422_invalid_id() {
    let h = loaded("ent-spawn-bad-id").await;
    let mut body = spawn_body("x");
    body["template"] = json!("Not A Valid Id!");
    h.post_json("/api/v1/entities", &body)
        .await
        .assert_problem(ProblemCode::InvalidId);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn spawn_duplicate_name_is_409() {
    let h = loaded("ent-spawn-dup").await;
    h.post_json("/api/v1/entities", &spawn_body("base-1"))
        .await
        .assert_problem(ProblemCode::DuplicateName);
    assert_eq!(
        ids(&h.entities("").await.assert_json(200)).len(),
        3,
        "nothing was added"
    );
}

#[tokio::test]
#[ignore = "WI-13"]
async fn spawn_when_stopped_is_409_invalid_state() {
    let h = loaded("ent-spawn-stopped").await;
    h.run_command("stop").await.assert_json(200);
    h.post_json("/api/v1/entities", &spawn_body("late"))
        .await
        .assert_problem(ProblemCode::InvalidState);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn remove_is_204_and_id_not_reused() {
    let h = loaded("ent-remove").await;
    let first = h
        .post_json("/api/v1/entities", &spawn_body("tmp-1"))
        .await
        .assert_json(201);
    assert_eq!(first["summary"]["id"], 4);
    let removed = h.send(Method::DELETE, "/api/v1/entities/4").await;
    assert_eq!(removed.status, 204);
    assert_eq!(removed.body, Vec::<u8>::new());
    h.get("/api/v1/entities/4")
        .await
        .assert_problem(ProblemCode::EntityNotFound);
    let second = h
        .post_json("/api/v1/entities", &spawn_body("tmp-2"))
        .await
        .assert_json(201);
    assert_eq!(second["summary"]["id"], 5, "id 4 is never reused");
}

#[tokio::test]
#[ignore = "WI-13"]
async fn second_remove_is_404() {
    let h = loaded("ent-remove-twice").await;
    assert_eq!(
        h.send(Method::DELETE, "/api/v1/entities/3").await.status,
        204
    );
    h.send(Method::DELETE, "/api/v1/entities/3")
        .await
        .assert_problem(ProblemCode::EntityNotFound);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn templates_list_and_detail_with_root_relative_sources() {
    let h = loaded("ent-templates").await;
    let list = h.get("/api/v1/templates").await.assert_json(200);
    let items = list.as_array().expect("a list of templates");
    let truck = items
        .iter()
        .find(|t| t["id"] == "truck.cargo")
        .expect("truck.cargo is listed");
    assert_eq!(
        truck["source"],
        "walking_skeleton/templates/truck_cargo.toml"
    );
    let root = h.root.to_string_lossy().into_owned();
    for template in items {
        let source = template["source"].as_str().expect("source");
        assert!(
            !source.starts_with('/') && !source.contains(&root),
            "{source}"
        );
    }
    let detail = h
        .get("/api/v1/templates/truck.cargo")
        .await
        .assert_json(200);
    assert_eq!(detail["id"], "truck.cargo");
    assert_eq!(
        detail["chain"],
        json!(["truck.cargo", "ground_vehicle.base"])
    );
    assert_eq!(detail["source"], truck["source"]);
    h.get("/api/v1/templates/no.such_template")
        .await
        .assert_problem(ProblemCode::TemplateNotFound);
}

#[tokio::test]
#[ignore = "WI-13"]
async fn components_list_with_param_schemas_and_kind_filter() {
    // Components need no scenario (ADR 0026 matrix).
    let h = Harness::start("ent-components");
    let all = h.get("/api/v1/components").await.assert_json(200);
    let all = all.as_array().expect("a list of components");
    assert_ne!(all.len(), 0);
    for entry in all {
        assert!(entry["params_schema"].is_object(), "{entry}");
        assert!(entry["id"].is_string() && entry["kind"].is_string());
    }
    let motion = h
        .get("/api/v1/components?kind=motion_model")
        .await
        .assert_json(200);
    let motion = motion.as_array().expect("a list");
    assert_ne!(motion.len(), 0);
    assert!(motion.iter().all(|e| e["kind"] == "motion_model"));
    assert!(motion.iter().any(|e| e["id"] == "builtin.ground.kinematic"));
    assert!(motion.len() < all.len(), "the filter removes other kinds");
}

#[tokio::test]
#[ignore = "WI-15"]
async fn rest_and_snapshot_values_are_bit_identical() {
    // The stream's frame entities and `GET /entities` are the same DTO from the same tick; every
    // number must have the same bits (REQ-K-5, ADR 0027).
    let h = loaded("ent-bits").await;
    h.run_command("start").await.assert_json(200);
    h.step(37).await.assert_json(200);
    let addr = h.serve_on_loopback().await;
    let mut ws = WsClient::connect(addr).await;
    ws.hello().await;
    ws.subscribe(json!({})).await;
    let frame = ws.next_frame().await;
    assert_eq!(frame["tick"], 37);
    let page = h.entities("").await.assert_json(200);
    assert_eq!(page["tick"], 37);
    let rest = page["items"].as_array().expect("items");
    let streamed = frame["entities"].as_array().expect("entities");
    assert_eq!(rest.len(), streamed.len());
    for (a, b) in rest.iter().zip(streamed) {
        for field in ["lat_deg", "lon_deg", "alt_m", "heading_deg", "speed_mps"] {
            let x = a[field].as_f64().expect("number");
            let y = b[field].as_f64().expect("number");
            assert_eq!(x.to_bits(), y.to_bits(), "{field} of entity {}", a["id"]);
        }
        assert_eq!(a, b);
    }
}
