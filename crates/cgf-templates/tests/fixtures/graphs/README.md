# Graph fixtures (M3)

Provenance: hand-written for M3 step 4 (test-engineer), used by `tests/scenario_force.rs`.
They use the stub components of `tests/graph_support/mod.rs` (`stub.cond.near`,
`stub.action.noop`, `stub.motion.alpha`), not the real built-ins.

| Directory | Content | Expected |
|---|---|---|
| `scenario_ok/` | scenario with two entities (forces `blue` and none), a template with `[behavior]` naming the graph `test.graph.patrol`, the graph file `templates/patrol.behavior.toml` | loads, no diagnostics |
| `scenario_bad_graph/` | same, but the graph names a child node `ghost` that does not exist | `unknown_node` (at `"ghost"`) and `unreachable_node` (at `idle`) in `templates/patrol.behavior.toml` |
