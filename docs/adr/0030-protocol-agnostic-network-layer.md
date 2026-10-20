# ADR 0030: Protocol-agnostic network layer, canonical network model and engine wiring

- **Status**: Accepted by the human 2026-10-20 (proposed by the architect, M6 plan `docs/plans/M6.md`, 2026-10-20).
  Records the **human decision of 2026-10-20** on protocol neutrality (below, "Human decision").
- **Requirement IDs**: REQ-G-4, REQ-N-1..5, REQ-N-8, REQ-K-2, REQ-K-4, REQ-K-5, REQ-T-4, REQ-C-5,
  REQ-S-1, REQ-A-1..3, D-8, D-9, D-16
- **Affected**: `umbra-core` (new `net` module, registry kind `NetworkAdapter`, `SimEvent::Detonation`),
  `umbra-kernel` (remote tracks, ADR 0032), `umbra-net` (new crate), `umbra-templates` (scenario key
  `[[network_adapters]]`), `umbra-engine` (wiring), `xtask` (dependency rules), `umbra-cli` (stderr summary)

## Context
Spec section 11 asks for a network manager (`umbra-net`) that owns publish policy, ghosts, dead
reckoning and the identity map, with protocol adapters (DIS in M6, HLA in M9, user adapters) built on the
public plugin interface (D-9, REQ-SDK-3). The kernel may not depend on any network crate (REQ-A-2) and
the core stays protocol-free (D-9). M5 moved command ingest to the engine boundary (ADR 0024, ASM-149)
and reserved the kernel Ingest phase for network input.

**Human decision (2026-10-20).** Networking MUST be a generic, protocol-agnostic abstraction layer:
`umbra-net` holds the manager, publish policy and thresholds, ghosts, dead reckoning, identity map, echo
suppression and the adapter trait / plugin interface. Nothing in `umbra-net`, `umbra-engine` (or
`umbra-core`, `umbra-kernel`) may know DIS types, PDUs, site/application/entity ids or DIS enumerations;
the neutral model maps to and from protocol specifics only inside an adapter. The loopback adapter must
implement the same interface, and HLA (M9) must fit it without changes. DIS is one adapter plugin of
that layer.

## Options
1. **Neutral canonical model in `umbra-core`, manager in `umbra-net`, engine applies ingest and publish
   at the tick boundary, kernel only dead-reckons ghosts (chosen).**
2. **Kernel Ingest/Publish phases call an injected network port.** Ghost creation needs templates and
   motion-model factories, which only `umbra-engine` has; the port would have to build entities for the
   kernel or the kernel would need the registry. Rejected: widens the kernel.
3. **Adapter does the mapping and identity itself.** Each adapter would keep its own `EntityId` map and
   publish policy (the `add-network-adapter` skill forbids private copies; REQ-N-2 puts policy in the
   manager). Rejected.

## Decision
Option 1.

**Canonical model (`umbra_core::net`, no I/O, no protocol vocabulary).**
- `NetKey(u64)`: an adapter-scoped, opaque entity key. Each adapter encodes its protocol identity into it
  (ADR 0031); the manager never interprets it beyond equality and range checks.
- `KeySpace { base: u64, count: u64 }`: the keys an adapter may use for **local** (owned) entities,
  reported by the configured adapter (ADR 0031).
- `NetEntityType(Arc<str>)`: an opaque, adapter-normalised type token (an adapter documents its format).
- `DeadReckoning`: neutral extrapolation models (ADR 0032).
- `WorldEuler { yaw, pitch, roll }`: body FRD orientation as z-y'-x'' rotations from the ECEF axes,
  radians (ADR 0032). Generic geometry, used so adapters need no trigonometry.
- `NetEntityState { key, entity_type: Option<NetEntityType>, force: Option<Force>, name: Option<Arc<str>>,
  position: Ecef, velocity: Vec3, acceleration: Vec3, angular_velocity: Vec3, orientation: WorldEuler,
  dead_reckoning: DeadReckoning }` (velocity and acceleration ECEF, angular velocity body, SI units).
- `#[non_exhaustive] enum NetMessage { EntityState(NetEntityState), EntityRemoved { key },
  Fire { shooter, target: Option<NetKey>, weapon: WeaponId, position: Ecef },
  Detonation { shooter: Option<NetKey>, target: Option<NetKey>, position: Ecef },
  SimulationControl(ControlAction) }`; `OutboundEvent` and `InboundEvent` are aliases of `NetMessage`
  (REQ-N-1 names; one type, DRY).
