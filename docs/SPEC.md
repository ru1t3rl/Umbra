# CGF Engine: Software Specification v0.1

*Computer Generated Forces engine: headless Rust simulation core, plugin SDK, optional control API and GUI.*

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
| D-7 | **`cgf-sdk` safe Rust wrapper** over the C ABI | Ergonomic plugin authoring; dogfooded by built-in plugins | Raw C only |
| D-8 | **One extension mechanism** for all plugin kinds (single registry, single loader) | DRY, OCP; adding a kind never needs a new loading system | Separate loader per kind |
| D-9 | **Ports and adapters for networking**; canonical internal event model; DIS/HLA are adapters built on the public plugin interface | Core stays protocol-agnostic; dogfooding proves SDK sufficiency; anti-corruption layer | Protocol types inside the kernel |
| D-10 | **Control API is an optional crate** (`cgf-server`) talking to the engine via an in-process command/event channel | Engine remains standalone (REQ-G-1); CLI and server are peers | Server as the engine's entry point |
| D-11 | **REST (OpenAPI generated from Rust via `utoipa`) + WebSocket stream** | Generated clients for any language; no extra gateway language; one source of truth in Rust types | gRPC (extra complexity for browsers), C# gateway (third runtime, extra hop, no current need) |
| D-12 | **Separate wire types crate** (`cgf-api-types`) from domain types | Refactor internals without breaking clients | Exposing domain structs directly |
| D-13 | **React + TypeScript GUI, Tauri for Windows** | One web codebase, small native shell, same toolchain family as engine, can spawn engine sidecar | Electron (heavy), separate native app |
| D-14 | **Templates as TOML** with JSON Schema | Human-friendly, comments allowed, schema gives validation and editor support | JSON (no comments), custom DSL |
| D-15 | **f64 positions; ECEF/WGS-84 canonical frame**, local ENU/NED for motion math | Matches DIS; avoids precision loss globally | f32, flat-earth only |
| D-16 | **Owned vs. remote (ghost) entities are distinct** | Ghosts are dead-reckoned only; avoids running behavior on others' entities and echo loops | Treating all entities the same |
| D-17 | **Declarative behaviors (FSM/behavior tree) in data first; native plugins for complex logic; scripting optional later** | Most users need no code; layering by complexity | Plugins as the only way |
| D-18 | **License: `MIT OR Apache-2.0` for the whole workspace**, plugin API and SDK included (ADR 0001) | Rust ecosystem norm; permissive enough that third parties can ship closed-source plugins, so the SDK needs no separate license | MPL-2.0 engine, proprietary, separate SDK license |
| D-19 | **No hosted CI; the local `verify-and-build` run is the quality gate** (ADR 0004). Read "CI" in this spec as that run. M0 acceptance 1 is waived | The project has no remote and will not use CI; all gates stay scripted commands | GitHub Actions as specified |

## 3. System overview

```
   Optional clients                         Engine process (cgf-cli)
+--------------------+            +----------------------------------------+
| GUI (React + TS)   |  REST/WS   | cgf-server (optional feature)          |
| Web / Tauri (Win)  |<---------->|   uses EngineHandle (channels)         |
+--------------------+            |----------------------------------------|
| Scripts / CI       |            | cgf-engine (facade, embeddable lib)    |
+--------------------+            |  kernel: clock, scheduler, ECS, bus    |
                                  |  behavior | motion | perception        |
   Template files (TOML)--------->|  templates + registry                  |
   Scenario files (TOML)--------->|  plugin host (C ABI)                   |
                                  |  network layer (adapter trait)         |
                                  +--------------^------------^------------+
                                                 |            |
                                        plugin DLLs     DIS / HLA / custom
```

- **REQ-S-1** `cgf-cli run scenario.toml` MUST run a scenario to completion with no server, no GUI, and no network adapters unless the scenario enables them.
- **REQ-S-2** The engine MUST also be usable as a Rust library (`cgf-engine`) for embedding and integration tests.

## 4. Workspace layout and dependency rules

