# Umbra: Software Specification v0.3

*Umbra, a Computer Generated Forces (CGF) engine: headless Rust simulation core, plugin SDK, optional control API and GUI.*

> **Changelog v0.2 -> v0.3 (amendment A-002, Umbra rename and visual timeline).** Renames the project to Umbra (D-35, ADR 0018: crates `umbra-*`, binary `umbra`, GUI scope `@umbra/`, C ABI prefix `umbra_`/`Umbra`/`UMBRA_`, env prefix `UMBRA_`); adds D-36, the optional and non-blocking M3b (deterministic `--trace`, GeoJSON/KML track export) and M5b (live map, the first GUI slice), and changes M8's scope to extend the M5b app; the map library is decided at M5b. **Compatibility:** no engine milestone acceptance criterion is weakened (M8's criteria are a superset); M0-M3 are content-identical to `pre-umbra` apart from the mechanical rename; file formats (`schema_version` 1), component IDs, the state hash (v2) and the plugin ABI are unchanged. Details: `docs/amendments/A-002-umbra-and-visual-timeline.md`.

> **Changelog v0.1 -> v0.2 (amendment A-001, behavior graphs and subgraphs).** Adds D-26, D-27, section 8.1 (REQ-B-6..17), REQ-X-10..12, `condition`/`action` plugin kinds, and milestone changes to M3, M5, M7 and new M8b. **Compatibility:** M0-M2 deliverables are frozen and unchanged. Existing template and scenario files stay valid; behavior graphs reuse the M2 loader, registry and validation pipeline. Details: `docs/amendments/A-001-behavior-graphs.md`.

> Note: GUI structure and tooling are further specified in `gui/CLAUDE.md` and `docs/frontend-architecture.md`, which supersede section 13 where they differ.

## 0. How to use this document (instructions for implementing agents)

- This spec is the source of truth. Requirements have IDs (e.g. `REQ-K-3`). Reference them in commits and PRs.
- Section 2 records decisions **and why**. Do not reverse a decision silently; if you believe one is wrong, stop and raise it.
- Build in the order of Section 17 (milestones). Do not start a milestone before the previous one's acceptance criteria pass.
- Where this spec says **MUST**, it is mandatory. **SHOULD** means follow unless you document a reason. **MAY** is optional.
- If something is ambiguous, choose the simplest option consistent with Section 16 (coding principles), record the assumption in `docs/assumptions.md`, and continue.
- Never add a dependency from an inner crate to an outer one (Section 4). CI enforces this.

## 1. Purpose and scope

### 1.1 Purpose
A Computer Generated Force (CGF) simulates many autonomous entities (vehicles, aircraft, ships, infantry) with believable behavior, and exchanges them with other simulations over standard protocols (DIS, HLA) or custom ones.

### 1.2 Goals
- **REQ-G-1** The engine MUST run fully standalone: a single binary plus plugins and data files, no GUI, no server, no network required.
- **REQ-G-2** Entities are defined in user-editable **entity template files** that reference behaviors and motion models by ID.
- **REQ-G-3** Users can extend the engine with **plugins (native dynamic libraries)** built with a provided **SDK**: behaviors, motion models, network adapters, sensors, components.
- **REQ-G-4** The engine publishes and receives entities and events through **network adapters**. DIS ships in the box; HLA later; users can write their own.
- **REQ-G-5** Internally simulated ("owned") entities use basic physics/kinematics plus pluggable behavior models.
- **REQ-G-6** An optional **control API** (REST + WebSocket) and a **GUI** (web and Windows) sit on top, never inside, the engine.
- **REQ-G-7** Clean architecture: SOLID, DRY, KISS, testable in isolation.

### 1.3 Non-goals (v1)
- High-fidelity rigid-body dynamics, weather, detailed damage models.
- Distributed/multi-node simulation of a single scenario.
- Sandboxed or out-of-process plugins (design must not preclude them).
- Authentication, multi-tenancy, persistent databases.

### 1.4 Assumptions (change here if wrong)
- **A-1** Target scale: up to ~10,000 owned entities at 10-60 Hz on one machine. Design for it, optimize only when profiling demands it.
- **A-2** Runs SHOULD be **deterministic**: same scenario + seed + plugin versions + same input stream gives identical results. This is cheap to build in now and very expensive to retrofit.
- **A-3** Primary platforms: Windows x64 and Linux x64. macOS is best-effort.

## 2. Decision log

