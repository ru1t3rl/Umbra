# Amendment A-002: Umbra, and earlier visual feedback (M3b, M5b)

Status: Accepted (decided by the human project owner on 2026-10-18, as drafted). Spec version: v0.2 -> v0.3. Decisions: D-35, D-36 (next free numbers after D-34). The spec and doc edits listed below are applied in WI-R4, after the rename commit (ADR 0018).

Names in this document are the post-rename names (`umbra-*`, `umbra`, `@umbra/`). Like ADR 0018, it is kept verbatim by the rename.

## Decision
1. **Rename (D-35).** The project is **Umbra**, a Computer Generated Forces (CGF) engine. The rename is full: crates `umbra-*`, binary `umbra`, GUI scope `@umbra/`, C ABI prefix `umbra_` / `Umbra` / `UMBRA_`, environment prefix `UMBRA_`, schema IDs `urn:umbra:schema:...` when added. "CGF" stays the domain term. Names, rename map, migration and verification: `docs/adr/0018-rename-the-project-to-umbra.md`. It is done as one work item before M4.
2. **Visual timeline (D-36).**

| When | What you can see | Milestone |
|---|---|---|
| Now (M3) | Printed or logged state (entity lines, per-tick and final state hashes). The behavior trace in the human's table is not in the CLI yet; it arrives with M3b | M3 (done) |
| After M3 | Optional: a deterministic per-tick behavior trace (`--trace`) and GeoJSON/KML export of a run for playback in an external viewer | **M3b (new, optional, non-blocking)** |
| After M5 | Live map with moving entities, the first real GUI slice | **M5b (new)** |
| M8 | Full GUI: entity inspector, template browser, plugin status, Windows app; MapView now comes from M5b | M8 (scope changed) |
| M8b | Visual behavior editor | M8b (unchanged) |

## Why
- **Rename now**: M4 fixes the C ABI symbol, type and header names, and REQ-P-2 makes them expensive to change later. Before M4 there are no plugins, no server, no GUI code and no external users.
- **M3b**: today the CLI prints only entity state. Behavior debugging (why did blue-1 fire at tick 1130?) needs intents, contacts and the nodes that ran. A track export lets the human watch a run in tools that already exist (Google Earth, QGIS, kepler.gl) without waiting for M8. Both are offline text writers over data the engine already has, so they are cheap and cannot affect the simulation.
- **M5b**: a live map is the most useful early GUI. Building it as the first real slice of `gui/` (not a throwaway viewer) means M8 extends it instead of replacing it (DRY). It also proves the M5 contract, the generated client and the GUI layering early.
- Neither slice touches the engine's mandatory milestone order or acceptance criteria (root `CLAUDE.md`: GUI work "must never block or change engine milestones").

## M3b: run trace and track export (optional, non-blocking)
Picked up only when the human asks. Additive only; no milestone depends on it.

**Deliverables**
1. `umbra run <scenario> --trace`: per tick, per entity in `EntityId` order: contacts (in their deterministic order), the behavior nodes ticked this tick in evaluation order (ADR 0013) with their status (`success`, `failure`, `running`, plus `halted` for halted Running nodes), and the intents emitted (in emission order). Nodes inside subgraphs are named by their call path (for example `main/engage/attack>fire`). With `--json`, one JSON object per tick line. Without `--trace`, `run` output stays byte-identical to today.
2. `umbra run <scenario> --export <path>` with the format taken from the extension (`.geojson` or `.kml`) and `--export-every N` (default: one sample per simulated second). Samples come from the existing `EntityDisplay` (lat, lon, height, heading, speed).
   - **GeoJSON** (RFC 7946): one `FeatureCollection`; per entity one `LineString` track and one `Point` per sample with `properties` `{ entity, name, template, force, tick, time, heading_deg, speed_mps, status }`. `time` is ISO 8601 from a fixed epoch (default `1970-01-01T00:00:00Z`, `--epoch` to override), never the wall clock. Coordinates in RFC 7946 order (lon, lat, height).
   - **KML** 2.2: one `Folder` per force; per entity one `Placemark` with a `gx:Track` (`when` plus `gx:coord`), so Google Earth's time slider plays the run.
   - Fixed number formatting (7 decimals for degrees, 2 for metres), stable key and element order; written with `serde_json` and a small escaping writer; no new dependency.