| Crate | Responsibility | May depend on |
|---|---|---|
| `cgf-core` | Domain types: IDs, time, units, coordinates, canonical events, core traits (`Behavior`, `MotionModel`, `NetworkAdapter`, `TerrainProvider`, `Sensor`). No I/O. | std, small utility crates |
| `cgf-kernel` | ECS world, clock, scheduler, tick pipeline, event bus, RNG service | core |
| `cgf-motion` | Built-in kinematic models (ground, air, sea) | core |
| `cgf-behavior` | Behavior runtime: FSM and behavior tree interpreters, built-in actions | core |
| `cgf-templates` | Parse, validate, resolve inheritance, registry of templates and scenarios | core |
| `cgf-plugin-api` | **C ABI contract only**: `#[repr(C)]` structs, function pointer tables, version constants. Generates `cgf_plugin_api.h` via `cbindgen`. | nothing |
| `cgf-plugin-host` | Load libraries, check manifest/ABI, lifecycle, bridge plugin vtables into core traits | core, plugin-api |
| `cgf-sdk` | Safe Rust wrapper and `export_plugin!` macro for plugin authors | plugin-api (not core) |
| `cgf-net` | Network manager, canonical message model, adapter registry, publish policy and dead reckoning | core |
| `cgf-net-dis` | DIS adapter (built as a plugin using the SDK) | sdk |
| `cgf-net-hla` | HLA adapter (later, same rules) | sdk |
| `cgf-engine` | Facade: wires everything, exposes `Engine`, `EngineHandle`, `EngineConfig` | all engine crates |
| `cgf-api-types` | Wire DTOs for REST and stream; derives schema and TS types | serde |
| `cgf-server` | axum REST + WebSocket, OpenAPI generation | engine facade, api-types |
| `cgf-cli` | Binary, composition root, argument parsing | engine, optionally server |
| `examples/plugins/*` | Reference plugins (behavior, motion, network) | sdk |
| `gui/` | React + TypeScript app, Tauri shell | generated client only |

**Rules (CI-enforced with a dependency-graph test):**
- **REQ-A-1** Dependencies point inward. `cgf-core` depends on no other workspace crate.
- **REQ-A-2** `cgf-kernel` MUST NOT depend on `cgf-server`, `cgf-net-*`, or any protocol crate.
- **REQ-A-3** `cgf-sdk` MUST depend only on `cgf-plugin-api`, so plugin authors never link engine internals.
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
- **REQ-T-6** `cgf-cli validate <path>` validates templates and scenarios without running them.
- **REQ-T-7 (MAY)** Hot reload of templates for new spawns (existing entities unaffected).

### 7.3 Scenario files
A scenario lists: seed, clock mode, tick rate, plugins to load, enabled network adapters with config, terrain provider, initial entity placements (template id, position, heading, optional param overrides, optional name/group), and optional scheduled events (e.g. spawn at t=60 s). Scenarios use the same validation pipeline as templates.

## 8. Behavior

- **REQ-B-1** Core trait (conceptual): `Behavior::tick(ctx: &BehaviorContext, entity: EntityView, out: &mut IntentBuffer)`. A behavior **reads** state and perception and **writes intents**; it never mutates entity state directly. This keeps behaviors deterministic and replaceable.
- **REQ-B-2 Intents** are a closed, versioned set: `MoveTo`, `FollowRoute`, `SetSpeed`, `SetHeading`, `Hold`, `Fire{target, weapon}`, `Spawn`, `Despawn`, `Custom{kind,payload}`.
- **REQ-B-3 Layers:**
  1. **Declarative**: FSM and behavior-tree definitions in TOML using built-in conditions and actions (`move_to`, `is_enemy_in_range`, `wait`, ...). Covers most users with no code.
  2. **Plugin behaviors** via the C ABI for complex logic.
  3. *Later (MAY):* embedded scripting or WASM as another plugin kind.
- **REQ-B-4** Plugin behaviors are invoked in **batches** (a slice of entities per call) to limit FFI overhead.
- **REQ-B-5** Per-instance behavior state is held in an engine-managed opaque blob (or a plugin-owned handle with create/destroy callbacks) so replay and snapshotting can be supported.

## 9. Motion and physics

- **REQ-M-1** `MotionModel` trait: `step(state, intents, dt, terrain) -> state`. Motion models translate intents into physically bounded movement.
- **REQ-M-2 Built-ins (v1):** `ground.kinematic` (speed, turn rate, acceleration limits, terrain following via `TerrainProvider`), `air.kinematic` (speed, altitude, climb/bank limits), `sea.kinematic` (speed, turn radius, constant surface height).
- **REQ-M-3** Full rigid-body/vehicle dynamics (e.g. `rapier`) is a later alternative implementing the same trait. Not v1.
- **REQ-M-4** Motion models are selected per template by ID, so adding one never changes the kernel (OCP).