| ID | Decision | Why | Rejected alternatives |
|---|---|---|---|
| D-1 | **Headless engine as a library + CLI**; everything else is a client | Satisfies REQ-G-1; usable in CI, servers, embedded in other tools; GUI can never leak into the domain | Engine coupled to a GUI process |
| D-2 | **Rust** for the engine | Performance, memory safety, no GC pauses in a real-time tick loop | C++ (safety cost), C# (GC, weaker fit for plugin ABI and real-time) |
| D-3 | **ECS** (`bevy_ecs` standalone, or `hecs`) for entity storage | Templates map naturally to "list of components"; cache-friendly; avoids writing our own | Custom object graph (slow, rigid), own ECS (waste) |
| D-4 | **Single-threaded deterministic tick first**; parallelism only via scheduler later | KISS; determinism (A-2); easier debugging | Threads everywhere from day one |
| D-5 | **Fixed timestep** with a `Clock` abstraction (real-time, scaled, as-fast-as-possible, stepped) | Reproducibility, testability; systems never read wall clock | Variable dt |
| D-6 | **C ABI plugin contract** with versioned function tables, loaded as dynamic libraries | Rust has no stable ABI; C ABI allows Rust, C, C++, C# (NativeAOT) plugin authors | `abi_stable` (Rust-only), WASM (overhead; possible future plugin kind) |
| D-7 | **`umbra-sdk` safe Rust wrapper** over the C ABI | Ergonomic plugin authoring; dogfooded by built-in plugins | Raw C only |
| D-8 | **One extension mechanism** for all plugin kinds (single registry, single loader) | DRY, OCP; adding a kind never needs a new loading system | Separate loader per kind |
| D-9 | **Ports and adapters for networking**; canonical internal event model; DIS/HLA are adapters built on the public plugin interface | Core stays protocol-agnostic; dogfooding proves SDK sufficiency; anti-corruption layer | Protocol types inside the kernel |
| D-10 | **Control API is an optional crate** (`umbra-server`) talking to the engine via an in-process command/event channel | Engine remains standalone (REQ-G-1); CLI and server are peers | Server as the engine's entry point |
| D-11 | **REST (OpenAPI generated from Rust via `utoipa`) + WebSocket stream** | Generated clients for any language; no extra gateway language; one source of truth in Rust types | gRPC (extra complexity for browsers), C# gateway (third runtime, extra hop, no current need) |
| D-12 | **Separate wire types crate** (`umbra-api-types`) from domain types | Refactor internals without breaking clients | Exposing domain structs directly |
| D-13 | **React + TypeScript GUI, Tauri for Windows** | One web codebase, small native shell, same toolchain family as engine, can spawn engine sidecar | Electron (heavy), separate native app |
| D-14 | **Templates as TOML** with JSON Schema | Human-friendly, comments allowed, schema gives validation and editor support | JSON (no comments), custom DSL |
| D-15 | **f64 positions; ECEF/WGS-84 canonical frame**, local ENU/NED for motion math | Matches DIS; avoids precision loss globally | f32, flat-earth only |
| D-16 | **Owned vs. remote (ghost) entities are distinct** | Ghosts are dead-reckoned only; avoids running behavior on others' entities and echo loops | Treating all entities the same |
| D-17 | **Declarative behaviors (FSM/behavior tree) in data first; native plugins for complex logic; scripting optional later** | Most users need no code; layering by complexity | Plugins as the only way |
| D-18 | **License: `MIT OR Apache-2.0` for the whole workspace**, plugin API and SDK included (ADR 0001) | Rust ecosystem norm; permissive enough that third parties can ship closed-source plugins, so the SDK needs no separate license | MPL-2.0 engine, proprietary, separate SDK license |
| D-19 | **No hosted CI; the local `verify-and-build` run is the quality gate** (ADR 0004). Read "CI" in this spec as that run. M0 acceptance 1 is waived | The project has no remote and will not use CI; all gates stay scripted commands | GitHub Actions as specified |
| D-20 | **ECS is `hecs`, kernel-internal only** (ADR 0005); entities addressed by `EntityId`, iteration in `EntityId` order | No scheduler of its own (our phase pipeline orders work), small dependency tree, deterministic use is easy to enforce | `bevy_ecs` (multi-threaded executor by default, frequent breaking releases), own ECS |
| D-21 | **`SimRng` is hand-rolled Xoshiro256\*\* with SplitMix64 key expansion; streams keyed by (domain, id)** (ADR 0006) | A dependency bump can never change what a seed produces; per-entity streams keep results independent of iteration and spawn order | `rand`/`rand_chacha`, one global stream |
| D-22 | **State hash is FNV-1a 64 over a versioned little-endian canonical encoding, walked in `EntityId` order; -0.0 and NaN canonicalised; v2 includes route content (ADR 0007 amendment)** (ADR 0007) | Stable by construction, no dependency, cheap; hashes are for equality checks, not security | xxHash/BLAKE3 crates, hashing `Debug` output |
| D-23 | **Component registry and generic `Factory<T>` live in `umbra-core`; component parameters are `serde_json::Value` with hand-written JSON Schemas; no serde derives on domain types** (ADR 0008) | `umbra-templates` may only see `umbra-core` (section 4); one registry for built-ins and plugins (REQ-T-1, D-8); schemas are the component's contract (REQ-T-3) | Registry in `umbra-engine`, `schemars`, serde derives on core types |
| D-24 | **Hand-rolled validator for a documented JSON Schema keyword subset; schemas using other keywords are rejected** (ADR 0009) | Small, deterministic, no heavy dependency; fail-closed | `jsonschema` crate |
| D-25 | **Template and scenario file format v1** (ADR 0010): keys, merge rules, initial routes in placements, `schema_version` policy (adding keys does not bump it), diagnostics `path:line:col: error[code]: message` | REQ-T-1..6, D-14 | Ad-hoc per-milestone formats |
| D-26 | **Behavior graphs: behavior tree as the primary format, with `fsm` and `subgraph` as node types** (A-001) | In a tree a subtree is just a node, so reuse of pre-built pieces (own, built-in, plugin, third-party) needs only a simple result interface; FSM nodes cover mode-based logic; both emit the same intents, so the engine core is unchanged; visual authoring stays simple | FSM-only (reuse needs exit-point design, nested transitions get tangled), tree-only (modal logic awkward), free-form scripting (determinism and test surface) |
| D-27 | **Visual behavior editor is part of the GUI**, driven by an engine **catalog** and **validation** API (A-001) | No second client to build and package; plugins extend the editor automatically; one validation pipeline | Separate editor application; hard-coded node palette in the GUI |
| D-28 | **Behavior traits, the graph definition and catalog types live in `umbra-core`; registry kinds `Condition` and `Action` added; file graphs are registered into a per-scenario copy of the registry; `builtin_registry()` keeps its M2 set, `standard_registry()` adds M3 built-ins** (ADR 0011) | `umbra-templates` and `umbra-behavior` may only see `umbra-core`; one registry for graphs and leaves (D-8); exact kind messages; M7 condition/action plugin kinds map 1:1. One M2 unit test (`all_lists_every_kind_once`) gains two arms, approved | Role tags under `Behavior`; separate leaf registry in `umbra-behavior` |
| D-29 | **Behavior graph file format v1 (`*.behavior.toml`)**: strict tree, node settings as node keys, FSM with `states`/`exits`/`transitions`, `{ param = .. }` references, `{ key = .. }` input bindings, `[editor]` dropped; found by suffix under `template_paths` (ADR 0012) | REQ-B-6..12; reuses the M2 pipeline without a scenario key | New `behavior_paths` key; graphs inline in templates |
| D-30 | **Tick semantics: evaluate from the root every tick; `sequence`/`selector` reactive unless `memory = true`; halt of skipped Running nodes; fixed FSM step rules; state hash stays v2 (revisit v3 with replay in M7)** (ADR 0013). *Amended 2026-10-18:* a halt from a parent (pre-emption, parallel, timeout, FSM, subgraph) is a no-op on nodes that are not Running and resets only the halted Running nodes' own state, so a cooling `cooldown` keeps its timer; `BehaviorInstance::halt` still resets every node, cooldown timers included; a subgraph node's parent halt calls the new additive default method `BehaviorInstance::halt_running` (default: `halt`; graph instances override it with the Running-only halt; the M4 plugin ABI mirrors it or documents the fallback) | REQ-B-8 fixed, documented order; the A-001 example works as written; bounded work per tick | Memory composites by default; event-driven trees |
| D-31 | **Declared, typed blackboard per graph instance; interface params static or bound at subgraph call sites to a parent key (copied in before each tick, read-only in the child); outputs copied out after each tick** (ADR 0014) | REQ-B-10/11/12: every binding checked at load; isolation by construction; dynamic values such as targets flow explicitly | Untyped blackboard with free bindings; static params only |
| D-32 | **Perception: core `Sensor` and `Contact`; kernel Perception phase on pre-tick state; `builtin.visual` (range, fov, line of sight with eye height) in `umbra-engine`; optional placement `force`, different forces are hostile** (ADR 0015) | M3 perception deliverable; deterministic; additive scenario key; O(n^2) accepted until M9 | New `umbra-perception` crate; forces in templates |
| D-33 | **Intent set v1 (`INTENT_SET_VERSION = 1`)**: all REQ-B-2 variants defined; `FollowRoute` expanded by the kernel route code, `SetHeading` applied; `Fire`, `Spawn`, `Despawn`, `Custom` recorded only until M5 (ADR 0016) | M4 ABI mirrors a complete, closed set; reuses the M1 route code | Adding variants only when consumed |
| D-34 | **Semantic versions via the `semver` crate in `umbra-core`** (ADR 0017) | Cargo's range semantics, no dependencies, reused for plugin versions in M4 | Hand-rolled semver subset |
| D-35 | **The project is Umbra, a CGF engine; full rename**: crates `umbra-*`, binary `umbra`, GUI scope `@umbra/`, C ABI prefix `umbra_`/`Umbra`/`UMBRA_`, env prefix `UMBRA_`, schema IDs `urn:umbra:schema:<name>:v<N>`; "CGF" stays the domain term; M0-M3 content-identical to `pre-umbra` apart from the mechanical rename; tags kept (ADR 0018, A-002) | One name before M4 fixes the ABI and header names; cheapest now (no external users) | Docs-only rename; rename after M4 (needs an ABI major bump) |
| D-36 | **Earlier visual feedback**: optional, non-blocking M3b (deterministic `--trace`, offline GeoJSON/KML export) and M5b (live map as the first real GUI slice in `gui/`, extended by M8); map library decided at M5b (A-002) | Behavior debugging and seeing runs long before M8; M8 builds on M5b instead of replacing it; engine milestones unchanged | Wait for M8; a throwaway viewer; any GUI inside the engine (D-1) |

