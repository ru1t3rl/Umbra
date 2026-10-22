# m7 fixtures

Provenance: written for M7 step 4 (WI-1, test-engineer), modelled on `../net_pair` (same ground motion
model and loopback template tables). Used by `spawn_despawn.rs`, `spawn_determinism.rs`, `record.rs`,
`replay.rs`, `replay_determinism.rs` and `replay_dis.rs` through `m7_support`.

The behavior `test.behavior.launcher` is defined in `m7_support/mod.rs` (registered by the tests, not
by the engine). Its params: `spawn_at` (its own tick numbers, 1 = first tick), `template`, `per_tick`
`despawn_at` and `ping` (also emit a `Custom` intent every tick). A spawned entity first acts in the tick after it was spawned (ADR 0052).

- `templates/`: `ground_vehicle.base` plus the carriers (`m7.carrier_*`, one per scenario, differing in
  the launcher params), the passive children (`m7.child`, `m7.child_a`, `m7.child_b`) and the
  self-removing ones (`m7.selfish` removes itself on its first tick, `m7.selfish_late` on its 30th).
  The base template declares `[network.lb] entity_type = "tank"` (inherited by all) so the `spawn_net` rows can publish.
- `spawn_basic/`: one carrier (blue) that spawns one `m7.child` on its second tick and pings every tick.
- `spawn_order/`: two carriers; each spawns two children on its first tick (`m7.child_a` and `m7.child_b`).
- `spawn_leaver/`: one carrier that spawns a child and despawns itself on its first tick.
- `spawn_selfish/`: one carrier that spawns an `m7.selfish` on its second tick.
- `spawn_flood/`: one carrier that emits 300 spawns on its first tick (cap test, `MAX_SPAWNS_PER_TICK`).
- `spawn_missing/`: one carrier that spawns the unknown template `m7.missing` on its first tick.
- `spawn_net/`: as `spawn_basic` with `m7.selfish_late` children and the loopback adapter `lb`; its
  peer is `../net_pair/b`.
- `handle_run/`: two ground vehicles on a route and the loopback adapter `lb`, long enough that a
  handle-driven run does not end during a test (replay of handle runs, `replay.rs`).
