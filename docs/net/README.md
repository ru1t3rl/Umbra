# Network adapters (guide)

How Umbra exchanges entities with other simulations, and how to write a protocol adapter. Requirements:
REQ-N-1..6, REQ-N-8, REQ-K-4, REQ-K-5, REQ-T-4, REQ-X-5, REQ-SDK-3, NFR-4. Decisions: D-49..D-54
([ADR 0030](../adr/0030-protocol-agnostic-network-layer.md) layer and wiring,
[ADR 0031](../adr/0031-identity-map-echo-suppression-and-ghost-lifecycle.md) identity, echo, ghosts,
[ADR 0032](../adr/0032-dead-reckoning-publish-policy-and-orientation.md) dead reckoning and publish policy,
[ADR 0033](../adr/0033-network-adapter-plugin-kind-abi-0-1.md) plugin kind,
[ADR 0034](../adr/0034-dis-adapter-codec-versions-and-transport.md) DIS,
[ADR 0035](../adr/0035-network-adapter-endpoints-and-events-in-api-v1.md) control API); details in
`docs/assumptions/M6.md` (ASM-224..ASM-299, ASM-400..). Status: M6.

The one shipped protocol is DIS: mapping, configuration and transport are in [`dis.md`](dis.md).

## The layers

```
 scenario [[network_adapters]]              templates [network.<name>] tables
              |                                          |
   umbra-engine::net  NetLink  (build, start, ghost world, owned metadata, fault policy)
              |
   umbra-net  NetworkManager   (per adapter: identity map, publish policy, ghosts, type map, counters)
              |   canonical NetMessage (umbra_core::net), no protocol vocabulary
   ---------------------------------------------------------------- NetworkAdapterConfig / NetworkAdapter
              |                         |                                  |
   umbra-net::loopback      umbra-plugin-host network bridge        (your built-in adapter)
   (in-process, built-in)               |  ABI 0.1 (UmbraNetworkVTable)
                               plugin built with umbra-sdk  (umbra-net-dis = DIS over UDP)
```

- **Protocol neutrality** (human decision 2026-10-20, D-49). `umbra-core`, `umbra-kernel`, `umbra-net` and
  `umbra-engine` know no protocol: no PDU, no site or application id, no DIS enumeration. Protocol
  specifics live only inside an adapter. Enforced by `cargo xtask check-deps` (no protocol crate may enter
  the neutral crates, `PROTOCOL_NEUTRAL` in `xtask/src/deps/rules.rs`) and the source test
  `crates/umbra-net/tests/protocol_neutral.rs`.
- **One registry** (D-8). Every adapter, built-in or plugin, is a factory of kind
  `ComponentKind::NetworkAdapter` (`Registry::register_network_adapter`, looked up with
  `Registry::network_adapter`). Plugins register through the descriptor like behaviors. `umbra
  list-plugins` and `GET /api/v1/components` show them.
- **What is available.** `standard_registry()` registers no network adapter: the CLI and server get one
  from a plugin (the DIS plugin `dis`, packaged by `cargo xtask build-plugins` into `target/plugins/dis`,
  capability `umbra.net.dis`). The loopback (`umbra_net::LoopbackHub`, re-exported by `umbra-engine`) is
  registered by embedders and tests that create the hub.

## Canonical model (`umbra_core::net`)

