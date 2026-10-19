# Architecture

Source of truth: `docs/SPEC.md` section 4. This file is started in M0 WI-1 and grows with each milestone.

## Crates (current)
| Crate | Path | Role | Workspace deps |
|---|---|---|---|
| `umbra-core` | `crates/umbra-core` | IDs, time, units; `math` (single site for non-exact float functions, ADR 0003), `geo` (Vec3, Quaternion, Euler), `coords` (WGS-84, ECEF, geodetic, ENU/NED, LocalFrame, Position), `hash` (canonical state hasher, ADR 0007), `rng` (SimRng, ADR 0006), and the motion domain `intent`, `ownership`, `terrain`, `motion` (M1), `params` (`ParamValue` = `serde_json::Value`, `ParamSchema`) and `registry` (`Registry`, `Factory<T>`, `ComponentKind`; M2, ADR 0008); M3: `behavior` (`Behavior`/`BehaviorInstance`, `Condition`, `Action`, `Blackboard`, `IntentBuffer`), `perception` (`Sensor`, `Contact`, `Force`), `graph` (`GraphDef`, the validated, format-free graph that `umbra-templates` produces and `umbra-behavior` compiles), `catalog` (`Catalog`, `ComponentInfo`, `Provenance`), registry kinds `Condition`/`Action`, `TerrainProvider::line_of_sight` (ADR 0011, 0013-0016) | none (REQ-A-1); external: `thiserror`, `serde_json`, `semver` (ADR 0017) |
| `umbra-kernel` | `crates/umbra-kernel` | Clock (four modes, wall-clock site), hecs-backed world, fixed six-phase tick pipeline, state hash, snapshot (M1); M3: Perception and Behavior phase systems, `AgentInit` and `Kernel::spawn_with` (force, behavior, sensors), snapshot fields `force`, `behavior`, `status`, `intents`, `contacts` | `umbra-core` (REQ-A-2) |
| `umbra-motion` | `crates/umbra-motion` | Built-in motion models, `builtin.ground.kinematic` (M1; `SetHeading` in M3) | `umbra-core` |
| `umbra-behavior` | `crates/umbra-behavior` | Behavior graph interpreter (M3): compiles a `umbra_core::GraphDef` into a shared program and runs per-entity instances (composites, decorators, FSM, subgraphs, halt, input and output copies); the eight built-in conditions and actions (`register_builtins`); `register_graphs` adds graphs to a `Registry` as behaviors. Knows no file format: it never sees TOML (ADR 0011) | `umbra-core` only (section 4); external: `thiserror`, `serde_json`. Tests may not use `umbra-templates` or `umbra-motion` (dev edges are checked, ASM-005) |
| `umbra-templates` | `crates/umbra-templates` | Template and scenario reading, inheritance, schema validation, located diagnostics (M2); M3: `*.behavior.toml` graph reader and graph-set validation (`graph_read`, `graph_resolve`), template `[behavior]`/`[[sensors]]` resolution against graphs of the same load (`overlay`), placement `force`, `behavior.v1.schema.json` | `umbra-core` only (section 4) |
| `umbra-plugin-api` | `crates/umbra-plugin-api` | Plugin C ABI v0 (M4, ADR 0019-0021): `#[repr(C)]` types, function-pointer tables and version constants only; `no_std`, no logic; the C header `include/umbra_plugin_api.h` is generated from it (REQ-SDK-2); 64-bit targets only (ASM-120) | none, external ones included (section 4, ASM-009) |
| `umbra-plugin-host` | `crates/umbra-plugin-host` | Plugin discovery, loading, descriptor validation, lifecycle and the bridge from plugin capabilities to the core traits in the one registry (M4, D-8, ADR 0021, ADR 0022); WI-8: `discover` (ADR 0022 layout and order), `plugin.toml` checked before opening, libraries opened with explicit flags (`libloading`, ADR 0023 item 1), the process-wide image table with lifecycle states, descriptor copy and validation, `init`/`shutdown`, the thread-local call-record stack, host API `log`, all-or-nothing `register` (see "Plugin host" below); `unsafe` allowed (FFI crate, NFR-2) | `umbra-core`, `umbra-plugin-api`; external: `libloading` (ISC), `toml`, `serde_json`, `thiserror`, `tracing` |
| `umbra-sdk` | `crates/umbra-sdk` | Safe Rust traits, conversions, panic guards and `export_plugin!` for plugin authors (M4, REQ-SDK-1, D-7); re-exports the ABI as `umbra_sdk::abi`; its only global state is the copied host API of the current plugin lifecycle, the static descriptor and an empty contacts scratch buffer (ASM-127); `unsafe` allowed (FFI crate, NFR-2) | `umbra-plugin-api` only (REQ-A-3) |
| `umbra-engine` | `crates/umbra-engine` | Embeddable synchronous facade, placement by ECEF or lat/lon/alt, the M1 hard-coded demo is replaced by `examples/scenarios/walking_skeleton/` (M2); built-in registry, `builtin.terrain.flat`, scenario load/validate/instantiate, `ControlFlow` run callbacks (M2); M3: `builtin.visual` sensor, the two built-in graphs (`behaviors/*.behavior.toml`), `standard_registry` (`builtin_registry` keeps its M2 contents, ASM-084), `scenario_registry` (a registry clone with the scenario's file graphs), `Engine::spawn_with` | `umbra-core`, `umbra-kernel`, `umbra-motion`, `umbra-templates`, `umbra-behavior` |
| `umbra-cli` | `crates/umbra-cli` | Binary; `run`, `validate`, `schema` commands (M2; `demo` removed); M3: all commands use `standard_registry()`, `schema behavior`, the command runs on a thread with an 8 MiB stack (ASM-098, ASM-109) | `umbra-engine` |

`umbra-templates` and `umbra-behavior` do not know each other: the first turns graph files into `GraphDef`s
(a `umbra-core` type), the second runs them, and `umbra-engine` is the one place that composes both (ADR
0011). Graph files, built-in graphs and (from M4) plugin behaviors all reach the kernel as
`Behavior` factories in the one registry (D-8); the kernel sees only the `umbra-core` traits.

Planned crates (section 4) are added in their own milestones; no placeholders (ASM-010).

## Plugin host (`umbra-plugin-host`, M4)
Loading follows ADR 0021's lifecycle: `plugin.toml` (ABI checked before the library is opened), image
table lookup by canonical library path, open (`RTLD_NOW | RTLD_LOCAL`; Windows
`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, absolute path), lookup by the
address of `umbra_plugin_entry` **before** calling it, the entry called exactly twice (same pointer
required), the descriptor copied and validated completely (sizes, majors, bounds, caps, UTF-8, grammar,
semver, kinds, metadata against the schema), per-host checks (manifest match, plugin id unique), `init`.

- **Image table** (`image.rs`): the one piece of persistent process-global mutable state in the engine
  (ASM-122). One `LoadedImage` per image lifecycle, shared by `Arc` across hosts, registered factories
  and handles; slot states `Loading`, `Live`, `Closing`, and a load that meets `Loading` or `Closing`
  waits on the table's condvar, then reuses a live image (no entry call) or starts a new lifecycle. The
  last release runs `shutdown` (only after a successful `init`) and closes the library unless
  `keep_loaded` (sticky per image) asked to keep it mapped.
- **Call records** (`tokens.rs`): a thread-local stack of in-progress plugin calls, pushed for every call
  into a plugin (`init`, `shutdown`, the bridge's calls) and popped after it, so it is empty whenever no
  plugin call runs on that thread. Host callbacks resolve tokens against the calling thread's top record
  only: from another thread, before `init`, after `shutdown` or with an old lifecycle's `host_ctx` they
  return `UMBRA_ERR_STATE`. Re-entry into an image from the same thread is detected through the stack.
- **Lock order** (ADR 0019, RR-M2): (1) image table, never held while plugin code runs or an
  `Arc<LoadedImage>` may drop; (2) the per-image call lock, blocking (contention waits, never faults; no
  `try_lock`); (3) per-host fault state, a leaf; (4) `PluginLog`, a leaf.
- Errors name the origin (the library path as discovered) and the field (`descriptor.<f>`,
  `capabilities[<i>].<f>`, `capabilities[<i>].vtable.<f>`, `plugin.toml abi`).
- **Behavior bridge** (`bridge/`, WI-9): each registered capability is one `PluginCapability` per host,
  which is also the capability's single `BehaviorBatch`; the kernel's grouped phase gives it all its
  instances in one `tick_batch` call per tick (views in the plugin's minor, statuses pre-filled `UNSET`,
  one RNG token per item drawing the entity's own stream, intents buffered). Statuses and intents are
  committed only after a clean call; otherwise the whole batch is discarded (every item Failure, RNG
  draws kept). Scratch buffers live in the image's call state and stop growing after warm-up.
- **Fault state** (`fault.rs`): per host, so per engine (ASM-118): faulted capabilities, the faults in
  order and the pending abort under `abort`; a faulted capability is skipped by that host only.
- **Build scope** (RR-m2, ASM-138): `PluginHost::build_scope()` on the calling thread lets a run-mode
  host's factories build; the engine the host is attached to (`PluginHost::try_attach`) opens it around
  building and spawning.

## Tick pipeline (`umbra-kernel`, REQ-K-2)
Every tick runs the six phases of `Phase::ALL` in this order. Perception, Behavior and Motion act on
**Owned** entities only (REQ-K-5); Remote entities are never ticked, but they are perceived.

| Phase | What it does |
|---|---|
| Ingest | Network and command input (empty until the adapters and the server exist) |
| Perception | For each Owned entity with sensors, in `EntityId` order: run its sensors against every other entity (Owned and Remote) as it was before the tick; merge contacts per target (first sensor wins), sort by id (ADR 0015) |
| Behavior | For each Owned entity, in `EntityId` order: clear its intents; tick its behavior instance with its own state, route view and contacts (it can only append intents), then expand the first `FollowRoute` into the route stand-in's intents; an entity without a behavior runs the M1 route stand-in unchanged (ASM-082); M4: an entity whose behavior returns a batch from `Behavior::batch()` is not ticked in that walk but grouped (by `Arc::ptr_eq`, groups ordered by their first entity id); each group is then ticked with one `tick_batch` call over its members in id order, and the batched entities get the `FollowRoute` expansion afterwards in id order (ADR 0020, ASM-126) |
| Motion | Motion models turn intents into new kinematic state (ASM-030, ASM-094) |
| Events | Event dispatch, empty so far: `Fire`, `Spawn`, `Despawn` and `Custom` intents are recorded in the snapshot, not applied, until M5 (ADR 0016) |
| Publish | Output to observers, empty so far (the server and adapters come later) |

Each entity has its own contact vector, intent buffer and hashed RNG stream and sees only pre-tick
state, so the outcome does not depend on the order in which entities are visited (ADR 0013). The state
hash stays v2: behavior state, contacts and intents are not hashed; gate 4 compares per-tick intents and
contacts as well (ASM-081, ASM-087). Behavior semantics: `docs/behavior.md`.

## Dependency rules
- REQ-A-1: `umbra-core` depends on no workspace crate.
- REQ-A-2: the kernel never depends on server, network adapters or API types.
- REQ-A-3: `umbra-sdk` depends only on `umbra-plugin-api`; `umbra-plugin-api` depends on nothing.
- REQ-SDK-3 (M4, ADR 0023 item 4): plugin crates (`example-wander`, `umbra-test-plugin`) depend on `umbra-sdk` only and keep `#![forbid(unsafe_code)]` (ASM-116); their rule-table entries exist before the crates do (ASM-007).
- REQ-A-4: GUI rule, enforced in `gui/` (M8, ASM-021).
- Enforcement: `cargo xtask check-deps` (`xtask/src/deps/`), the only gate (no CI, ADR 0004). Pass 1 checks every declared direct edge (normal, build, dev, target-specific, renamed) of every workspace member against the rule table in `xtask/src/deps/rules.rs` (ASM-005..007); membership is by path, and non-member path dependencies are rejected (ASM-006). Pass 2 checks the resolved graph by package id (all features, all platforms, `--offline --locked`), so a forbidden member cannot be reached through `[patch]`, git, registry or same-name packages, and no non-member path package (whose code would escape the unsafe and lints checks) can enter the build (ASM-005, ASM-006). It also checks that every target root outside `umbra-plugin-api`, `umbra-plugin-host` and `umbra-sdk` has `#![forbid(unsafe_code)]` (parsed with `syn`) and that every member inherits the workspace lints (parsed with `toml`) (ASM-024). Negative tests run it against `xtask/tests/fixtures/` (`violating`, `launder_*`, `unsafe_*`, `lints_only`).

## Code-level guards (workspace-wide)
- `unsafe_code = "deny"` in `[workspace.lints]`; non-FFI crates also `#![forbid(unsafe_code)]` (NFR-2, ASM-012).
- `clippy.toml` bans `SystemTime::now`/`elapsed`, `Instant::now`/`elapsed`, `thread::sleep`, `HashMap`, `HashSet` and `RandomState` (REQ-K-4, ASM-013). A `HashMap` from a third-party API iterated without naming the type remains a review item.
- Math (ADR 0003, ASM-034): every non-exact float function (trig, inverse trig, hyperbolic, exp/log, `powi`, `powf`, `cbrt`, `hypot`) goes through `umbra_core::math`, the one place that may call the `std` methods and so the one swap point for `libm`. `clippy.toml` bans the direct `f64`/`f32` methods (and `mul_add`) workspace-wide with the reason "ADR 0003: use umbra_core::math"; `sqrt`, `abs`, `floor`, `to_radians` and the basic operators stay allowed because IEEE 754 makes them exact.
- Randomness (ASM-042): `clippy.toml` also bans the `rand` entry points `random`, `thread_rng`, `rng`, `OsRng`, `ThreadRng`, `SeedableRng::from_entropy` and `from_os_rng`, the `getrandom` functions and the `fastrand` entry points (paths that do not resolve use `allow-invalid`), `RandomState` and `DefaultHasher` (ADR 0007: use `StateHasher`), and the types `hashbrown::HashMap`/`HashSet`. Simulation randomness comes from `SimRng` streams (ADR 0006).
- Wall time (ASM-035): `Instant::now` is allowed in exactly one place, a private module of `crates/umbra-kernel/src/clock/wall.rs` (`StdWallClock`), through one `#[expect(clippy::disallowed_methods, reason = "REQ-K-4: ...")]`; everything else reads time through `WallClock` or `Clock`.
- `allow_attributes` and `allow_attributes_without_reason` are `warn`: use `#[expect(..., reason = "...")]` (ASM-012).
- Binaries and tests may print (`print_stdout`/`print_stderr` are `warn`; tests are exempt, ASM-023).