## 3. System overview

```
   Optional clients                         Engine process (umbra)
+--------------------+            +----------------------------------------+
| GUI (React + TS)   |  REST/WS   | umbra-server (optional feature)        |
| Web / Tauri (Win)  |<---------->|   uses EngineHandle (channels)         |
+--------------------+            |----------------------------------------|
| Scripts / CI       |            | umbra-engine (facade, embeddable lib)  |
+--------------------+            |  kernel: clock, scheduler, ECS, bus    |
                                  |  behavior | motion | perception        |
   Template files (TOML)--------->|  templates + registry                  |
   Scenario files (TOML)--------->|  plugin host (C ABI)                   |
                                  |  network layer (adapter trait)         |
                                  +--------------^------------^------------+
                                                 |            |
                                        plugin DLLs     DIS / HLA / custom
```

- **REQ-S-1** `umbra run scenario.toml` MUST run a scenario to completion with no server, no GUI, and no network adapters unless the scenario enables them.
- **REQ-S-2** The engine MUST also be usable as a Rust library (`umbra-engine`) for embedding and integration tests.

## 4. Workspace layout and dependency rules

| Crate | Responsibility | May depend on |
|---|---|---|
| `umbra-core` | Domain types: IDs, time, units, coordinates, canonical events, core traits (`Behavior`, `MotionModel`, `NetworkAdapter`, `TerrainProvider`, `Sensor`). No I/O. | std, small utility crates |
| `umbra-kernel` | ECS world, clock, scheduler, tick pipeline, event bus, RNG service | core |
| `umbra-motion` | Built-in kinematic models (ground, air, sea) | core |
| `umbra-behavior` | Behavior runtime: FSM and behavior tree interpreters, built-in actions | core |
| `umbra-templates` | Parse, validate, resolve inheritance, registry of templates and scenarios | core |
| `umbra-plugin-api` | **C ABI contract only**: `#[repr(C)]` structs, function pointer tables, version constants. Generates `umbra_plugin_api.h` via `cbindgen`. | nothing |
| `umbra-plugin-host` | Load libraries, check manifest/ABI, lifecycle, bridge plugin vtables into core traits | core, plugin-api |
| `umbra-sdk` | Safe Rust wrapper and `export_plugin!` macro for plugin authors | plugin-api (not core) |
| `umbra-net` | Network manager, canonical message model, adapter registry, publish policy and dead reckoning | core |
| `umbra-net-dis` | DIS adapter (built as a plugin using the SDK) | sdk |
| `umbra-net-hla` | HLA adapter (later, same rules) | sdk |
| `umbra-engine` | Facade: wires everything, exposes `Engine`, `EngineHandle`, `EngineConfig` | all engine crates |
| `umbra-api-types` | Wire DTOs for REST and stream; derives schema and TS types | serde |
| `umbra-server` | axum REST + WebSocket, OpenAPI generation | engine facade, api-types |
| `umbra-cli` | Binary, composition root, argument parsing | engine, optionally server |
| `examples/plugins/*` | Reference plugins (behavior, motion, network) | sdk |
| `gui/` | React + TypeScript app, Tauri shell | generated client only |

