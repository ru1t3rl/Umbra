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
| `umbra-engine` | `crates/umbra-engine` | Embeddable synchronous facade, placement by ECEF or lat/lon/alt, the M1 hard-coded demo is replaced by `examples/scenarios/walking_skeleton/` (M2); built-in registry, `builtin.terrain.flat`, scenario load/validate/instantiate, `ControlFlow` run callbacks (M2); M3: `builtin.visual` sensor, the two built-in graphs (`behaviors/*.behavior.toml`), `standard_registry` (`builtin_registry` keeps its M2 contents, ASM-084), `scenario_registry` (a registry clone with the scenario's file graphs), `Engine::spawn_with` | `umbra-core`, `umbra-kernel`, `umbra-motion`, `umbra-templates`, `umbra-behavior` |
| `umbra-cli` | `crates/umbra-cli` | Binary; `run`, `validate`, `schema` commands (M2; `demo` removed); M3: all commands use `standard_registry()`, `schema behavior`, the command runs on a thread with an 8 MiB stack (ASM-098, ASM-109) | `umbra-engine` |

`umbra-templates` and `umbra-behavior` do not know each other: the first turns graph files into `GraphDef`s
(a `umbra-core` type), the second runs them, and `umbra-engine` is the one place that composes both (ADR
0011). Graph files, built-in graphs and (from M4) plugin behaviors all reach the kernel as
`Behavior` factories in the one registry (D-8); the kernel sees only the `umbra-core` traits.

Planned crates (section 4) are added in their own milestones; no placeholders (ASM-010).

## Tick pipeline (`umbra-kernel`, REQ-K-2)
Every tick runs the six phases of `Phase::ALL` in this order. Perception, Behavior and Motion act on
**Owned** entities only (REQ-K-5); Remote entities are never ticked, but they are perceived.

| Phase | What it does |
|---|---|
| Ingest | Network and command input (empty until the adapters and the server exist) |
| Perception | For each Owned entity with sensors, in `EntityId` order: run its sensors against every other entity (Owned and Remote) as it was before the tick; merge contacts per target (first sensor wins), sort by id (ADR 0015) |
| Behavior | For each Owned entity, in `EntityId` order: clear its intents; tick its behavior instance with its own state, route view and contacts (it can only append intents), then expand the first `FollowRoute` into the route stand-in's intents; an entity without a behavior runs the M1 route stand-in unchanged (ASM-082) |
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
