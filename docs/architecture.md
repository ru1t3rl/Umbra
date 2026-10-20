# Architecture

Source of truth: `docs/SPEC.md` section 4. This file is started in M0 WI-1 and grows with each milestone.

## Crates (current)
| Crate | Path | Role | Workspace deps |
|---|---|---|---|
| `umbra-core` | `crates/umbra-core` | IDs, time, units; `math` (single site for non-exact float functions, ADR 0003), `geo` (Vec3, Quaternion, Euler), `coords` (WGS-84, ECEF, geodetic, ENU/NED, LocalFrame, Position), `hash` (canonical state hasher, ADR 0007), `rng` (SimRng, ADR 0006), and the motion domain `intent`, `ownership`, `terrain`, `motion` (M1), `params` (`ParamValue` = `serde_json::Value`, `ParamSchema`) and `registry` (`Registry`, `Factory<T>`, `ComponentKind`; M2, ADR 0008); M3: `behavior` (`Behavior`/`BehaviorInstance`, `Condition`, `Action`, `Blackboard`, `IntentBuffer`), `perception` (`Sensor`, `Contact`, `Force`), `graph` (`GraphDef`, the validated, format-free graph that `umbra-templates` produces and `umbra-behavior` compiles), `catalog` (`Catalog`, `ComponentInfo`, `Provenance`), registry kinds `Condition`/`Action`, `TerrainProvider::line_of_sight` (ADR 0011, 0013-0016) | none (REQ-A-1); external: `thiserror`, `serde_json`, `semver` (ADR 0017) |
| `umbra-kernel` | `crates/umbra-kernel` | Clock (four modes, wall-clock site), hecs-backed world, fixed six-phase tick pipeline, state hash, snapshot (M1); M3: Perception and Behavior phase systems, `AgentInit` and `Kernel::spawn_with` (force, behavior, sensors), snapshot fields `force`, `behavior`, `status`, `intents`, `contacts` | `umbra-core` (REQ-A-2) |
| `umbra-net` | `crates/umbra-net` | Protocol-neutral network layer (M6, ADR 0030): manager, publish policy, ghosts, identity map, adapter interface; knows no protocol vocabulary (`tests/protocol_neutral.rs`). WI-1 scaffold only | `umbra-core` only (section 4); no protocol crate (`check-deps`, ASM-256) |
| `umbra-motion` | `crates/umbra-motion` | Built-in motion models, `builtin.ground.kinematic` (M1; `SetHeading` in M3) | `umbra-core` |
| `umbra-behavior` | `crates/umbra-behavior` | Behavior graph interpreter (M3): compiles a `umbra_core::GraphDef` into a shared program and runs per-entity instances (composites, decorators, FSM, subgraphs, halt, input and output copies); the eight built-in conditions and actions (`register_builtins`); `register_graphs` adds graphs to a `Registry` as behaviors. Knows no file format: it never sees TOML (ADR 0011) | `umbra-core` only (section 4); external: `thiserror`, `serde_json`. Tests may not use `umbra-templates` or `umbra-motion` (dev edges are checked, ASM-005) |
| `umbra-templates` | `crates/umbra-templates` | Template and scenario reading, inheritance, schema validation, located diagnostics (M2); M3: `*.behavior.toml` graph reader and graph-set validation (`graph_read`, `graph_resolve`), template `[behavior]`/`[[sensors]]` resolution against graphs of the same load (`overlay`), placement `force`, `behavior.v1.schema.json`; M5: the `umbra.toml` configuration reader (`config`, ADR 0029) with `config.v1.schema.json`, `LoaderOptions::default_tick_rate` | `umbra-core` only (section 4) |
| `umbra-plugin-api` | `crates/umbra-plugin-api` | Plugin C ABI v0 (M4, ADR 0019-0021): `#[repr(C)]` types, function-pointer tables and version constants only; `no_std`, no logic; the C header `include/umbra_plugin_api.h` is generated from it (REQ-SDK-2); 64-bit targets only (ASM-120); layout final since WI-14, frozen at the M4 close-out (`docs/plugin-abi-changelog.md`) | none, external ones included (section 4, ASM-009) |
| `umbra-plugin-host` | `crates/umbra-plugin-host` | Plugin discovery, loading, descriptor validation, lifecycle and the bridge from plugin capabilities to the core traits in the one registry (M4, D-8, ADR 0020-0022): `discover` (ADR 0022 layout and order), `plugin.toml` checked before opening, libraries opened with explicit flags (`libloading`, ADR 0023 item 1), the process-wide image table with lifecycle states, descriptor copy and validation, `init`/`shutdown`, the thread-local call-record stack, host API `log` and RNG, all-or-nothing `register`, the behavior bridge, per-host fault state and build scope (see "Plugin host" below); `unsafe` allowed (FFI crate, NFR-2) | `umbra-core`, `umbra-plugin-api`; external: `libloading` (ISC), `toml`, `serde_json`, `thiserror`, `tracing` |
| `umbra-sdk` | `crates/umbra-sdk` | Safe Rust traits, conversions, panic guards and `export_plugin!` for plugin authors (M4, REQ-SDK-1, D-7); re-exports the ABI as `umbra_sdk::abi`; its only global state is the copied host API of the current plugin lifecycle, the static descriptor and an empty contacts scratch buffer (ASM-127); `unsafe` allowed (FFI crate, NFR-2) | `umbra-plugin-api` only (REQ-A-3) |
| `umbra-test-plugin` | `crates/umbra-test-plugin` | Test fixture plugin (M4 WI-10, ADR 0023 item 4): a `cdylib` built with `umbra-sdk` and `#![forbid(unsafe_code)]`, plugin `test.plugin.fixture` 0.1.0 with one capability per fault an SDK plugin can produce (ASM-132) and the capabilities the engine tests use (`ok`, `draw_speed`, `needs_target`, `cross_param`); features `entry_panics`, `init_fails`, `init_panics`, `shutdown_panics`, `bad_schema`. Built by the tests' build helper into `CARGO_TARGET_TMPDIR`, never linked | `umbra-sdk` only (REQ-SDK-3) |
| `example-wander` | `examples/plugins/wander` | Example plugin (M4 WI-13, REQ-SDK-4, M4 acceptance 1): a `cdylib` built with `umbra-sdk` and `#![forbid(unsafe_code)]`, plugin `example.plugin.wander` 0.1.0 with behavior `example.behavior.wander` (params `speed_mps`, bindable, and `turn_every_s`; a new heading from the entity's RNG stream on the first tick and every `ceil(turn_every_s * hz)` ticks; output `turns`). An explicit workspace member (ADR 0023 item 4); packaged by `cargo xtask build-plugins` to `<target dir>/plugins/wander/` and run by `examples/scenarios/plugin_wander` with `--plugin-path target/plugins` (ASM-113, ASM-147); never linked | `umbra-sdk` only (REQ-SDK-3) |
| `umbra-engine` | `crates/umbra-engine` | Embeddable synchronous facade, placement by ECEF or lat/lon/alt, the M1 hard-coded demo is replaced by `examples/scenarios/walking_skeleton/` (M2); built-in registry, `builtin.terrain.flat`, scenario load/validate/instantiate, `ControlFlow` run callbacks (M2); M3: `builtin.visual` sensor, the two built-in graphs (`behaviors/*.behavior.toml`), `standard_registry` (`builtin_registry` keeps its M2 contents, ASM-084), `scenario_registry` (a registry clone with the scenario's file graphs), `Engine::spawn_with` | `umbra-core`, `umbra-kernel`, `umbra-motion`, `umbra-templates`, `umbra-behavior` |
| `umbra-api-types` | `crates/umbra-api-types` | Wire types of the control API and stream (M5, REQ-X-4, REQ-X-6, D-12, ADR 0025): `serde` + `utoipa::ToSchema` DTOs only (REST bodies, `Problem` and `ProblemCode`, stream messages) and `API_VERSION` (`1.0.0`); the domain types never appear on the wire, the server maps at the edge | none (ASM-008) |
| `umbra-server` | `crates/umbra-server` | REST API under `/api/v1` and WebSocket stream over the engine handle (M5, REQ-X-4..9, D-10, D-11, ADR 0025); owns the tokio runtime; optional in the CLI (feature `server`); it talks only to `EngineHandle` and never touches ECS data; see "Engine handle, bus and server" below | `umbra-engine`, `umbra-api-types` (never `umbra-kernel` or `umbra-core`, REQ-X-7) |
| `umbra-cli` | `crates/umbra-cli` | Binary; `run`, `validate`, `schema` commands (M2; `demo` removed); M4: `--plugin-path` on `run` and `validate`, `list-plugins [SCENARIO] [--plugin-path DIR].. [--json]`; M3: all commands use `standard_registry()`, `schema behavior`, the command runs on a thread with an 8 MiB stack (ASM-098, ASM-109) | `umbra-engine`, `umbra-server` (optional, feature `server`, default on; `umbra serve`, M5 WI-17; `--no-default-features` builds the engine-only CLI, gate 16) |

`umbra-templates` and `umbra-behavior` do not know each other: the first turns graph files into `GraphDef`s
(a `umbra-core` type), the second runs them, and `umbra-engine` is the one place that composes both (ADR
0011). Graph files, built-in graphs and (from M4) plugin behaviors all reach the kernel as
`Behavior` factories in the one registry (D-8); the kernel sees only the `umbra-core` traits.

Planned crates (section 4) are added in their own milestones; no placeholders (ASM-010).

Not a crate: the C example plugin `examples/plugins/c-circle` (M4 WI-14, M4 acceptance 3, ADR 0023 item 3)
is one C11 file written against the generated header only (no Rust, no `umbra-sdk`); `cargo xtask
build-plugins` compiles it with the `cc` crate (an `xtask` dependency since WI-14) into
`<target dir>/plugins/c-circle/`, and `umbra-plugin-host/tests/c_example.rs` compiles, loads and ticks it.
Its scenario is `examples/scenarios/plugin_c_circle` (ASM-148).

## Plugin host (`umbra-plugin-host`, M4)
The plugin author's view (ABI rules, lifecycle, faults, determinism, packaging) is `docs/plugin-guide.md`;
this section is the host's internal structure.

Loading follows ADR 0021's lifecycle: `plugin.toml` (ABI checked before the library is opened), image
table lookup by canonical library path, open (`RTLD_NOW | RTLD_LOCAL`; Windows
`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, absolute path), lookup by the
address of `umbra_plugin_entry` **before** calling it, the entry called exactly twice (same pointer
required), the descriptor copied and validated completely (sizes, majors, bounds, caps, UTF-8, grammar,
semver, kinds, metadata against the schema), per-host checks (manifest match, plugin id unique), `init`.

- **Image table** (`image.rs`): the one piece of persistent process-global mutable state in the engine
  (ASM-122), apart from the host-id counter that names hosts for the build scope (ASM-138). One `LoadedImage` per image lifecycle, shared by `Arc` across hosts, registered factories
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

## Engine and CLI plugin wiring (M4 WI-12)
- **Loading** (`umbra-engine/src/plugins.rs`): `load_scenario_with_plugins` reads the scenario's
  `plugin_paths` leniently (`scenario_plugin_paths`), puts the CLI paths first, drops duplicates by
  canonical path (`resolve_plugin_paths`), creates a fresh `PluginHost`, loads and registers the packages
  (ADR 0022 order), loads the scenario against the extended registry inside the host's build scope (the
  loader dry-runs `configure`), and sets the scenario's `on_plugin_fault` policy. `validate_path_with_plugins`
  does the same with a `HostMode::Validation` host and no scope. The same plugin ID from two directories is a
  `PluginError::Duplicate` load error (ASM-144).
- **Running**: `instantiate_with_plugins` creates the engine, calls `Engine::attach_plugins` (`try_attach`;
  `false` is `EngineError::PluginHostInUse`) and builds and spawns inside `build_scope()`. Plain
  `instantiate` builds outside any scope, so a run-mode plugin factory fails with `BehaviorBuild`.
  `Engine::run`/`step` call `take_abort()` after every tick; a fault under `abort` ends the run with
  `EngineError::PluginFault`, after `on_tick` has seen the faulting tick (ASM-145 a).
- **CLI**: `run` and `validate` always use the `*_with_plugins` functions (an empty host without plugins);
  `run` prints one `info:` line per loaded plugin to stderr when `--log-level` is `info` or more verbose, and
  nothing on stdout. `list-plugins` prints the plugins and capabilities as text, or with `--json`:
  `{"plugins":[{"id","name","version","abi":"<major>.<minor>","path","capabilities":[{"kind","id","version"}]}]}`
  in load order. Exit codes: 0 success, 1 invalid input, load error or plugin fault, 2 usage error.

## Engine handle, bus and server (M5)
```
 HTTP / WebSocket clients
        |  (tokio runtime, 2 workers, owned by umbra-server)
   umbra-server  --- DTO mapping, problem details, Host/Origin layers, stream connections
        |  EngineHandle (cloneable; bounded std channel, callback replies)
   engine thread "umbra-engine"  --- owns Engine, PluginHost, Session; applies commands between ticks
        |  Frame (immutable Arc) per tick / per state-changing command
   Bus: one ring per subscription (capacity 4, oldest dropped, events carried over)
```
- **Handle** (`umbra-engine::handle`, ADR 0024). One named thread (stack 8 MiB, at least 4 MiB) owns every
  `Engine`, plugin host and registry of a session, so `Engine` need not be `Send`. `EngineHandle::submit`
  uses `try_send`; a full queue is `Busy`, a finished thread `Closed`. The thread drains queued commands in
  arrival order between ticks (after Publish of tick N, before Ingest of tick N+1), then polls the clock
  once through `Engine::pump`. A `Session` is one scenario load; a new load drops the old session first.
  Async code never ticks, and wall time stays inside the `Clock` (D-5).
- **Bus** (`umbra-engine::bus`). After each tick and each state-changing command the thread builds one
  `Arc<Frame>`, only if some subscription is due (stride follows the tick rate and the subscriber's
  rate), and pushes it to every subscription. A push takes a short mutex and never waits; on overflow
  the oldest frame is dropped, its events are carried into the next drained frame (bounded), and a
  counter increments. A late subscriber is primed with one frame of the current state. Reply after
  publish: a reply implies the frame is already queued.
- **Server** (`umbra-server`, ADR 0025..0027). `routes` builds the axum router; `handlers/` map requests
  to handle commands and engine types to `umbra-api-types` DTOs (`map`); `extract` wraps axum's
  rejections so every error is a problem document (`problem`); `layers/` holds the `Host` allow-list and
  the single mutating-routes layer (the seam for a later auth layer); `stream/` is the WebSocket: a reader
  half and a writer half over a generic sink and stream, filtering and JSON encoding on blocking tasks,
  send timeout and counters. `run` binds, installs the signal handlers, serves, and shuts down in order
  (streams 1001, engine thread joined within the grace period). `contract::documents()` produces the three
  committed files in `api/`.
- **Configuration.** The reader of `umbra.toml` lives in `umbra-templates` (`config`), not in the CLI or
  the server: it has the located-diagnostics pipeline and returns plain values. `umbra-cli` maps them to
  `HandleConfig` and `ServerSettings` (flag > file > default).
- **Not here.** The server never sees `umbra-kernel` or `umbra-core` (REQ-X-7). Network adapters (M6) and
  recording (M7) join later.
Reference for clients: `docs/api.md`.

## Tick pipeline (`umbra-kernel`, REQ-K-2)
Every tick runs the six phases of `Phase::ALL` in this order. Perception, Behavior and Motion act on
**Owned** entities only (REQ-K-5); Remote entities are never ticked, but they are perceived.

| Phase | What it does |
|---|---|
| Ingest | Network input (empty until the adapters exist, M6); API commands apply between ticks in the engine handle, not here (ADR 0024) |
| Perception | For each Owned entity with sensors, in `EntityId` order: run its sensors against every other entity (Owned and Remote) as it was before the tick; merge contacts per target (first sensor wins), sort by id (ADR 0015) |
| Behavior | For each Owned entity, in `EntityId` order: clear its intents; tick its behavior instance with its own state, route view and contacts (it can only append intents), then expand the first `FollowRoute` into the route stand-in's intents; an entity without a behavior runs the M1 route stand-in unchanged (ASM-082); M4: an entity whose behavior returns a batch from `Behavior::batch()` is not ticked in that walk but grouped (by `Arc::ptr_eq`, groups ordered by their first entity id); each group is then ticked with one `tick_batch` call over its members in id order, and the batched entities get the `FollowRoute` expansion afterwards in id order (ADR 0020, ASM-126) |
| Motion | Motion models turn intents into new kinematic state (ASM-030, ASM-094) |
| Events | Owned entities' `Fire` and `Custom` intents become canonical `SimEvent`s in `TickReport::events` (ADR 0028); no state changes. `Spawn` and `Despawn` intents are recorded in the snapshot, not applied, until M7 (ADR 0016, ADR 0028) |
| Publish | Empty; observers are fed by the engine bus after the tick (ADR 0024); adapters come later |

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