**Rules (CI-enforced with a dependency-graph test):**
- **REQ-A-1** Dependencies point inward. `umbra-core` depends on no other workspace crate.
- **REQ-A-2** `umbra-kernel` MUST NOT depend on `umbra-server`, `umbra-net-*`, or any protocol crate.
- **REQ-A-3** `umbra-sdk` MUST depend only on `umbra-plugin-api`, so plugin authors never link engine internals.
- **REQ-A-4** The GUI MUST only use the generated API client and stream schema, never engine source.

## 5. Core domain

- **REQ-C-1 Time:** `SimTime` (integer ticks plus fixed dt, convertible to seconds), separate from wall-clock time. Systems receive `dt` and `SimTime` from the kernel only.
- **REQ-C-2 IDs:** `EntityId` (engine-unique u64, never reused within a run); `TemplateId`, `BehaviorId`, `MotionModelId`, `PluginId` as string newtypes with dotted namespaces like `acme.behavior.patrol`.
- **REQ-C-3 Coordinates:** Canonical position is ECEF f64 (meters). Provide conversions to/from WGS-84 geodetic and local ENU/NED frames, unit-tested against published reference points. Orientation is a quaternion; Euler (heading/pitch/roll) are helpers.
- **REQ-C-4 Units:** SI units internally (m, s, rad, kg). Conversion happens only at the edges (templates, network, API). Use newtypes for distinct quantities where cheap.
- **REQ-C-5 Canonical events:** `EntityStateUpdate`, `EntityCreated`, `EntityRemoved`, `Fire`, `Detonation`, `Collision`, `SimulationControl` (start/pause/freeze/stop), plus generic `Custom{ kind, payload }` for plugin-defined events.
- **REQ-C-6 Terrain:** `TerrainProvider` trait (`height_at`, `surface_normal_at`, `line_of_sight`). v1 ships `FlatTerrain`. Real terrain arrives as plugins.

## 6. Simulation kernel

- **REQ-K-1** Entities live in an ECS world. Components are plain data. Systems are functions over components.
- **REQ-K-2 Tick pipeline** (fixed order, each phase a system set):
  1. **Ingest**: drain control commands and network inbound queue, create/update ghosts
  2. **Perception**: sensors populate per-entity contact lists
  3. **Behavior**: decide, emit intents (move to, set speed, fire, ...)
  4. **Motion**: integrate intents into state via motion models
  5. **Events**: resolve interactions (fire/detonation/collision) via the event bus
  6. **Publish**: hand outbound events and state snapshots to the network layer and the snapshot buffer
- **REQ-K-3 Clock modes:** `RealTime`, `Scaled(f64)`, `AsFastAsPossible`, `Stepped` (advance N ticks on command). Pause/resume supported in all modes.
- **REQ-K-4 Determinism (A-2):** Fixed system order; no `HashMap` iteration order in simulation logic (use ordered or indexed structures); a single seeded RNG service (`SimRng`) is the only randomness source, with a per-entity derived stream so spawn order does not alter other entities' streams; no wall-clock or thread-timing reads in systems. Plugins receive RNG through the host API and MUST NOT use their own.
- **REQ-K-5 Ownership:** Every entity has an `Ownership` component: `Owned` or `Remote{ source }`. Behavior and motion systems run only for `Owned`. Remote entities are dead-reckoned from last received state. The network layer MUST NOT re-publish `Remote` entities (prevents echo loops).
- **REQ-K-6 Observability:** The kernel exposes an immutable per-tick **snapshot** (entity states, events since last snapshot) through a bounded buffer, so observers (server, recorder) never block the tick loop. If an observer is slow, snapshots are dropped for that observer, never queued unboundedly.
- **REQ-K-7 Recording/replay (SHOULD, M7):** Record the input command/event stream plus seed, enabling deterministic replay.
- **REQ-K-8 Overrun handling:** If a tick exceeds its time budget in real-time mode, log a warning with per-phase timings; never skip simulation steps silently.

## 7. Entity templates and scenarios

### 7.1 Template files
TOML files in configurable search paths. Example:

```toml
id = "tank.mbt_generic"
extends = "ground_vehicle.base"   # optional single inheritance, child overrides parent
schema_version = 1

[motion]
model = "builtin.ground.kinematic"
params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }

[[sensors]]
kind = "builtin.visual"
params = { range_m = 3000.0, fov_deg = 120.0 }

[behavior]
ref = "builtin.fsm.patrol_and_engage"   # or a plugin id, e.g. acme.behavior.smart_tank
params = { aggression = 0.7 }

[network.dis]                             # protocol-specific mapping lives under network.<adapter>
entity_type = "1.1.225.1.1.1.0"
```

### 7.2 Requirements
- **REQ-T-1** Templates reference motion models, behaviors, sensors and components **by ID only**, resolved through the registry. Templates never know whether an ID is built-in or from a plugin.
- **REQ-T-2** Loading MUST validate: unknown IDs, inheritance cycles, missing required params, wrong types. Errors include file, line, and a clear message. All errors are reported, not just the first.
- **REQ-T-3** Parameter schemas are provided by the component behind each ID (built-in or plugin) as JSON Schema, used for validation and by the GUI for editors.
- **REQ-T-4** `network.<adapter>` sections are opaque to the engine and handed to the named adapter.
- **REQ-T-5** Template files carry `schema_version`; the loader MUST reject newer versions it does not understand and SHOULD migrate older ones.
- **REQ-T-6** `umbra validate <path>` validates templates and scenarios without running them.
- **REQ-T-7 (MAY)** Hot reload of templates for new spawns (existing entities unaffected).

### 7.3 Scenario files
A scenario lists: seed, clock mode, tick rate, plugins to load, enabled network adapters with config, terrain provider, initial entity placements (template id, position, heading, optional param overrides, optional name/group), and optional scheduled events (e.g. spawn at t=60 s). Scenarios use the same validation pipeline as templates.

## 8. Behavior

