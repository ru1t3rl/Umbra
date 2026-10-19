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
| wander-graph | `example.wander.graph` | `example.behavior.wander_graph` (file) | calls the plugin as a `subgraph`: `speed_mps` bound to the graph's `speed` key (12 m/s), `turn_every_s = 5`, output `turns` copied to the key `turns`, inside a 45 s `timeout` |
| patrol-builtin | `example.wander.patrol` | `builtin.fsm.patrol_and_engage` (built-in) | drives its route east at 10 m/s (no sensor, so it never engages) |

Swapping a behavior is a template change: replace the `ref` in `templates/patrol.toml` by
`example.behavior.wander` and patrol-builtin wanders too (no scenario or code change).

Flat terrain; every entity stays within about 1 km of its start for the 60 s run. Seed 7, 30 Hz,
1800 ticks: the final line is `state_hash=3109c928b9d8de94` (Linux, ADR 0003). `umbra run --seed N`
gives another walk. The determinism gate (gate 4, `plugin_determinism`) runs this scenario.