3. The trace is recorded host-side by the behavior interpreter through an opt-in, additive API (for example a recorder that the CLI enables). A disabled recorder must leave behavior and timing paths unchanged. The trace never crosses the plugin ABI: a plugin behavior (M4+) appears as one opaque node. Exact types are left to the M3b plan.
4. Docs: `docs/behavior.md` (reading a trace) and a short "watch a run" section in `README.md`.

**Acceptance criteria**
1. For `walking_skeleton` and `patrol_and_engage`, `--trace` output and both export files are byte-identical across two runs and across 4 processes (same seed).
2. Enabling `--trace` and/or `--export` leaves per-tick state hashes unchanged: final `849026656b7b42d2` (walking skeleton) and `3a7bfda0a76111b9` (patrol_and_engage, tick 4200).
3. The patrol_and_engage trace shows blue-1's first contact at tick 328 and its first `Fire` at tick 1130 (the M3 evidence values).
4. GeoJSON passes an RFC 7946 structure test; KML is well-formed XML with one `gx:Track` per entity; sampled coordinates equal the engine's `EntityDisplay` values to the written precision.
5. A manual check (walking skeleton opened in Google Earth or QGIS and played back) is recorded in `docs/milestones.md`.
6. All gates in `docs/build.md` pass; the A-001 freeze check passes; all M0-M3 tests pass unchanged.

## M5b: live map, the first GUI slice
Built in `gui/` per `docs/frontend-architecture.md` and `gui/CLAUDE.md`, with the frontend workflow (frontend-architect, frontend-test-engineer, frontend-engineer, code-reviewer, verify-frontend).

**Deliverables**
1. The `gui/` Bun workspace scaffold: root `package.json` with the scripts from `gui/CLAUDE.md`, `bun.lock`, `tsconfig.base.json` (strict, F-3), Biome, dependency-cruiser layer rules, `bun test` with happy-dom, Playwright.
2. Foundation features: `@umbra/engine-client` (generated from `api/openapi.json` by `bun run gen:api`, F-7), `@umbra/engine-stream` (WebSocket `/api/v1/stream`, JSON encoding with the negotiation hook, subscription filters, entity store, per-frame batching), `@umbra/ui-kit` (minimum: i18n helper, Panel).
3. Domain feature `@umbra/map-view`: the `MapView` interface and its first implementation, an imperatively rendered entity layer (no React render per entity), owned and remote entities styled differently (REQ-K-5; remote entities appear from M6).
4. Shell `@umbra/shell-dashboard` with the map only (M8 adds controls and inspector), and the `src/` app (providers, router, runtime `/config.json` with the engine URL). Development and e2e use the Vite proxy to reach the engine, so the server needs no CORS change.
5. A map-library ADR (spec open question 6 moves from M8 to M5b; F-8 now says "decided at M5b"). Proposal for that ADR: MapLibre GL JS (2D, permissive licence, no token); tests use no remote tiles (blank or bundled basemap), an online basemap is a runtime option.

**Acceptance criteria**
1. Playwright against a real headless `umbra` engine serving `walking_skeleton` in stepped mode: all entities appear on the map; after `step(n)` through the generated client, the entity layer's positions equal the engine's entity list for that tick (within a stated coordinate tolerance), and the markers have moved.
2. `bun run typecheck`, `lint`, `test`, `check:deps` and `test:e2e` pass (`verify-frontend`); `bun run gen:api` produces no diff.
3. The GUI imports no engine source (REQ-A-4, enforced by dependency-cruiser).
4. M5b changes no engine crate behavior and no engine acceptance criterion. If the GUI needs something the M5 contract lacks, that is a separate, additive api-engineer work item through `update-openapi` with the OpenAPI drift check; it never reopens M5.

## M8: changed scope
M8 is "full GUI and Windows app". It **extends the M5b app** (the MapView, generated client, stream store and dashboard shell come from M5b): `@umbra/entity-inspector`, `@umbra/scenario-control`, `@umbra/template-browser`, `@umbra/plugin-status`, `@umbra/log-view`, shells `dashboard` (extended), `templates`, `plugins`, `settings`, and Tauri packaging for Windows (sidecar `umbra`). The original acceptance criteria stay and are extended.

## Amended section 17 table
Rows marked **new** or **changed (scope)** are A-002 changes. All other rows are unchanged apart from the mechanical rename (D-35).

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

