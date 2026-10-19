# wander (example plugin)

The example Umbra plugin (M4, spec section 17 acceptance 1): a Rust `cdylib` built with `umbra-sdk`
only (REQ-SDK-3) and `#![forbid(unsafe_code)]`. Plugin `example.plugin.wander` 0.1.0, ABI 0.0.

| Capability | Kind | Params | Interface |
|---|---|---|---|
| `example.behavior.wander` | `behavior` | `speed_mps` (number > 0, default 5), `turn_every_s` (number > 0, default 10) | `speed_mps` bindable (`number`); output `turns` (`integer`); Running forever |

On its first tick, and then every `ceil(turn_every_s * hz)` ticks, the behavior draws a new absolute
heading `2 * pi * next_f64()` from the entity's own RNG stream through the host API; every tick it
emits `SetSpeed(speed)` and `SetHeading(heading)`. It never uses its own RNG or the wall clock
(REQ-K-4), so runs are reproducible and the scenario seed changes the walk. `turns` counts the
headings drawn since the instance was created or halted; `halt` (and the default `halt_running`)
reset the walk. A bound `speed_mps` overrides the configured speed until the binding is unset.

A bound `speed_mps` must be a positive number. A non-positive value makes `set_input` fail, and the
host records that as a **plugin fault** of `example.behavior.wander` (ASM-140): under
`on_plugin_fault = "abort"` (the default) the run stops with the fault (exit 1 from `umbra run`),
under `"disable_capability"` the capability is disabled and its entities get no more intents from
it. Non-finite numbers never reach the plugin: the host refuses them before the call.

## Build and run
```
cargo xtask build-plugins            # or --release; writes target/plugins/wander/
cargo run -p umbra-cli -- list-plugins --plugin-path target/plugins
cargo run -p umbra-cli -- run examples/scenarios/plugin_wander/scenario.toml --plugin-path target/plugins
```

The package is the library plus `plugin.toml` (this directory's manifest, ADR 0022). The host checks
the manifest before it opens the library; `id`, `version` and `abi` must equal the descriptor.

Use it from a template (`[behavior] ref = "example.behavior.wander"`) or as a `subgraph` target in a
graph, binding `speed_mps` with `{ key = .. }` and copying `turns` with `outputs` (see
`examples/scenarios/plugin_wander/`).
