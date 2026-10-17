# patrol_and_engage

The M3 acceptance scenario (spec section 17, M3): a behavior tree with an embedded FSM and a
reusable subgraph, loaded from files, run through the kernel with the `builtin.visual` sensor.

```
cargo run -p cgf-cli -- validate examples/scenarios/patrol_and_engage
cargo run -p cgf-cli -- run examples/scenarios/patrol_and_engage/scenario.toml
```

| Entity | Template | Behavior | Notes |
|---|---|---|---|
| blue-1 | `example.tank.blue` | `example.behavior.patrol_and_engage` (file) | patrols a two-leg route east (50 m, then 800 m), engages red-1, fires, resumes the patrol |
| red-1 | `example.truck.red` | `example.behavior.follow_route` (file) | drives west at 20 m/s along y = +200 m |
| blue-2 | `example.tank.builtin` | `builtin.fsm.patrol_and_engage` (built-in) | the spec 7.1 template shape, `aggression = 0.7` |

The engage state rolls a `chance` of 0.5 before each shot, so the seed decides on which ticks `Fire` is emitted (`cgf-cli run --seed N`).

`Fire` is recorded in the snapshot only until M5 (ADR 0016); nothing is destroyed.

Swapping a behavior is a template change: replace the `ref` in `templates/tank_blue.toml` by
`example.behavior.follow_route` and blue-1 just drives its route (AT-17).

Status: written in M3 step 4 as the subject of the acceptance tests. The layout has not been run
yet; WI-16 runs it and records the ticks of first contact, approach and first `Fire` in
`docs/plans/M3.md`.