## Ordering rules
1. The mandatory engine milestones M0, M1, M2, M3, M4, M5, M6, M7, M9 keep strict order (root `CLAUDE.md` rule 1). The rename (ADR 0018) is a work item between M3 and M4, not a milestone.
2. **M3b** is optional. It may be picked up any time after M3 passes, only when the human asks. No milestone depends on it; M4 may start without it. When picked up, it runs in its own work items and commits, never mixed into another milestone's work item.
3. **M5b** may start once M5's acceptance criteria pass. It runs on the GUI track and may run alongside M6 and M7; no engine milestone waits for it, and it changes no engine milestone.
4. **M8** starts after M7 (as today) **and** after M5b. M8b follows M8 (as today).

## Decision log rows (to add to spec section 2 on acceptance)
| ID | Decision | Why | Rejected alternatives |
|---|---|---|---|
| D-35 | **The project is Umbra, a CGF engine; full rename**: crates `umbra-*`, binary `umbra`, GUI scope `@umbra/`, C ABI prefix `umbra_`/`Umbra`/`UMBRA_`, env prefix `UMBRA_`, schema IDs `urn:umbra:schema:<name>:v<N>`; "CGF" stays the domain term; M0-M3 content-identical to `pre-umbra` apart from the mechanical rename; tags kept (ADR 0018, A-002) | One name before M4 fixes the ABI and header names; cheapest now (no external users) | Docs-only rename; rename after M4 (needs an ABI major bump) |
| D-36 | **Earlier visual feedback**: optional, non-blocking M3b (deterministic `--trace`, offline GeoJSON/KML export) and M5b (live map as the first real GUI slice in `gui/`, extended by M8); map library decided at M5b (A-002) | Behavior debugging and seeing runs long before M8; M8 builds on M5b instead of replacing it; engine milestones unchanged | Wait for M8; a throwaway viewer; any GUI inside the engine (D-1) |

## Compatibility (hard constraints for implementers)
1. **No engine milestone acceptance criterion is weakened.** M0-M7 and M9 criteria are unchanged apart from the mechanical rename. M8's criteria are a superset of the old ones. M8b is unchanged.
2. **M0-M3 stay frozen** (A-001 rules, scope per ADR 0018 C10). M3b is additive only: new CLI flags and an opt-in trace API; with the new flags off, every existing output is byte-identical; no existing test or fixture changes.
3. M3b and M5b do not change file formats (`schema_version` 1), component IDs, the state hash (v2) or the plugin ABI.
4. M5b uses only the generated client and the stream schema (REQ-A-4). Contract gaps go through an additive `update-openapi` work item.
5. Determinism (REQ-K-4): the trace and exports read state after the tick and use no wall clock, no `HashMap` order and no randomness.

## What changes in the spec (applied by the architect after acceptance and after the rename, WI-R4)
- Title "Umbra: Software Specification v0.3" with subtitle "a Computer Generated Forces (CGF) engine"; changelog entry v0.2 -> v0.3 (A-002).
- Section 2: rows D-35 and D-36.
- Sections 3, 4, 10, 12, 13, 18: names per ADR 0018 (done by the rename itself); REQ-P-1 and REQ-SDK-2 names per ADR 0018 C5.
- Section 13: "map library decided at M5b"; the live map arrives in M5b and M8 extends it.
- Section 17: the table above. Section 19 question 6 moves to M5b.
- `docs/milestones.md`: rows "M3b Run trace and track export (optional, A-002)" and "M5b Live map (A-002)"; the D-35 evidence note.
- Root `CLAUDE.md`: the block below; the frontend workflow line reads "milestones M5b, M8 and M8b".
- `gui/CLAUDE.md` F-8: "decided at M5b". `docs/frontend-architecture.md`: mark which packages M5b delivers.

### Amendments block (append to root CLAUDE.md)
```
- A-002 (Umbra rename; visual timeline; decisions D-35, D-36): see docs/amendments/A-002-umbra-and-visual-timeline.md and ADR 0018. M3b (trace and GeoJSON/KML export) is optional and non-blocking, picked up only when the human asks. M5b (live map) is the first GUI slice, after M5; M8 extends it. Mandatory engine milestones keep strict order.
```

## Work to apply this amendment (give to the architect agent)
1. After acceptance: run the rename (ADR 0018, WI-R1..R3), then WI-R4 (the spec and doc edits above).
2. Plan M4 as usual. Plan M3b with `implement-milestone` only when the human asks.
3. After M5: frontend-architect plans M5b (packages, tiers, public APIs, the map-library ADR).
4. Record any assumptions made in `docs/assumptions.md`.