## 10. Plugin system

The highest-risk area. Keep it small, versioned, and heavily tested.

### 10.1 Contract
- **REQ-P-1** Plugins are dynamic libraries (`.dll` / `.so`) exporting exactly one symbol: `const CgfPluginDescriptor* cgf_plugin_entry(const CgfHostApi* host);`
- **REQ-P-2** All plugin API structs are `#[repr(C)]`, begin with `abi_version` and `struct_size` fields, and are **append-only** within a major ABI version. Hosts and plugins check sizes before reading newer fields.
- **REQ-P-3 Descriptor** contains: plugin id, name, semantic version, required ABI major/minor, and a list of **capabilities** (kind + id + param JSON Schema + vtable pointer).
- **REQ-P-4 Capability kinds (v1):** `behavior`, `motion_model`, `network_adapter`, `sensor`, `component`. New kinds extend the enum and vtable set, never add new loading paths (D-8).
- **REQ-P-5 HostApi** provides: logging, `SimRng` access, current `SimTime`, terrain queries, event emit, memory helpers, capability registration. Plugins interact with the engine only through this.
- **REQ-P-6 Ownership:** memory crossing the boundary is freed by the side that allocated it, via a provided free function. Prefer host-allocated output buffers. Strings are UTF-8 pointer+length. Params and payloads cross as UTF-8 JSON or length-prefixed byte buffers.
- **REQ-P-7 Panics:** the SDK wraps every exported function in `catch_unwind` and converts panics to error codes. On plugin error the host logs, marks the capability faulted, and applies the scenario's `on_plugin_fault` policy (`abort` | `disable_capability`).
- **REQ-P-8 Lifecycle:** `load -> describe -> configure(params) -> init -> tick/call* -> shutdown -> unload`. Unload only after all handles are released.
- **REQ-P-9 Versioning:** Host rejects plugins whose ABI major differs or whose minor is newer than the host supports, with a clear message.
- **REQ-P-10 Discovery:** plugin directories set in engine config and scenario; an optional `plugin.toml` next to each library carries metadata checked *before* loading the library.
- **REQ-P-11 In-process risk:** v1 plugins run in-process and can crash the engine. This is accepted and documented. The vtable design MUST NOT assume a shared address space beyond pointers passed in calls, so an out-of-process or WASM host can be added later.

### 10.2 SDK
- **REQ-SDK-1** `cgf-sdk` offers idiomatic Rust traits (`Behavior`, `MotionModel`, `NetworkAdapter`, `Sensor`) and an `export_plugin!` macro generating the entry point, vtables, panic guards and descriptor.
- **REQ-SDK-2** `cgf_plugin_api.h` generated by `cbindgen` is published so C/C++ authors can write plugins. A C# NativeAOT sample SHOULD exist (M9).
- **REQ-SDK-3** The DIS adapter and example plugins MUST be built with the SDK exactly as third parties would. No private back doors.
- **REQ-SDK-4** Ship `cgf-cli new-plugin <kind>` scaffolding and documented examples under `examples/plugins/`.

## 11. Networking

- **REQ-N-1** Canonical model lives in `cgf-core`; `NetworkAdapter` trait (and C ABI equivalent): `configure`, `start`, `publish(&[OutboundEvent])`, `poll(&mut Vec<InboundEvent>)`, `stop`. Adapters MUST NOT block the tick; I/O runs on adapter-owned threads with bounded queues.
- **REQ-N-2** The network manager (`cgf-net`) owns **publishing policy** independent of protocol: which owned entities to publish, thresholds/heartbeats (position/orientation deltas, max interval), rate limiting. Adapters only translate.
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
- **REQ-X-6 Stream:** WebSocket `/api/v1/stream`. Clients subscribe with filters (bounding box, entity ids, event kinds) and a max rate. Frames are **batched snapshots/deltas**. v1 encodes JSON; encoding is negotiated (`json` | `msgpack`) so the format can change without breaking clients. Stream message types live in `cgf-api-types`, are exported as JSON Schema/TypeScript types, and documented with AsyncAPI.
- **REQ-X-7** The server holds only an `EngineHandle`; it MUST NOT touch ECS data directly. A slow WebSocket client never slows the tick (REQ-K-6).
- **REQ-X-8** Multiple concurrent clients are allowed; no command restrictions or authentication in v1. Server binds to localhost by default; exposing it is an explicit config choice. Document this limitation.
- **REQ-X-9** Errors use a consistent problem-details JSON shape with stable error codes.

