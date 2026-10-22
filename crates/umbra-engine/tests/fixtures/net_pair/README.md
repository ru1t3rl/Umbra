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
- `fault_abort_publish/`, `fault_abort_poll/`, `fault_disable_publish/` (from WI-9): one adapter,
  `test.net.fault` (defined in `net_fault.rs`), whose `publish` or `poll` fails with a fault on its first
  call, under the `abort` and `disable_capability` policies (`net_fault.rs`).
- `control/` (from WI-9): as `a/` with `publish.control = true` (`net_control.rs`).
- `order/` (from WI-9): one owned entity of `order/templates/tank_ticker.toml`, whose behavior
  `test.behavior.ticker` (registered by `net_order.rs`) emits a `Custom` event every tick, on the
  adapter `lb` next to `b/` (`net_order.rs`).
- `fault_abort_start/`, `fault_abort_stop/` (from WI-9): `test.net.fault` with `publish.control = true`, failing
  on any run-control message (`fail = "control"`) or on `Stop` only (`fail = "stop"`) (`net_fault.rs`).
- `control_short/` (from WI-9): as `control/`, 0.2 s long, so a run reaches its duration in 6 ticks.
- `plugins_a/`, `plugins_fault/` (from WI-12, ASM-410): as `a/` with two more adapters from two
  distinct SDK-built plugin images: `left` (`test.network.net_ok` of `test.plugin.fixture`, feature
  `network`, `key_base = 3000`) and `right` (`test.network.peer_ok` of `test.plugin.net_peer`, feature
  `network_peer`, `key_base = 4000`); both mirror what they publish back as remote entities. In
  `plugins_fault/`, `right` is `test.network.peer_panics_in_poll` (`panic_at_tick = 60`) under
  `disable_capability` (`net_determinism.rs`).
- `plugin_fault_abort/`, `plugin_fault_disable/`, `plugin_bad_params/` (from WI-12): one plugin adapter
  `left`: `test.network.net_panics_in_poll` (`panic_at_tick = 5`) under `abort` and under
  `disable_capability`, and `test.network.net_ok` with `remote_offset = 7`, which its `configure`
  refuses (`net_plugin_faults.rs`).