- **REQ-B-1** Core trait (conceptual): `Behavior::tick(ctx: &BehaviorContext, entity: EntityView, out: &mut IntentBuffer)`. A behavior **reads** state and perception and **writes intents**; it never mutates entity state directly. This keeps behaviors deterministic and replaceable.
- **REQ-B-2 Intents** are a closed, versioned set: `MoveTo`, `FollowRoute`, `SetSpeed`, `SetHeading`, `Hold`, `Fire{target, weapon}`, `Spawn`, `Despawn`, `Custom{kind,payload}`.
- **REQ-B-3 Layers:**
  1. **Declarative behavior graphs** (section 8.1): behavior trees in TOML, with `fsm` and `subgraph` node types, using built-in conditions and actions (`move_to`, `enemy_in_range`, `wait`, ...). Covers most users with no code.
  2. **Plugin behaviors, conditions, actions and subgraphs** via the C ABI for complex logic.
  3. *Later (MAY):* embedded scripting or WASM as another plugin kind (needs an ADR on determinism).
- **REQ-B-4** Plugin behaviors are invoked in **batches** (a slice of entities per call) to limit FFI overhead.
- **REQ-B-5** Per-instance behavior state is held in an engine-managed opaque blob (or a plugin-owned handle with create/destroy callbacks) so replay and snapshotting can be supported.

### 8.1 Behavior graphs and subgraphs (A-001)

- **REQ-B-6 Graph format:** A **behavior graph** is a TOML file (`*.behavior.toml`) with `id`, `schema_version`, `version`, optional `[interface]`, a `root` node, and a `[nodes.*]` table. Graphs load through the same loader, registry and validation pipeline as templates (REQ-T-2). Existing M2 template and scenario files remain valid unchanged; templates keep referencing behaviors through `[behavior] ref` (REQ-T-1), and a graph's ID is a valid `ref`.
- **REQ-B-7 Node kinds (v1):** composites `sequence`, `selector`, `parallel`; decorators `inverter`, `repeat`, `cooldown`, `timeout`, `succeed_always`; leaves `condition` and `action` referencing the catalog by ID; `fsm` (states, transitions with conditions, optional hierarchical child states, named exits mapped to Success/Failure); `subgraph`.
- **REQ-B-8 Tick semantics:** every node returns Success, Failure or Running. Evaluation order is fixed and documented. Nodes never mutate entity state; they only emit intents (REQ-B-1).
- **REQ-B-9 Subgraphs:** a `subgraph` node references another graph by ID with an optional semver range (`version`) and `params`. It resolves through the registry; whether the target is a file, built-in or plugin is invisible to the referencing graph (D-8).
- **REQ-B-10 Interface:** a reusable graph MAY declare `[interface]`: typed parameters (JSON Schema, REQ-T-3), optional named outputs, and its result semantics. Call sites are validated against it (missing, unknown or wrongly typed params are errors).
- **REQ-B-11 Isolation:** every subgraph instance has its own node state and scoped blackboard. Data crosses only through declared params (in) and outputs (out) bindings. No hidden shared state between instances or graphs.
- **REQ-B-12 Validation:** loading MUST reject unknown refs, version mismatches, subgraph cycles (direct or indirect), nesting deeper than a configurable limit (default 16), unreachable FSM states, FSMs without an initial state, and invalid interface bindings. All errors are reported with file and line (REQ-T-2).
- **REQ-B-13 Provenance:** each graph has a source tag (`builtin`, `file`, `plugin`) exposed by the catalog. `builtin` and `plugin` graphs are read-only for editors.
- **REQ-B-14 Versioning:** graphs carry a semantic version; a breaking interface change requires a major bump; `schema_version` follows REQ-T-5.
- **REQ-B-15 Conditions (v1):** selected from a catalog of parameterized built-in or plugin conditions (e.g. `enemy_in_range`, `health_below`, `timer_elapsed`). No free-form expression language or scripting in v1.
- **REQ-B-16 Determinism:** graph evaluation uses only `SimTime`, perception data and `SimRng` (REQ-K-4). Editor metadata (`[editor]` tables: node positions, comments, colors) is ignored by the engine and excluded from state hashes and replay.
- **REQ-B-17 Catalog:** the engine exposes a catalog of node types, conditions, actions and available graphs/subgraphs, each with id, description, source, version and param JSON Schema. It is the single data source for editors (REQ-X-10).

## 9. Motion and physics

- **REQ-M-1** `MotionModel` trait: `step(state, intents, dt, terrain) -> state`. Motion models translate intents into physically bounded movement.
- **REQ-M-2 Built-ins (v1):** `ground.kinematic` (speed, turn rate, acceleration limits, terrain following via `TerrainProvider`), `air.kinematic` (speed, altitude, climb/bank limits), `sea.kinematic` (speed, turn radius, constant surface height).
- **REQ-M-3** Full rigid-body/vehicle dynamics (e.g. `rapier`) is a later alternative implementing the same trait. Not v1.
- **REQ-M-4** Motion models are selected per template by ID, so adding one never changes the kernel (OCP).

## 10. Plugin system

The highest-risk area. Keep it small, versioned, and heavily tested.

### 10.1 Contract
- **REQ-P-1** Plugins are dynamic libraries (`.dll` / `.so`) exporting exactly one symbol: `const UmbraPluginDescriptor* umbra_plugin_entry(const UmbraHostApi* host);`
- **REQ-P-2** All plugin API structs are `#[repr(C)]`, begin with `abi_version` and `struct_size` fields, and are **append-only** within a major ABI version. Hosts and plugins check sizes before reading newer fields.
- **REQ-P-3 Descriptor** contains: plugin id, name, semantic version, required ABI major/minor, and a list of **capabilities** (kind + id + param JSON Schema + vtable pointer).
- **REQ-P-4 Capability kinds (v1):** `behavior`, `motion_model`, `network_adapter`, `sensor`, `component`; `condition` and `action` (custom behavior-graph leaves) are added in M7. New kinds extend the enum and vtable set, never add new loading paths (D-8).
- **REQ-P-5 HostApi** provides: logging, `SimRng` access, current `SimTime`, terrain queries, event emit, memory helpers, capability registration. Plugins interact with the engine only through this.
- **REQ-P-6 Ownership:** memory crossing the boundary is freed by the side that allocated it, via a provided free function. Prefer host-allocated output buffers. Strings are UTF-8 pointer+length. Params and payloads cross as UTF-8 JSON or length-prefixed byte buffers.
- **REQ-P-7 Panics:** the SDK wraps every exported function in `catch_unwind` and converts panics to error codes. On plugin error the host logs, marks the capability faulted, and applies the scenario's `on_plugin_fault` policy (`abort` | `disable_capability`).
- **REQ-P-8 Lifecycle:** `load -> describe -> configure(params) -> init -> tick/call* -> shutdown -> unload`. Unload only after all handles are released.
- **REQ-P-9 Versioning:** Host rejects plugins whose ABI major differs or whose minor is newer than the host supports, with a clear message.
- **REQ-P-10 Discovery:** plugin directories set in engine config and scenario; an optional `plugin.toml` next to each library carries metadata checked *before* loading the library.
- **REQ-P-11 In-process risk:** v1 plugins run in-process and can crash the engine. This is accepted and documented. The vtable design MUST NOT assume a shared address space beyond pointers passed in calls, so an out-of-process or WASM host can be added later.

