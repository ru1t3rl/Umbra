# plugin_c_circle

The C example plugin `examples/plugins/c-circle` (behavior `example.behavior.c_circle`, written in
plain C11 against `umbra_plugin_api.h`) drives two entities on circles (M4 acceptance 3, WI-14).

```
cargo xtask build-plugins
cargo run -p umbra-cli -- validate examples/scenarios/plugin_c_circle --plugin-path target/plugins
cargo run -p umbra-cli -- run examples/scenarios/plugin_c_circle/scenario.toml --plugin-path target/plugins --print-every 225
```

The scenario has no `plugin_paths` (a build directory is not portable, ASM-113), so `--plugin-path`
is required; without it validation reports `unknown_id` for `example.behavior.c_circle`.

| Entity | Template | Params | Circle |
|---|---|---|---|
| circle-cw | `example.c_circle.clockwise` | `speed_mps = 10`, `turn_rate_dps = 12` | clockwise, 30 s per circle, radius about 48 m |
| circle-ccw | `example.c_circle.counter_clockwise` | `speed_mps = 5`, `turn_rate_dps = -6` | counter-clockwise, 60 s per circle, radius about 48 m |

Both entities use the same plugin behavior with different params, so the kernel ticks them in one
`tick_batch` call (REQ-B-4). The plugin's heading is `turn_rate_dps * t`, so both start heading north
(`heading_deg = 0`) at their configured speed. With `--print-every 225` (every 7.5 s) circle-cw
prints headings 89.6, 179.6, 269.6, 359.6 (the motion model follows the commanded heading one tick
later) and is back at its start (`lat=52.0000000 lon=4.0000000`) at ticks 900 and 1800; circle-ccw
is back at its start at tick 1800. Seed 7, 30 Hz, 1800 ticks: `state_hash=3cc077a4c83c4a63` (Linux,
ADR 0003). The plugin draws no random numbers, so `--seed 8` prints the same positions and
headings; only the final hash differs, because it also covers the seeded state.