## 13. GUI (separate deliverable, after M5)

- React + TypeScript, built with Vite; small store (e.g. Zustand); map library decided at M8 behind a `MapView` component interface.
- API client generated from `api/openapi.json` (`openapi-typescript` + `openapi-fetch`); stream types generated from the shared schema. No hand-written duplicates.
- **Windows app:** Tauri shell loading the same build; may spawn `cgf-cli` as a sidecar or connect to a remote engine URL.
- v1 features: connect to engine, load scenario, run controls, map with live entities, entity inspector, template browser, plugin/adapter status, log view. Template/scenario editing UI comes later, driven by JSON Schemas.
- The GUI is never required for any engine feature (REQ-G-1).
- Project structure, tooling (Bun) and layering rules: see `gui/CLAUDE.md` and `docs/frontend-architecture.md`.

## 14. Non-functional requirements

- **NFR-1 Performance (A-1):** 10,000 owned kinematic entities with declarative behaviors at 30 Hz on a modern 8-core desktop using one simulation thread, median tick under 25 ms. Provide a `bench` scenario and `criterion` benchmarks per phase; regressions over 15% fail the benchmark CI job.
- **NFR-2 Reliability:** No `unwrap`/`expect` on external input. Typed errors (`thiserror`) in libraries, `anyhow` only in binaries. `unsafe` confined to `cgf-plugin-host`, `cgf-sdk`, `cgf-plugin-api`, each block with a `// SAFETY:` comment and tests; `#![forbid(unsafe_code)]` elsewhere.
- **NFR-3 Portability:** Windows and Linux CI builds, including plugin loading tests on both.
- **NFR-4 Security:** Plugins are trusted native code (document clearly). Server is localhost-only by default. `cargo audit` and `cargo deny` (licenses, advisories) in CI.
- **NFR-5 Documentation:** rustdoc on all public items; `docs/` with architecture overview, plugin authoring guide, template reference (generated from schemas), network adapter guide.
- **NFR-6 Licensing:** Decide the project license before M1.

## 15. Testing strategy

- **Unit:** coordinate math, motion models, FSM/BT interpreters, template resolution. Coordinate conversions tested against published reference values.
- **Property tests (`proptest`):** coordinate round-trips, template inheritance, clock monotonicity.
- **Determinism tests:** run each reference scenario twice (and under different clock modes) and compare per-tick state hashes. This test MUST gate every PR.
- **Plugin tests:** a test plugin built in CI and loaded by the host; cases for ABI mismatch, missing symbol, plugin panic, wrong struct size, double load, unload ordering.
- **Network tests:** DIS golden-packet encode/decode; loopback adapter integration; echo-loop prevention.
- **API tests:** contract tests against the generated OpenAPI; WebSocket back-pressure test (slow client does not slow ticks).
- **End-to-end:** CLI runs a scenario headless and asserts on recorded output.
- **CI jobs:** fmt, clippy (`-D warnings`), tests (Windows + Linux), cargo-deny/audit, OpenAPI drift check, benchmarks, dependency-rule check (REQ-A-*).

## 16. Coding principles

- **SRP:** one responsibility per crate and module; systems are small and single-purpose.
- **OCP:** new behaviors, motion models, protocols and sensors are added by registering, never by editing kernel code.
- **LSP:** all implementations honor the documented trait contract; contract tests live next to each trait and run against every implementation (built-in and example plugins).
- **ISP:** small traits (`Behavior`, `MotionModel`, `Sensor`, `NetworkAdapter`, `TerrainProvider`), not one large interface.
- **DIP:** the kernel depends on traits in `cgf-core`; concrete implementations are injected in `cgf-engine`/`cgf-cli`.
- **DRY:** one registry, one plugin loader, one validation pipeline, one canonical event model.
- **KISS:** no speculative generality; add an abstraction when a second implementation exists or the spec requires it. Single-threaded tick until profiling says otherwise.
- **Style:** `rustfmt` defaults, selective `clippy::pedantic`, short functions, documented public APIs, no global mutable state, time and randomness injected.
- **Git:** small PRs per requirement ID, conventional commits, every PR updates the docs and tests it affects.