### 10.2 SDK
- **REQ-SDK-1** `umbra-sdk` offers idiomatic Rust traits (`Behavior`, `MotionModel`, `NetworkAdapter`, `Sensor`) and an `export_plugin!` macro generating the entry point, vtables, panic guards and descriptor.
- **REQ-SDK-2** `umbra_plugin_api.h` generated by `cbindgen` is published so C/C++ authors can write plugins. A C# NativeAOT sample SHOULD exist (M9).
- **REQ-SDK-3** The DIS adapter and example plugins MUST be built with the SDK exactly as third parties would. No private back doors.
- **REQ-SDK-4** Ship `umbra new-plugin <kind>` scaffolding and documented examples under `examples/plugins/`.

## 11. Networking

- **REQ-N-1** Canonical model lives in `umbra-core`; `NetworkAdapter` trait (and C ABI equivalent): `configure`, `start`, `publish(&[OutboundEvent])`, `poll(&mut Vec<InboundEvent>)`, `stop`. Adapters MUST NOT block the tick; I/O runs on adapter-owned threads with bounded queues.
- **REQ-N-2** The network manager (`umbra-net`) owns **publishing policy** independent of protocol: which owned entities to publish, thresholds/heartbeats (position/orientation deltas, max interval), rate limiting. Adapters only translate.
- **REQ-N-3** Multiple adapters may be active at once.
- **REQ-N-4 Inbound:** adapters emit canonical events. The kernel creates/updates `Remote` entities, maps remote entity types to templates via configurable mapping (default: template-declared `network.<adapter>` data), and removes stale ghosts after a timeout.
- **REQ-N-5 Identity:** the network layer maintains a bijective map `EntityId <-> adapter-specific id` (e.g. DIS site/application/entity). Adapter config supplies site/application IDs.
- **REQ-N-6 DIS adapter (M6):** UDP unicast/broadcast/multicast; IEEE 1278.1 PDUs: Entity State, Fire, Detonation, Start/Resume, Stop/Freeze at minimum; DIS version configurable (v6/v7); dead reckoning algorithm populated and used for ghosts; ECEF conversion per standard. Reuse a maintained codec crate if license-compatible, otherwise implement with a golden-packet test suite.
- **REQ-N-7 HLA adapter (M9):** requires a vendor RTI (Pitch, MAK) or Portico, wrapped via its C/C++ API; FOM mapping (e.g. RPR-FOM) configurable. Built as an optional separate plugin so the core build has no RTI dependency.
- **REQ-N-8** A **loopback/in-memory adapter** MUST exist for tests; a **recording adapter** SHOULD exist for debugging.

## 12. Control API and standalone operation

### 12.1 Standalone control (no server)
- **REQ-X-1** `EngineHandle` is a cloneable, thread-safe handle exposing commands (`load_scenario`, `start`, `pause`, `resume`, `step(n)`, `stop`, `spawn(template, state)`, `remove(entity)`, `set_clock`) and subscriptions (snapshots, events). Commands go through a bounded channel into the **Ingest** phase, so they apply at deterministic tick boundaries.
- **REQ-X-2** CLI commands: `run`, `validate`, `list-templates`, `list-plugins`, `new-plugin`, `schema` (emit JSON Schemas). Machine-readable output with `--json`.
- **REQ-X-3** Logging via `tracing`; structured, configurable level; per-phase timing spans.

### 12.2 Server (optional crate, feature `server`)
- **REQ-X-4** REST under `/api/v1/` generated with `utoipa`. The OpenAPI spec is served at `/api/v1/openapi.json` **and** committed at `api/openapi.json`; CI fails if the committed file differs from generated output.
- **REQ-X-5 Minimum endpoints:** engine status; load/unload scenario; start/pause/resume/step/stop; set clock; list templates (with param schemas); list plugins and capabilities; list/enable/disable network adapters; spawn/remove/get entity; list entities (paged, filtered).
- **REQ-X-6 Stream:** WebSocket `/api/v1/stream`. Clients subscribe with filters (bounding box, entity ids, event kinds) and a max rate. Frames are **batched snapshots/deltas**. v1 encodes JSON; encoding is negotiated (`json` | `msgpack`) so the format can change without breaking clients. Stream message types live in `umbra-api-types`, are exported as JSON Schema/TypeScript types, and documented with AsyncAPI.
- **REQ-X-7** The server holds only an `EngineHandle`; it MUST NOT touch ECS data directly. A slow WebSocket client never slows the tick (REQ-K-6).
- **REQ-X-8** Multiple concurrent clients are allowed; no command restrictions or authentication in v1. Server binds to localhost by default; exposing it is an explicit config choice. Document this limitation.
- **REQ-X-9** Errors use a consistent problem-details JSON shape with stable error codes.
- **REQ-X-10 Catalog:** `GET /api/v1/catalog/behavior` returns node types, conditions, actions and graphs/subgraphs with schemas, versions and provenance (REQ-B-17).
- **REQ-X-11 Validation:** `POST /api/v1/behaviors/validate` validates a graph document with the same pipeline as `umbra validate`, returning all errors with locations. It changes no state.
- **REQ-X-12 (SHOULD) Graph files:** list/get/put/delete of user behavior graph files in configured content directories; reject path traversal, write atomically, refuse to modify `builtin` or `plugin` sources; changes apply to new spawns only (REQ-T-7).

