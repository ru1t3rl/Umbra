//! AT-4 (engine handle, queries; M5 WI-9): keyset paging, the tick a page was read at, and graph
//! validation inside the plugin build scope. Requirements: REQ-X-1, REQ-X-5, REQ-X-11, ADR 0024,
//! ADR 0026.

#![forbid(unsafe_code)]

mod handle_support;

use handle_support::guarded_step;
mod plugin_support;

use handle_support::{
    config, engine, example, load_stepped, plugin_lock, start_engine, walking_skeleton,
};
use umbra_engine::EntityId;
use umbra_engine::handle::EntityQuery;

fn ids(items: &[umbra_engine::handle::EntityState]) -> Vec<u64> {
    items.iter().map(|e| e.snapshot.id.get()).collect()
}

#[test]
fn entities_keyset_paging_with_after_id() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let mut query = EntityQuery::default();
    query.limit = 2;

    let first = handle.entities(query.clone()).expect("first page");
    query.after_id = first.next_after_id;
    let second = handle.entities(query.clone()).expect("second page");
    let all = handle.entities(EntityQuery::default()).expect("everything");

    assert_eq!(first.total, 3);
    assert_eq!(first.items.len(), 2);
    assert_eq!(
        first.next_after_id,
        first.items.last().map(|e| e.snapshot.id)
    );
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.next_after_id, None, "the last page has no cursor");
    let paged: Vec<u64> = ids(&first.items)
        .into_iter()
        .chain(ids(&second.items))
        .collect();
    assert_eq!(paged, ids(&all.items));
    assert!(
        paged.windows(2).all(|w| w[0] < w[1]),
        "ascending by id: {paged:?}"
    );
}

#[test]
fn pages_report_their_tick() {
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let at_start = handle.entities(EntityQuery::default()).expect("page").tick;
    handle.start().expect("start");
    guarded_step(&handle, 4).expect("step");

    let later = handle.entities(EntityQuery::default()).expect("page").tick;
    let detail = handle
        .entity(EntityId::new(1).expect("non-zero"))
        .expect("entity");

    assert_eq!((at_start, later), (0, 4));
    assert_eq!(detail.snapshot.id.get(), 1);
}

#[test]
fn entities_filter_by_name() {
    // extra: a filter narrows both the items and the total.
    let (handle, _thread) = engine();
    load_stepped(&handle, walking_skeleton());
    let mut query = EntityQuery::default();
    query.name = Some("tank-1".to_owned());

    let page = handle.entities(query).expect("page");

    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].info.name.as_deref(), Some("tank-1"));
}

#[test]
fn validate_graph_runs_in_build_scope() {
    // REQ-X-11, M6: a posted graph that calls a plugin behavior is valid only inside the session's
    // plugin build scope (the real `example-wander` library).
    let _lock = plugin_lock();
    let mut with_wander = config();
    with_wander.load.extra_plugin_paths = vec![plugin_support::wander()];
    let (handle, _thread) = start_engine(with_wander);
    handle
        .load(example("plugin_wander"), None, None)
        .expect("load");
    let document =
        plugin_support::call_graph("test.behavior.calls_wander", "example.behavior.wander");

    let result = handle
        .validate_graph("draft.behavior.toml".to_owned(), document)
        .expect("validate");

    assert!(result.valid, "{:?}", result.diagnostics);
}