| Type | Meaning |
|---|---|
| `NetKey(u64)` | An adapter-scoped, opaque entity key. The adapter packs its protocol identity into it (DIS: `site << 32 \| application << 16 \| entity`); the manager only compares keys and checks ranges |
| `KeySpace { base, count }` | The keys an adapter issues for **owned** entities: `base < k <= base + count` |
| `NetEntityType` | An opaque type token, normalised by the adapter (DIS: seven decimal fields, `1.1.225.1.1.0.0`) |
| `NetEntityState` | `key`, `entity_type`, `force`, `name` (all optional except the key), `position` (ECEF m), `velocity`, `acceleration` (ECEF axes, SI), `angular_velocity` (body axes, rad/s), `orientation` (`WorldEuler`: yaw, pitch, roll as z-y'-x'' rotations from the ECEF axes to the body FRD axes, rad), `dead_reckoning` |
| `NetMessage` (`#[non_exhaustive]`) | `EntityState`, `EntityRemoved { key }`, `Fire { shooter, target, weapon, position }`, `Detonation { shooter, target, position }`, `SimulationControl(ControlAction)`; `OutboundEvent` and `InboundEvent` are aliases |
| `DeadReckoning` (`umbra_core::dead_reckoning`) | `Static`, `Velocity`, `VelocityRotation`, `AccelerationRotation`, `Acceleration` and the four `Body*` variants; `extrapolate(basis, elapsed)` is the one function sender and receiver use |
| `AdapterCounters` | The adapter's own drop counts: `tx_dropped`, `rx_dropped`, `decode_errors`, `io_errors` |

Adapters need no trigonometry: the manager converts between the kernel's quaternion and `WorldEuler`
(`Quaternion::{to,from}_world_euler`, through `umbra_core::math`, ADR 0003).

The traits (REQ-N-1's `configure`, `start`, `publish`, `poll`, `stop`):

```rust
pub trait NetworkAdapterConfig: Send + Sync {          // what Factory::build returns: configure, no I/O
    fn key_space(&self) -> KeySpace;
    fn start(&self, ctx: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError>; // at most once
}
pub trait NetworkAdapter: Send {
    fn publish(&mut self, now: SimTime, out: &[OutboundEvent]) -> Result<(), NetError>; // never blocks
    fn poll(&mut self, now: SimTime, max: usize, into: &mut Vec<InboundEvent>) -> Result<(), NetError>; // never blocks
    fn counters(&self) -> AdapterCounters;
    fn stop(&mut self);                                  // idempotent, joins every I/O thread
}
```

## The tick boundary

Network input and output happen in the engine, around the kernel tick, at the same boundary as API
commands (ADR 0024, ASM-224); the kernel only dead-reckons ghosts in its Ingest phase:

```
 commands applied (engine handle, between ticks)
 NetLink::ingest   poll every adapter in scenario order, apply messages in delivery order:
                   create / update / remove ghosts, time out stale ghosts -> ghost and inbound events
 Kernel::tick      Ingest (dead-reckon every ghost to now) -> Perception -> Behavior -> Motion -> Events -> Publish
 NetLink::publish  owned entities in EntityId order + this tick's Fire / Detonation / removals, per adapter
 abort check       a plugin or network fault under `abort` ends the run after this tick
```

- The ghost and inbound events are put before the kernel's events of the same tick in `TickReport::events`
  (ASM-248); ghosts get `EntityCreated` (with their template) and `EntityRemoved` events (ADR 0028 extended).
- While the clock is paused there are no ticks, so nothing is polled or published (ASM-240); the adapters'
  own queues drop and count.
- Build order (`NetLink::connect`, before the first spawn): every adapter is configured (the factory gets
  `{"name", "params", "templates": {<template id>: <its [network.<name>] table>}}`, ADR 0030), the type maps
  and the ghosts' motion models are built, then the adapters are started in scenario order; a start error
  stops the ones already started and fails the load (`EngineError::NetworkStart`).
- `umbra validate` and the scenario reader configure every adapter (a dry run) and never start one (no
  socket is opened, ASM-251).
- A scenario without `[[network_adapters]]` creates no link and runs exactly as before M6 (every golden hash
  unchanged; `crates/umbra-engine/tests/net_absent.rs`).
- Shutdown: the link stops every adapter (`NetworkManager::stop`, also on drop) before the plugin host
  releases the images (`crates/umbra-engine/tests/net_shutdown_order.rs`).

## Publish policy (REQ-N-2, ADR 0032)

Per adapter, on simulation time only. After each tick every **Owned** entity is visited in `EntityId` order
and sent when it was never sent on this adapter, or its heartbeat is due, or the receiver's extrapolation of
the last sent state (the same `extrapolate` with the same model) is off by more than
`position_threshold_m` or `orientation_threshold_deg`; and in every case only when `min_interval_s` has
passed since its last send (ASM-285). Thresholds are strict (`>`); heartbeat and `min_interval` are whole
ticks, `ceil(seconds * hz)`. For the `acceleration` model the acceleration is the finite difference of the
last two ticks' velocities; the published angular velocity is zero (ASM-231). An owned entity removed
between ticks sends `EntityRemoved` once, at the next publish, if it was ever sent. `Fire` and `Detonation`
events go out only when their shooter is owned (ASM-278). Run-control actions (start, pause, resume, stop,
step) are sent only with `control = true` (ASM-244); ABI 0.1 cannot carry a step, so plugin adapters do not
receive it (ASM-409).

Every owned entity is published on every enabled adapter; a template without an `entity_type` for that
adapter is published without a type (ASM-239). Enabling an adapter again restarts its policy, so every
owned entity is sent afresh.

## Ghosts and their lifecycle (REQ-N-4, REQ-K-5, ADR 0031)

A ghost is a kernel entity with ownership `Remote`, its `RemoteSource` the adapter's scenario name, the
force from the message, a motion model for identity only (its template's, built once at load, never
stepped, ASM-241) and a track (`RemoteTrack`: the last received basis and its tick). It has no behavior and
no sensors and is never ticked by Behavior or Motion, but owned sensors perceive it; its dead-reckoned state
is in the state hash, its track is not. The kernel never dead-reckons a ghost past its adapter's
`timeout_s` (`Kernel::set_remote_timeouts`, ADR 0032), whatever the manager does.

| Inbound | Effect |
|---|---|
| `EntityState`, unknown key | Type map (below): a template -> the ghost is created (`EntityCreated`); none -> not mirrored, `unmapped` (+ the token in `unmapped_types` and one log warning per adapter and token, for the first 64 distinct tokens of an adapter, `MAX_UNMAPPED_TYPES`); `max` live ghosts reached -> `ghosts_capped` |
| `EntityState`, known key | The ghost's basis is replaced (no smoothing, ASM-234); a state the kernel refuses removes the ghost (`world_errors`) |
| Non-finite number | Dropped, `invalid_state` |
| `EntityRemoved` | The ghost is removed (`EntityRemoved` event) |
| Nothing for `timeout_s` of simulation time | Removed at the next ingest, also for disabled or faulted adapters |
| `Fire` | `SimEvent::Fire` only when shooter and target both map to entities here (ASM-243); changes no state |
| `Detonation` | `SimEvent::Detonation` with whatever maps; a non-finite point is `invalid_state` |
| `SimulationControl` | Counted in `control_ignored`, never acted on (ASM-235) |

**Type map** (per adapter, `umbra_net::TypeMap`): an entry of `[ghosts] type_map` for the token wins, then the
template whose `[network.<name>] entity_type` equals the token (several: the smallest template id, with a load
warning), then `[ghosts] default_template`; with none of them the entity is not mirrored. No wildcards
(ASM-232).

Ghosts appear in `Session::directory`, `GET /entities` (`ownership: "remote"`, `remote_source`) and stream
frames; they cannot be removed through the API (`409 entity_remote`): only their owner removes them.
Timeouts use simulation time (ASM-233): a paused engine keeps its ghosts, and the peers' ghosts of a paused
engine time out.

## Identity and key spaces (REQ-N-5, ADR 0031)

The manager keeps one `IdentityMap` per adapter: a bijection `EntityId <-> NetKey` with a role (`Owned` or
`Remote`) per entry; an insertion that would break it is refused, never overwritten. An owned entity gets its
key at its first publish on that adapter: `base + n`, `n` counting from 1 in first-publish order (so in
`EntityId` order, deterministic). Keys are never reused within a run. When the key space is used up,
further owned entities are not published on that adapter (`keys_exhausted`, one log warning per adapter). The adapter keeps no identity
map of its own; it only packs and unpacks keys.

## Echo handling (REQ-K-5)

Three layers, independent of the adapter getting it right:

1. Only `Owned` entities and events whose shooter is owned are published; a ghost of **any** adapter is
   skipped (so ghosts are never re-published, not even on another adapter).
2. An inbound message whose key (or shooter key) lies in the adapter's own key space is dropped and counted
   in `echo_dropped`: the sender's own broadcast or multicast coming back, or a peer misconfigured with the
   same identity range (both sides then drop each other's traffic; watch `echo_dropped`).
3. An inbound key already bound to an owned entity is dropped as an echo as well.

## Scenario configuration (REQ-T-2, REQ-T-4)

```toml
[[network_adapters]]
adapter = "umbra.net.dis"          # component id of a registered network adapter (plugin or built-in)
name = "dis"                       # [a-z][a-z0-9_]*, unique; selects the templates' [network.dis]; ghost source
enabled = true                     # optional, default true
params = { site = 1, application = 1, bind = "127.0.0.1:47001", destinations = ["127.0.0.1:47002"] }

[network_adapters.publish]         # optional; defaults shown
position_threshold_m = 1.0
orientation_threshold_deg = 3.0
heartbeat_s = 5.0
min_interval_s = 0.0
dead_reckoning = "velocity"        # static | velocity | acceleration
control = false

[network_adapters.ghosts]          # optional; defaults shown
timeout_s = 12.0
max = 10000
max_inbound_per_tick = 4096
default_template = "ground_vehicle.base"                 # optional
type_map = { "1.1.225.1.1.1.0" = "tank.mbt_generic" }    # optional, token -> template id
```

Template side (each template, per adapter name):

```toml
[network.dis]
entity_type = "1.1.225.1.1.0.0"
```

`entity_type` is the one key the engine reads (a non-string value is ignored with a scenario warning) (`umbra_engine::MANAGER_TEMPLATE_KEYS`, mirrored for plugins as
`umbra_sdk::MANAGER_TEMPLATE_KEYS`); every other key of the table belongs to the adapter, which receives the
tables in `configure` and validates them (the DIS adapter refuses any other key and a bad token, naming the
template). Diagnostics and ranges: `docs/templates.md`, "Network adapters". Run example:
`examples/scenarios/dis_pair` (two `umbra run` processes, `--plugin-path target/plugins`).

## Several adapters at once (ASM-410)

A scenario may run any number of adapters at the same time: built-in and plugin ones, adapters from several
plugins, several capabilities of one plugin, and one capability under several names (each entry is its own
instance; one DIS plugin image configured twice is two independent DIS adapters).

- Each adapter has its own key space, identity map, publish tracker, type map, ghost set, counters,
  `enabled` flag and fault state. Give two adapters of one protocol distinct identities (DIS: distinct
  `(site, application)`; see `dis.md`, "Several adapters in one scenario").
- Every tick the adapters run in scenario order: all are polled and applied in that order, then all publish
  in that order.
- **No gateway forwarding**: a ghost received on one adapter is never published on another (echo layer 1).
  Umbra simulates on networks; it does not bridge them. Opt-in forwarding (for example a per-adapter
  `forward_from` list with its own echo and identity rules) is deferred (`docs/plans/M6.md`, "Deferred").
- Tests: `crates/umbra-net/tests/echo.rs` (`ghosts_from_one_adapter_are_not_published_on_another`),
  `crates/umbra-engine/tests/dis_exchange.rs` (DIS next to the loopback; two DIS adapters), the
  `net_determinism` row with two plugin images plus the loopback.

## Determinism (REQ-K-4)

The network layer adds no nondeterminism of its own: no `HashMap` (every map is a `BTreeMap` or a `Vec` in
scenario order), no wall clock (thresholds, heartbeats, timeouts and the DIS timestamps come from `SimTime`
and the tick), no randomness; owned entities in `EntityId` order, adapters in scenario order, inbound
messages in delivery order. Given the same inbound messages at the same ticks, a run is identical.

What arrives from a real network is outside that contract. Over UDP the tick at which a datagram is polled
depends on wall-clock timing, so two runs with live peers differ (ghost states are hashed). Such a run is
reproducible only from its recorded input stream, which M7 records (A-2). The in-process loopback delivers
synchronously inside `publish`, so engines over one hub are deterministic: gate 4
(`crates/umbra-engine/tests/net_determinism.rs`) compares per-tick hashes of two engines across runs, in
every clock mode, across a pause, and with two plugin adapters next to the loopback.

## Faults and host policy

An adapter whose `publish` or `poll` returns `Err` (or, for a plugin, any fault of ADR 0033: a panic, a non-OK
code, an invalid message, too many pushes, a token misuse) is **faulted**: the manager stops it, it is never
called again, and its ghosts time out. The scenario's `on_plugin_fault` applies to every adapter, built-in or
plugin (ASM-252):

- `abort` (default): the run ends after the faulting tick, with `EngineError::NetworkFault` / stop reason
  `network_fault` (`adapter`, `reason`) for a built-in adapter, and with `EngineError::PluginFault` / stop
  reason `plugin_fault` for a plugin adapter (the host's fault wins, ASM-415, ASM-450);
- `disable_capability`: that adapter stays disabled and the run goes on with the others. For a plugin the
  whole capability is faulted in that host, so other adapters of the same capability fault at their next
  call; adapters of other capabilities and images are unaffected (ASM-408 c, ASM-410).

A faulted adapter cannot be enabled again in the session. A failing `configure`, `key_space` or `start` is a
build or start error of the load, not a fault. Adapters must not fault on bad input from the network: they
drop and count it (`decode_errors`, `rx_dropped`).

## Observing adapters

- `umbra run --log-level info` prints one line per adapter at the end (ASM-250):
  `info: net <name>: published=.. received=.. ghosts=.. echo_dropped=.. unmapped=.. tx_dropped=..
  rx_dropped=.. decode_errors=.. invalid_state=.. world_errors=..`, then ` faulted="<reason>"` when faulted
  and ` unmapped_types=<tokens>` when any type was refused (`umbra_engine::net_summary`).
- `umbra run` logs a warning (stderr, level `warn`) the first time each entity type is refused as unmapped,
  per adapter and token, and the first time an adapter has no key left for an owned entity (ASM-236,
  ASM-457); logging never touches simulation state.
- Control API (API 1.1.0, ADR 0035; reference `docs/api.md`, "Network adapters"):
  `GET /api/v1/network/adapters` lists every adapter in scenario order (`name`, `adapter`, `plugin`,
  `enabled`, `faulted`, `fault_reason`, `key_space`, `ghosts`, `counters`), and
  `POST /api/v1/network/adapters/{name}/enable` and `.../disable` change one adapter between ticks
  (idempotent; `404 adapter_not_found`, `409 adapter_faulted` when enabling a faulted one). A disabled
  adapter is neither polled nor published to, stays started (its sockets stay open, its queues drop and
  count), and its ghosts time out. Events of kind `detonation` come from peers; ghosts carry
  `remote_source`.
- Embedders: `Engine::network()` and `Engine::network_mut()` (`NetLink::status`, `set_enabled`,
  `ghosts`), or the handle's `EngineHandle::{network_adapters, set_adapter_enabled}`.

## Writing an adapter

Start with a mapping document `docs/net/<protocol>.md` (canonical fields to protocol fields and back, units
and frames, identity packing and the key space, configuration, unsupported features), as `dis.md` does. Then
choose one of two forms; both implement the same contract and run under the same manager.

**Plugin (the normal case, REQ-SDK-3).** A `cdylib` crate built with `umbra-sdk` only and
`#![forbid(unsafe_code)]`, exactly as a third party would: traits `NetworkAdapterFactory` (configure, no
I/O), `NetworkAdapterConfig` (`key_space`, `start`) and `NetworkAdapter` (`publish`, `poll` into an `Inbox`,
`counters`, `stop`), exported with `export_plugin! { .., network_adapters: [..] }`, packaged with
`plugin.toml` `abi = "0.1"`. The SDK details (borrowed `NetMessage<'a>`, push rules, host-free test
constructors, the force and weapon grammars) are in `docs/plugin-guide.md`, "Writing a network adapter in
Rust (ABI 0.1)". The DIS adapter (`crates/umbra-net-dis`) is the reference, with this split:

| Module | Role |
|---|---|
| `codec` | Pure, panic-free encode and decode (typed errors, bounded lengths, `deny` lints for indexing, arithmetic, `unwrap`, `expect`) |
| `version` | The version strategy, the only place where protocol versions differ |
| `translate` | SDK message mirror <-> protocol messages: key packing, type tokens, force, weapon and model tables, timestamps from the tick |
| `config` | Params schema and validation, template tables (only `MANAGER_TEMPLATE_KEYS` plus the adapter's own keys), security warning from `start` |
| `transport` | Sockets, the adapter-owned I/O threads, bounded queues that drop and count; `stop` joins the threads |
| `adapter` | The SDK traits over the layers above; `lib.rs` only exports |

**Built-in (in-process Rust).** Implement `umbra_core::net::{NetworkAdapterConfig, NetworkAdapter}` and a
`Factory<dyn NetworkAdapterConfig>` whose `build` takes the configure wrapper; register it with
`Registry::register_network_adapter` in the embedding program (as tests do with
`LoopbackHub::factory(id, key_space)`). Such an adapter may not live in a neutral crate if it speaks a
protocol: protocol code belongs in its own crate outside `umbra-core`, `umbra-kernel`, `umbra-net` and
`umbra-engine` (`check-deps`).

**Rules for both.**
- Never block `publish` or `poll`: I/O runs on adapter-owned threads behind bounded queues; drop and count.
  `stop` joins every thread (a plugin's image unloads after it). Plugin I/O threads never call the host API.
- No wall clock, no randomness: timestamps from the `SimTime` of the call.
- No identity map, publish policy, thresholds, ghost handling or echo logic in the adapter: the manager owns
  them. The adapter reports its `KeySpace`, packs keys, and delivers what it receives (including its own
  echoes, which the manager drops).
- Never panic or fault on network input: drop and count malformed datagrams. Refuse bad configuration in
  `configure`, naming the key or template. Several instances must be independent (no statics).
- Bound inbound input at every layer (datagram size, queue length, `max` per poll). Default to a
  loopback-only, receive-only posture and log one warning from `start` when exposed to a network (NFR-4).

**Tests.**
- Golden packets in `tests/fixtures/<protocol>/`, generated independently of the codec (DIS: Python
  generators with two encodings each) with provenance in a README, decoded, encoded byte for byte and
  round-tripped; cross-checked with an independent decoder where one exists (DIS: Open-DIS Python,
  `xcheck_opendis.py`, optional and not a gate); property tests that arbitrary and mutated bytes give typed
  errors, never a panic.
- The contract suite: `umbra_core::network_adapter_contract_tests!(make)` or `(make, feed = f)` (`feed` makes
  the adapter deliver three messages, so the poll bound is checked exactly; adapters with an inbound path must
  use it); the `attrs = [..]` form puts attributes on every generated test. A plugin crate may depend on
  `umbra-sdk` only, so its contract run lives where the host can load it: `crates/umbra-engine/tests/dis_plugin.rs`
  runs the suite against the real DIS plugin through `umbra-plugin-host`, fed over UDP; the loopback runs it in
  `crates/umbra-net/tests/loopback_contract.rs`.
- Two instances exchanging over localhost, the echo case (own broadcast or multicast coming back), several
  instances side by side, `stop` joining within a bound, queue overflow counted; then whole engines
  (`dis_exchange.rs`) and two CLI processes (`cli_dis_pair.rs`). UDP tests pick free ports at run time
  (ASM-246) and wait on conditions, never fixed sleeps.

The skill `.claude/skills/add-network-adapter/SKILL.md` is the step list for agents.

## How HLA will fit (M9, ADR 0030)

An HLA adapter is another `NetworkAdapterConfig` plugin; the interface does not change. The RPR-FOM
`BaseEntity.EntityIdentifier` packs into `NetKey` and the federate's own identifier range is its `KeySpace`;
`EntityType` formats to the same type tokens; `WorldLocation`, `VelocityVector`, `AccelerationVector`,
`AngularVelocityVector`, `Orientation` (psi, theta, phi, the same world-frame angles as `WorldEuler`) and
`DeadReckoningAlgorithm` map field for field onto `NetEntityState`; `WeaponFire` and `MunitionDetonation`
interactions map onto `Fire` and `Detonation`; object removal onto `EntityRemoved`. The RTI callback thread
fills the adapter's bounded queue as the DIS receive thread does. Federation management and time management,
if needed, stay inside the adapter.

## Not in M6

Recording adapter (REQ-N-8 SHOULD; M7 with recording and replay), HLA (M9), gateway forwarding between
adapters, acting on inbound run control (ASM-235), exact body-frame dead reckoning (approximated, ASM-229),
ghost smoothing (ASM-234), entity-type wildcards (ASM-232), wall-clock ghost timeouts.
