# plugin_wander

The M4 acceptance scenario (spec section 17, M4 acceptance 1): the example plugin
`examples/plugins/wander` drives entities, used directly by a template and as a `subgraph` of a
file graph, next to a built-in behavior.

```
cargo xtask build-plugins --release
cargo run -p umbra-cli -- validate examples/scenarios/plugin_wander --plugin-path target/plugins
cargo run -p umbra-cli -- run examples/scenarios/plugin_wander/scenario.toml --plugin-path target/plugins
```

The scenario has no `plugin_paths` (a build directory is not portable, ASM-113), so `--plugin-path`
is required; without it validation reports `unknown_id` for `example.behavior.wander`.

| Entity | Template | Behavior | Notes |
|---|---|---|---|
| wander-direct | `example.wander.direct` | `example.behavior.wander` (plugin) | defaults: 5 m/s, a new heading every 10 s (every 300 ticks at 30 Hz); ticked in a batch (REQ-B-4) |
| wander-graph | `example.wander.graph` | `example.behavior.wander_graph` (file) | calls the plugin as a `subgraph`: `speed_mps` bound to the graph's `speed` key (12 m/s), `turn_every_s = 5`, output `turns` copied to the key `turns`, inside a 20 s `timeout` (at tick 601 the timeout halts the plugin child, `halt_running`, and the graph fails for that one tick; at tick 602 a fresh walk starts with a new heading) |
| patrol-builtin | `example.wander.patrol` | `builtin.fsm.patrol_and_engage` (built-in) | drives its route east at 10 m/s (no sensor, so it never engages) |

Swapping a behavior is a template change: replace the `ref` in `templates/patrol.toml` by
`example.behavior.wander` and patrol-builtin wanders too (no scenario or code change).

Flat terrain; every entity stays within about 1 km of its start for the 60 s run. Seed 7, 30 Hz,
1800 ticks: the final line is `state_hash=88954c5dc6b52b48` (Linux, ADR 0003); `umbra run --seed 8`
gives another walk (`0dbd8be4ed32dc90`). The determinism gate (gate 4, `plugin_determinism`, 900 ticks)
and the four-process CLI test run this scenario past the first timeout.