- Traits (ISP): `NetworkAdapterConfig: Send + Sync` is what the registry builds (configure = `Factory::build`
  with the adapter's params, no I/O; `fn key_space(&self) -> KeySpace`; `fn start(&self, ctx:
  &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError>`, at most once); `NetworkAdapter: Send`
  with `publish(&mut self, now: SimTime, out: &[OutboundEvent]) -> Result<(), NetError>`, `poll(&mut
  self, now: SimTime, max: usize, into: &mut Vec<InboundEvent>) -> Result<(), NetError>`, `counters(&self)
  -> AdapterCounters`, `stop(&mut self)`. REQ-N-1's `configure`, `start`, `publish`, `poll`, `stop` map
  one to one; `publish` and `poll` never block (I/O on adapter-owned threads, bounded queues,
  drop-and-count). A contract test suite (`umbra_core::net::contract`) runs against every implementation.
- Registry (D-8): new `ComponentKind::NetworkAdapter` with `register_network_adapter` /
  `network_adapter` (factory `Arc<dyn Factory<dyn NetworkAdapterConfig>>`). Plugins register through the
  same descriptor path (ADR 0033).
- `SimEvent::Detonation { shooter: Option<EntityId>, target: Option<EntityId>, position: Ecef }` (additive,
  ADR 0028 planned it for M6); `EntityStateUpdate` stays unadded (snapshots carry state).

**Manager (`umbra-net`, depends on `umbra-core` only).** `NetworkManager` owns, per configured adapter
(in scenario order): the adapter, its identity map and key allocator (ADR 0031), its publish policy
state (ADR 0032), its ghost table and timeouts, its type map (entity type token -> template), its
`enabled` flag and counters. It talks to the world through one port, `trait GhostWorld { create_ghost,
update_ghost, remove_ghost }`, implemented by `umbra-engine`, and reads owned state through a plain
`&[OwnedView]` slice. The in-memory **loopback adapter** (`umbra_net::loopback::LoopbackHub`, REQ-N-8)
implements the same traits; deliveries are synchronous and ordered, so two engines in one process
exchange entities deterministically. A file **recording adapter** (REQ-N-8 SHOULD) is deferred to M7
with recording and replay; the loopback hub's capture list serves debugging in M6 (ASM-238).

**Engine wiring (`umbra-engine::net`).** At load, each `[[network_adapters]]` entry is built through the
registry and started; templates' `network.<name>` tables are handed to the adapter's `configure` as
`{"name", "params", "templates": {<template id>: <table>}}` (REQ-T-4, opaque to the engine). Per tick,
in `Engine::run` and `Engine::pump`: **ingest** (poll every enabled adapter in scenario order, apply
ghost changes through `GhostWorld`, at the same boundary as M5 commands, i.e. before the kernel tick), then
`Kernel::tick` (its Ingest phase dead-reckons ghosts, ADR 0032), then **publish** (owned entities in
`EntityId` order, plus `Fire` events from the tick report and removals). Ghost creation and removal and
inbound fire/detonation become `SimEvent`s prepended to that tick's `TickReport::events` (ADR 0028:
`EntityCreated` now also for ghosts). With no `[[network_adapters]]` nothing is created and nothing
runs, so every golden hash and `run` stdout stays as it is (REQ-S-1).

**Scenario key** (additive, `SCHEMA_VERSION` stays 1, ADR 0010 planned it): `[[network_adapters]]` with
`adapter` (component id), `name` (`[a-z][a-z0-9_]*`, unique, the `network.<name>` template key and the
ghosts' `RemoteSource`), `enabled` (default true), `params` (adapter schema), `[publish]` and `[ghosts]`
tables (ADR 0031, ADR 0032). Located diagnostics as for every scenario key.

**Entity type to template (REQ-N-4).** Default mapping: the `entity_type` string of each template's
`network.<name>` table (the one key name every adapter shares, documented in `docs/net/README.md`),
compared by exact token equality after the adapter normalises inbound types; `[ghosts] type_map` entries
override it; `[ghosts] default_template` catches the rest; with neither, the entity is not mirrored and
counted as `unmapped`. Several templates with one type: the smallest template id wins, with a load
warning. Wildcards are not in M6 (ASM-232).

**Enforcement of neutrality (human decision).** (a) `xtask` rule table: `umbra-core`, `umbra-kernel`,
`umbra-net`, `umbra-engine`, `umbra-server`, `umbra-api-types`, `umbra-cli` may not depend (declared or
resolved) on `umbra-net-dis`, `umbra-net-hla` or any external protocol crate on a deny list (`dis-rs`,
`open-dis*`, names matching `dis-*`/`*-dis`/`*_dis`, `rti*`, `portico*`, `hla*`), each with a negative
fixture in `xtask/tests/check_rules.rs`; (b) a source test `crates/umbra-net/tests/protocol_neutral.rs`
fails if `src/` of `umbra-core`, `umbra-kernel`, `umbra-net` or `umbra-engine` contains the words `DIS`,
`PDU`, `1278`, `site_id`, `application_id`, `SISO` or `RPR` (case rules in ASM-226).

**HLA fit (M9, no interface change).** An HLA adapter is another `NetworkAdapterConfig` plugin: RPR-FOM
`BaseEntity.EntityIdentifier` (federate/site/app/entity) encodes into `NetKey` and the federate's own
identifier range is its `KeySpace`; `EntityType` attributes format to the same type tokens;
`WorldLocation`, `VelocityVector`, `AccelerationVector`, `AngularVelocityVector`, `Orientation` (psi,
theta, phi) and `DeadReckoningAlgorithm` map field for field onto `NetEntityState`; `WeaponFire` and
`MunitionDetonation` interactions map onto `Fire`/`Detonation`; object removal maps onto `EntityRemoved`;
the RTI callback thread fills the adapter's bounded queue exactly as the DIS receive thread does.
Federation synchronisation and time management, if ever needed, are adapter-internal.

## Consequences
- Easier: one manager, one policy, one identity map for every protocol; DIS, loopback and HLA are
  interchangeable; the kernel gains only ghost dead reckoning; runs without adapters are unchanged.
- Harder: the engine applies network input outside the kernel's Ingest phase (as for commands); runs
  with a real network are reproducible only given the received input stream (A-2), which M7 records.
  One frozen M2 unit test (`all_lists_every_kind_once`) needs the two-arm edit precedent of ADR 0011
  (human decision).
- Revisit if an adapter needs per-entity data beyond the canonical fields (add optional fields, never
  protocol types), if type wildcards are wanted, or when M7 adds recording.
