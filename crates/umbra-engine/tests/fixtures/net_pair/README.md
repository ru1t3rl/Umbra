# net_pair fixtures

Provenance: written for M6 step 4 (acceptance tests), modelled on `examples/scenarios/walking_skeleton`
(same motion model and route shape; the positions are rounded to 7 decimals of latitude and longitude).
Used by `crates/umbra-engine/tests/net_loopback.rs`, `net_determinism.rs` and (from WI-9) `net_validate.rs`.

- `templates/`: shared by every scenario here (`template_paths = ["../templates"]`). `tank.mbt_generic`
  declares `entity_type = "tank"` for the adapter names `lb`, `left` and `right`; `truck.cargo` declares
  none, so its entities are published without a type and a peer without `default_template` cannot mirror them.
- `a/` and `b/`: two peers on one loopback hub (adapter name `lb`). The tests register the loopback
  factory under `test.net.loopback` with a distinct key space per engine (a: 1000, b: 2000).
- `c/`: a bridge with two adapters, `left` (`test.net.left`, hub 1) and `right` (`test.net.right`, hub 2).
- Heartbeat 1 s and ghost timeout 3 s in every scenario, so a silent peer's ghosts disappear within
  100 ticks and a live peer's never flap.