## 17. Milestones and acceptance criteria

| # | Milestone | Deliverable | Acceptance criteria |
|---|---|---|---|
| M0 | Foundations | Workspace, CI, lint/deny config, license, dependency-rule check, `cgf-core` skeleton (IDs, time, units) | CI green on Windows + Linux; dependency-rule check fails when a rule is violated (tested) |
| M1 | Walking skeleton | Kernel, clock modes, ECS, `SimRng`, coordinates, `ground.kinematic`, hard-coded entity, CLI prints state, state hash | Determinism test passes; coordinate tests pass; entity follows a waypoint list |
| M2 | Templates and scenarios | TOML loader, inheritance, registry, validation errors, `validate` command, JSON Schemas | REQ-T-1..6 tested including cycle and unknown-ID errors; example scenario with 3 templates runs |
| M3 | Declarative behavior | FSM and behavior-tree interpreters, built-in actions/conditions, intents pipeline, perception (visual sensor) | Patrol-and-engage scenario runs deterministically; swapping a behavior needs only a template change |
| M4 | Plugin ABI v0 and SDK | `cgf-plugin-api`, host, SDK, `export_plugin!`, behavior plugin kind, example plugin, generated C header | Example plugin loaded and driving entities; all plugin failure tests pass; a C example compiles against the header |
| M5 | Engine facade and control | `EngineHandle`, snapshots, `cgf-server` REST + WebSocket, committed OpenAPI, stream schema | OpenAPI drift check passes; a script or generated client runs a scenario end to end; slow-client test passes |
| M6 | Networking and DIS | `cgf-net`, loopback adapter, DIS adapter (as plugin), publish thresholds, ghosts and dead reckoning | Two engine instances exchange entities over DIS on localhost; no echo loops; golden-packet tests pass |
| M7 | More plugin kinds and replay | Motion model, sensor and network plugin kinds; air/sea models; recording/replay | A third-party-style example plugin of each kind in `examples/`; replay reproduces state hashes |
| M8 | GUI and Windows app | React app, generated client, Tauri packaging, `MapView` | Web and Windows builds both show live entities from a running engine; GUI uses no engine source |
| M9 | HLA and hardening | HLA adapter with chosen RTI, C# plugin sample, benchmark tuning, docs | HLA interop test against a reference federate; NFR-1 target met |

## 18. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Plugin ABI churn breaks users | Tiny `cgf-plugin-api`, append-only structs, versioned descriptors, example plugins (M4) before adding more kinds |
| Plugin crash takes down the engine | Panic guards, fault policy, documented risk; vtable design allows out-of-process hosting later |
| FFI overhead for behaviors | Batched calls (REQ-B-4); declarative behaviors for the common case |
| Determinism broken by plugins or hash ordering | Determinism test gates PRs; host-provided RNG and time only; ordered structures |
| DIS interoperability subtleties (dead reckoning, coordinates) | Golden packets; test against a second DIS tool during M6 |
| HLA RTI licensing and availability | Isolated optional plugin; Portico as an open option; scheduled last |
| Scope creep into high-fidelity physics | Non-goal for v1; trait boundary keeps it addable |

## 19. Open questions (resolve before or during the named milestone)

1. **M0 (resolved 2026-10-17, D-18):** ~~Project license, and whether the SDK/plugin API gets a more permissive license than the engine.~~ `MIT OR Apache-2.0` everywhere.
2. **M1:** Confirm A-1 (scale) and A-2 (determinism). Changes affect the scheduler and plugin RNG rules.
3. **M3:** Behavior trees vs FSM as the first declarative format, or both at once.
4. **M5:** Is command access control needed (more than one operator)? Currently out of scope.
5. **M6:** Reuse an existing DIS crate or implement PDUs in-house (depends on license and coverage).
6. **M8:** Map library (MapLibre 2D vs Cesium 3D).
7. **M9:** Which RTI(s) to support first, and which FOM (RPR-FOM version).