## 13. GUI (separate deliverable, after M5)

- React + TypeScript, built with Vite; small store (e.g. Zustand); map library decided at M5b behind a `MapView` component interface.
- **Live map first (A-002, milestone M5b):** the live map with moving entities is the first GUI slice, built after M5 as the start of the real `gui/` app (generated client, stream store, `MapView`, dashboard shell with the map only). M8 extends that app with the remaining v1 features and Windows packaging instead of replacing it.
- API client generated from `api/openapi.json` (`openapi-typescript` + `openapi-fetch`); stream types generated from the shared schema. No hand-written duplicates.
- **Windows app:** Tauri shell loading the same build; may spawn `umbra` as a sidecar or connect to a remote engine URL.
- v1 features: connect to engine, load scenario, run controls, map with live entities, entity inspector, template browser, plugin/adapter status, log view. Template/scenario editing UI comes later, driven by JSON Schemas.
- **Behavior editor (A-001, milestone M8b):** visual node-and-arrow graph editing for behavior graphs, built from the engine catalog (REQ-X-10) and validated by the engine (REQ-X-11). Includes a subgraph library panel, "convert selection to subgraph", expand/collapse and open-in-tab for subgraphs, and read-only display of `builtin` and `plugin` graphs. Layout metadata is stored in the graph's `[editor]` table.
- The GUI is never required for any engine feature (REQ-G-1).
- Project structure, tooling (Bun) and layering rules: see `gui/CLAUDE.md` and `docs/frontend-architecture.md`.

## 14. Non-functional requirements

- **NFR-1 Performance (A-1):** 10,000 owned kinematic entities with declarative behaviors at 30 Hz on a modern 8-core desktop using one simulation thread, median tick under 25 ms. Provide a `bench` scenario and `criterion` benchmarks per phase; regressions over 15% fail the benchmark CI job.
- **NFR-2 Reliability:** No `unwrap`/`expect` on external input. Typed errors (`thiserror`) in libraries, `anyhow` only in binaries. `unsafe` confined to `umbra-plugin-host`, `umbra-sdk`, `umbra-plugin-api`, each block with a `// SAFETY:` comment and tests; `#![forbid(unsafe_code)]` elsewhere.
- **NFR-3 Portability:** Windows and Linux CI builds, including plugin loading tests on both.
- **NFR-4 Security:** Plugins are trusted native code (document clearly). Server is localhost-only by default. `cargo audit` and `cargo deny` (licenses, advisories) in CI.
- **NFR-5 Documentation:** rustdoc on all public items; `docs/` with architecture overview, plugin authoring guide, template reference (generated from schemas), network adapter guide.
- **NFR-6 Licensing:** Decide the project license before M1.

## 15. Testing strategy

- **Unit:** coordinate math, motion models, FSM/BT interpreters, template resolution. Coordinate conversions tested against published reference values.
- **Property tests (`proptest`):** coordinate round-trips, template inheritance, clock monotonicity.
- **Determinism tests:** run each reference scenario twice (and under different clock modes) and compare per-tick state hashes. This test MUST gate every PR.
- **Plugin tests:** a test plugin built in CI and loaded by the host; cases for ABI mismatch, missing symbol, plugin panic, wrong struct size, double load, unload ordering.
- **Behavior graph tests (A-001):** subgraph cycle and depth-limit rejection, version-range mismatch, interface param validation, blackboard isolation between two instances of the same subgraph, FSM exit mapping, determinism with nested graphs, `[editor]` data not affecting state hashes.
- **Network tests:** DIS golden-packet encode/decode; loopback adapter integration; echo-loop prevention.
- **API tests:** contract tests against the generated OpenAPI; WebSocket back-pressure test (slow client does not slow ticks).
- **End-to-end:** CLI runs a scenario headless and asserts on recorded output.
- **CI jobs:** fmt, clippy (`-D warnings`), tests (Windows + Linux), cargo-deny/audit, OpenAPI drift check, benchmarks, dependency-rule check (REQ-A-*).

## 16. Coding principles

- **SRP:** one responsibility per crate and module; systems are small and single-purpose.
- **OCP:** new behaviors, motion models, protocols and sensors are added by registering, never by editing kernel code.
- **LSP:** all implementations honor the documented trait contract; contract tests live next to each trait and run against every implementation (built-in and example plugins).
- **ISP:** small traits (`Behavior`, `MotionModel`, `Sensor`, `NetworkAdapter`, `TerrainProvider`), not one large interface.
- **DIP:** the kernel depends on traits in `umbra-core`; concrete implementations are injected in `umbra-engine`/`umbra-cli`.
- **DRY:** one registry, one plugin loader, one validation pipeline, one canonical event model.
- **KISS:** no speculative generality; add an abstraction when a second implementation exists or the spec requires it. Single-threaded tick until profiling says otherwise.
- **Style:** `rustfmt` defaults, selective `clippy::pedantic`, short functions, documented public APIs, no global mutable state, time and randomness injected.
- **Git:** small PRs per requirement ID, conventional commits, every PR updates the docs and tests it affects.

## 17. Milestones and acceptance criteria

