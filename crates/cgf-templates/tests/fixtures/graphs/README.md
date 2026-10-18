# Graph fixtures (M3)

Provenance: hand-written for M3 step 4 (test-engineer), used by `tests/scenario_force.rs`.
They use the stub components of `tests/graph_support/mod.rs` (`stub.cond.near`,
`stub.action.noop`, `stub.motion.alpha`), not the real built-ins.

| Directory | Content | Expected |
|---|---|---|
| `scenario_ok/` | scenario with two entities (forces `blue` and none), a template with `[behavior]` naming the graph `test.graph.patrol`, the graph file `templates/patrol.behavior.toml` | loads, no diagnostics |
| `scenario_bad_graph/` | same, but the graph names a child node `ghost` that does not exist | `unknown_node` (at `"ghost"`) and `unreachable_node` (at `idle`) in `templates/patrol.behavior.toml` |
| `discovery_mixed/` | scenario (`template_paths = ["templates"]`), template `templates/veh.toml`, valid graph `templates/good.behavior.toml`, graph `templates/bad.behavior.toml` with an unknown key `colour` at 7:1 (WI-12, `graph_discovery.rs`) | graphs are classified by the `.behavior.toml` suffix; only `bad.behavior.toml:7:1` has a diagnostic (`unknown_key`) |
| `scenario_table_graph/` | one `x.behavior.toml` with a `[scenario]` table (WI-12 follow-up, `graph_discovery.rs`) | read as a graph: `unknown_key` for `scenario`; `load_scenario` says `not_a_scenario` |
| `sibling_graphs/` | `a.behavior.toml` and `b.behavior.toml` define one ID (`test.graph.same`); `caller.behavior.toml` calls `callee.behavior.toml` with the range `^2.0` but the callee is 1.0.0 (WI-13, `graph_discovery.rs`) | `validate_path` of a single graph file also reads its direct siblings: `duplicate_id` at `b` (note at `a`), `version_mismatch` at `caller:9:11` |
| `scenario_bad_call/` | as `scenario_ok/`, but the template passes `engage_range_m = 13.0` (a value the leaf `stub.cond.near` refuses at build) and an undeclared `extra` (WI-14, `template_graph_use.rs`) | `unknown_param` at `extra` and `param_invalid` at `13.0`, both in `templates/tank.toml`; `validate_path` and `load_scenario` agree |
| `invalid_structure/` | twelve graph files with a structural error each: unknown top-level key, missing `nodes` or `schema_version`, node without `type`, unknown node type, `children` not an array, `repeat` count 0, unknown node key, interface param without `type`, FSM transition without or with both triggers, unknown blackboard type (WI-15, `behavior_file_schema.rs`) | the schema `BEHAVIOR_FILE_SCHEMA` rejects each and the reader rejects each |
