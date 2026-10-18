# walking_skeleton

The M1 walking skeleton as a scenario (ASM-061): three ground vehicles near 52 N 4 E on flat
terrain, each following a square route of four 500 m legs. It reproduces the hard-coded M1 demo
layout (the `umbra demo` command and `umbra_engine::demo` no longer exist) with fixed cruise speeds
instead of the demo's random jitter.

| File | Role |
|---|---|
| `scenario.toml` | seed 42, 30 Hz, 240 s, flat terrain, three placements (two by lat/lon, one by ECEF) |
| `templates/ground_vehicle_base.toml` | `ground_vehicle.base`: the spec 7.1 kinematic limits |
| `templates/tank_mbt_generic.toml` | `tank.mbt_generic` extends the base (slower turn rate, opaque `network.dis` table) |
| `templates/truck_cargo.toml` | `truck.cargo` extends the base (faster, gentler acceleration) |

The tank also overrides `max_speed_mps` in the scenario, to show per-entity overrides.

```
cargo run -p umbra-cli -- validate examples/scenarios/walking_skeleton/scenario.toml
cargo run -p umbra-cli -- run examples/scenarios/walking_skeleton/scenario.toml --ticks 300 --print-every 30
```

This scenario is the subject of the determinism gate (`cargo test -p umbra-engine --test determinism`):
the state hash after every tick must be identical across runs, clock modes and processes.