| # | Milestone | Deliverable | Acceptance criteria |
|---|---|---|---|
| M0 | Foundations | Workspace, CI, lint/deny config, license, dependency-rule check, `umbra-core` skeleton (IDs, time, units) | CI green on Windows + Linux; dependency-rule check fails when a rule is violated (tested) |
| M1 | Walking skeleton | Kernel, clock modes, ECS, `SimRng`, coordinates, `ground.kinematic`, hard-coded entity, CLI prints state, state hash | Determinism test passes; coordinate tests pass; entity follows a waypoint list |
| M2 | Templates and scenarios | TOML loader, inheritance, registry, validation errors, `validate` command, JSON Schemas | REQ-T-1..6 tested including cycle and unknown-ID errors; example scenario with 3 templates runs |
| M3 | Declarative behavior | Behavior-tree interpreter with `fsm` and `subgraph` nodes, graph format and loader (REQ-B-6..17), subgraph resolution, versioning and cycle detection, built-in conditions/actions, catalog data model, intents pipeline, perception (visual sensor) | Patrol-and-engage scenario (tree with an embedded FSM and a reusable subgraph) runs deterministically; swapping a behavior needs only a template change; REQ-B-6..17 tested incl. cycle, version-mismatch and scoped-blackboard isolation; all M2 template/scenario fixtures still pass unchanged |
| **M3b** (new, optional) | Run trace and track export | `umbra run --trace` (per tick: contacts, behavior nodes ticked with status, intents); `umbra run --export` of a run as GeoJSON and KML tracks; deterministic and offline | Trace and exports byte-identical across runs and 4 processes for both reference scenarios; enabling them leaves state hashes unchanged (`849026656b7b42d2`, `3a7bfda0a76111b9`); patrol_and_engage trace shows blue-1's first contact at tick 328 and first `Fire` at 1130; GeoJSON passes an RFC 7946 structure test, KML well-formed with one `gx:Track` per entity; manual playback check recorded; all M0-M3 tests pass unchanged |
| M4 | Plugin ABI v0 and SDK | `umbra-plugin-api`, host, SDK, `export_plugin!`, behavior plugin kind, example plugin, generated C header | Example plugin loaded and driving entities; all plugin failure tests pass; a C example compiles against the header |
| M5 | Engine facade and control | `EngineHandle`, snapshots, `umbra-server` REST + WebSocket, committed OpenAPI, stream schema, behavior catalog and validation endpoints (REQ-X-10, X-11) | OpenAPI drift check passes; a script or generated client runs a scenario end to end; slow-client test passes; catalog and validate endpoints covered by contract tests |
| **M5b** (new) | Live map (first GUI slice) | `gui/` Bun workspace and tooling; `@umbra/engine-client` (generated), `@umbra/engine-stream`, `@umbra/ui-kit` (minimum), `@umbra/map-view` (`MapView` interface and first implementation), `@umbra/shell-dashboard` (map only), app with runtime config; map-library ADR | Playwright against a real headless engine running `walking_skeleton` in stepped mode: all entities shown, positions after `step(n)` match the engine's entity list within tolerance; `verify-frontend` passes and `gen:api` gives no diff; GUI uses no engine source; no engine acceptance criterion changed |
| M6 | Networking and DIS | `umbra-net`, loopback adapter, DIS adapter (as plugin), publish thresholds, ghosts and dead reckoning | Two engine instances exchange entities over DIS on localhost; no echo loops; golden-packet tests pass |
| M7 | More plugin kinds and replay | Motion model, sensor, network, condition and action plugin kinds; air/sea models; recording/replay | A third-party-style example plugin of each kind in `examples/`; replay reproduces state hashes |
| **M8** (changed scope) | Full GUI and Windows app | Extends the M5b app (MapView, generated client and stream from M5b): entity inspector, scenario/run controls, template browser, plugin/adapter status, log view, settings, dashboard extended; Tauri packaging for Windows | Web and Windows builds both show live entities from a running engine; selecting an entity shows its details in the inspector; the template browser lists templates with parameter forms from their schemas; plugin status lists plugins, capabilities and faults; the M5b e2e tests still pass; GUI uses no engine source |
| M8b | Behavior editor | `@umbra/behavior-editor`, `@umbra/shell-behaviors`, subgraph library, convert-selection-to-subgraph, validation display | A user builds a tree with a nested subgraph in the GUI, saves it, and the engine runs it; invalid graphs show engine validation errors; `builtin` and `plugin` graphs are read-only |
| M9 | HLA and hardening | HLA adapter with chosen RTI, C# plugin sample, benchmark tuning, docs | HLA interop test against a reference federate; NFR-1 target met |

Ordering (A-002): the mandatory engine milestones M0, M1, M2, M3, M4, M5, M6, M7, M9 keep strict order. M3b is optional and non-blocking: it may be picked up any time after M3 passes, only when the human asks, and no milestone depends on it. M5b may start once M5 passes and may run alongside M6 and M7; no engine milestone waits for it. M8 starts after both M7 and M5b; M8b follows M8.

## 18. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Plugin ABI churn breaks users | Tiny `umbra-plugin-api`, append-only structs, versioned descriptors, example plugins (M4) before adding more kinds |
| Plugin crash takes down the engine | Panic guards, fault policy, documented risk; vtable design allows out-of-process hosting later |
| FFI overhead for behaviors | Batched calls (REQ-B-4); declarative behaviors for the common case |
| Determinism broken by plugins or hash ordering | Determinism test gates PRs; host-provided RNG and time only; ordered structures |
| DIS interoperability subtleties (dead reckoning, coordinates) | Golden packets; test against a second DIS tool during M6 |
| HLA RTI licensing and availability | Isolated optional plugin; Portico as an open option; scheduled last |
| Scope creep into high-fidelity physics | Non-goal for v1; trait boundary keeps it addable |

## 19. Open questions (resolve before or during the named milestone)

1. **M0 (resolved 2026-10-17, D-18):** ~~Project license, and whether the SDK/plugin API gets a more permissive license than the engine.~~ `MIT OR Apache-2.0` everywhere.
2. **M1 (resolved 2026-10-17):** ~~Confirm A-1 (scale) and A-2 (determinism). Changes affect the scheduler and plugin RNG rules.~~ Both confirmed by the human as written; A-2 scope is same-OS (ADR 0003).
3. ~~M3: Behavior trees vs FSM~~ **Resolved (A-001, D-26):** behavior tree primary, with `fsm` and `subgraph` as node types.
4. **M5:** Is command access control needed (more than one operator)? Currently out of scope.
5. **M6:** Reuse an existing DIS crate or implement PDUs in-house (depends on license and coverage).
6. **M5b** (moved from M8 by A-002, D-36): Map library (MapLibre 2D vs Cesium 3D). Decided by an ADR in M5b; the A-002 proposal is MapLibre GL JS.
7. **M9:** Which RTI(s) to support first, and which FOM (RPR-FOM version).
