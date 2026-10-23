# Amendment A-004: Documentation milestone M10

Status: Accepted (decided by the human project owner on 2026-10-23, including the follow-up decisions H1..H5 below, relayed by the coordinator). Spec version: v0.3 -> v0.4, applied 2026-10-23. Decision: D-96 (next free number after the M8 block D-80..D-95). The milestone, its ordering, the file layout, screenshots, the Mermaid check and plain Markdown are decided. Only the items marked **(M10 ADR)** are details that the M10-planning ADR settles (numbering block below).

## Decision
1. **New milestone M10 "Documentation" (D-96).** It is mandatory and it is the **last** milestone. It starts only after M9 passes (ordering rules below).
2. **Rejected alternatives** (the human chose M10 over both):
   - *A parallel docs track* (like the M5b/M8 GUI track). The pages would be written against surfaces that are still changing (M7 plugin kinds and replay, M8/M8b GUI, M9 HLA) and rewritten later. Two branches would also edit the same docs.
   - *Folding the work into M9's "docs" deliverable.* This overloads the hardening milestone, delays `m9-done` for work that is not hardening, and mixes HLA reference docs with cross-cutting manuals.
3. The human's request, verbatim: "a markdown picture of the architecture with explanation on what talks to what, what segments there are and which are important classes within some of them. And a simple development manual and an actual user manual on how to use all separate parts (server, engine, cli, web-app, etc.)".

## Why
- The repository has precise reference documents that each milestone owns (`docs/architecture.md`, `docs/api.md`, `docs/templates.md`, `docs/behavior.md`, `docs/plugin-guide.md`, `docs/net/`, `docs/frontend-architecture.md`, `docs/build.md`). It has no entry point for a user or a new contributor. `README.md` still describes the agent pack.
- At v1 the surfaces are final, so they are documented once. A mechanical check then keeps the manuals current for every later change. Without the check, the manuals go stale.

## Scope: three deliverables
M10 adds documentation and the tooling that checks it. It adds no engine, server, plugin or GUI feature (see Compatibility).

### (a) Architecture overview: `docs/manual/architecture.md`
- **Diagrams are Mermaid** blocks in Markdown. GitHub, GitLab, VS Code and most viewers render them, and the source can be read as text, so an ASCII fallback is not needed. The existing ASCII internals diagrams in `docs/architecture.md` (engine handle and bus, network layer) stay where they are, because they show detail below the overview level.
- **Required diagrams:**
  1. Segments and layers: core, kernel, domain crates (motion, behavior, templates), engine facade, plugin boundary (API, host, SDK, plugins), network (`umbra-net`, adapters as plugins), server and API types, CLI, GUI (app, shells, domain and foundation features), contract files in `api/`.
  2. Crate dependency graph. Its edges must equal the workspace's normal dependency edges (checked).
  3. Runtime data flow: CLI or server -> `EngineHandle` -> engine thread -> tick phases -> bus -> WebSocket stream -> GUI store and map, and REST commands in the other direction.
  4. Plugin ABI calls as a sequence diagram: discover, `plugin.toml`, entry, descriptor, `init`, `register`, `tick_batch` (and the other kinds' calls), fault, `shutdown`.
  5. Network I/O: scenario adapters -> `NetLink` -> `NetworkManager` -> loopback or plugin bridge -> DIS/HLA adapter -> UDP/RTI, plus ingest and publish at the tick boundary.
  6. GUI packages by tier, and the GUI's link to the engine (generated client and stream only, REQ-A-4).
- **Key types** for each important crate (at least `umbra-core`, `umbra-kernel`, `umbra-engine`, `umbra-behavior`, `umbra-templates`, `umbra-plugin-host`, `umbra-sdk`, `umbra-net`, `umbra-server`, `umbra-api-types`) and for the foundation GUI features. Each entry gives one line of role and a link to the detail.
- **One source per fact (DRY).** The overview **extends** `docs/architecture.md` and `docs/frontend-architecture.md` and replaces no part of them:
  - the overview owns the big picture, meaning the diagrams, the segments and the key-type summary;
  - `docs/architecture.md` stays the internal reference, meaning the crate table with roles and dependencies, the subsystem internals and the dependency rules;
  - `docs/frontend-architecture.md` stays the GUI reference, meaning the package inventory, tiers and data flow.
  The overview links to these files for detail and does not copy their tables. Both files get a link to the overview in their first lines.

### (b) Development manual: `docs/manual/development.md`
Setup and prerequisites, and build and gates (both **link** to `docs/build.md` without repeating it). It also covers: repository layout; the agent team and the standard and frontend workflows (from `CLAUDE.md`, linked); milestones, ADRs, assumptions and amendments; how to add a crate, a plugin kind, a plugin, a network adapter, a behavior node or subgraph, and a GUI feature or shell (each points to its skill and guide); testing (determinism, property, plugin failure, golden, e2e; points to `docs/test-matrix.md`); conventions (requirement IDs in commits, errors, `unsafe`, determinism, dependency rules). The agent-pack content of today's `README.md` moves here.

### (c) User manual: `docs/manual/user/`
Task-oriented. Each section heading is a task ("Run a scenario headless", "Step a paused run over REST"), and each task gives the commands and the expected result. The manual covers each part:

| Page | Part |
|---|---|
| `getting-started.md` | Install or build, first run, first served run, open the web app |
| `scenarios.md` | Engine: scenarios, templates, behavior graphs and subgraphs, forces, sensors, clock modes, seeds |
| `cli.md` | Every `umbra` subcommand and flag, exit codes. This page becomes the one CLI reference |
| `configuration.md` | `umbra.toml` (every key) and the precedence flag > file > default |
| `server.md` | `umbra serve`, the control API by task, the WebSocket stream, generated clients |
| `web-app.md` | Web app and the Windows (Tauri) app: dashboard and live map, inspector, run controls, template browser, plugin status, log view, settings, behavior editor |
| `plugins.md` | Using plugins: paths, packages, `list-plugins`, fault policy. Writing plugins stays in `docs/plugin-guide.md` |
| `network.md` | Network adapters: loopback, DIS, HLA, and running two engines together |
| `recording-replay.md` | Recording a run and replaying it (M7) |

The exhaustive references stay where they are and are linked, not copied: `docs/api.md` (endpoints, problem codes, configuration keys; already checked by `crates/umbra-server/tests/docs_api.rs`), `docs/templates.md`, `docs/behavior.md`, `docs/net/*.md`, `docs/plugin-guide.md`. The **user documentation set** is `docs/manual/user/**` plus these linked reference pages.

### File layout and `README.md`
```
README.md                       front page: what Umbra is, quickstart (executed), links to docs/manual/, licence
docs/manual/README.md           index: who reads what (user, contributor, plugin author, integrator)
docs/manual/architecture.md     (a)
docs/manual/development.md      (b)
docs/manual/user/*.md           (c)
docs/manual/images/             screenshots (H2)
```
Plain Markdown, rendered by GitHub and editors, with no site generator in v1 (H4). The layout stays compatible with mdBook (one index, relative links, no generator-specific syntax), so mdBook can be added later without moving files.

Screenshots (H2) are generated by a Playwright script against a real headless engine running `walking_skeleton`, and committed under `docs/manual/images/`. They are regenerated when the GUI changes. They are not the test baselines in `gui/e2e/__screenshots__/`. Script location, theme and viewport: **(M10 ADR)**.
`README.md` is rewritten as the project front page. Its "Running scenarios" list moves to `docs/manual/user/cli.md`, and its agent-pack text moves to the development manual.

## Mechanical check: gate 17
`cargo xtask check-docs`, written test-first with negative fixtures in the same way as `check-deps`. It becomes **gate 17 "Docs"** in `docs/build.md` and `scripts/gates.sh`, active from M10 and then mandatory for every change (H3). The command names, fence syntax and exact list sources are details for the **(M10 ADR)**. Without the gate, a later flag or endpoint makes the manual stale without anyone noticing. xtask keeps no workspace dependency (ASM-002) and runs the built binary, as `gen-api` runs an example. The checks:
1. **Coverage.** Each item of each list below must be named in the place shown:
   - workspace members (`cargo metadata`) and GUI packages (`gui/features/*`, `gui/feature-shells/*` `package.json` names): in the overview;
   - `umbra` subcommands and long flags (`umbra --help`, `umbra <sub> --help`): in `cli.md`;
   - `umbra.toml` keys (`config.v1.schema.json`): in the user documentation set;
   - top-level keys of the template, scenario and behavior schemas: in the user documentation set;
   - OpenAPI operations (method and path, `api/openapi.json`), each OpenAPI tag (at least one task section in `server.md`), and stream message types (`api/stream.schema.json`): in the user documentation set;
   - `.claude/agents/*` and `.claude/skills/*`: in the development manual.
2. **Consistency.** The edges of the crate dependency diagram equal the workspace's normal dependency edges. Every type named in a key-types table exists as a `pub` item in its crate.
3. **Links.** Every relative link and `#anchor` in `README.md` and `docs/**/*.md` resolves. The gate does not check external URLs, because it runs offline.
4. **Commands.** Every `sh` block in `README.md` and `docs/manual/` is executed, unless it is marked no-run with a reason (needs Windows, a GUI or a network peer). Executed blocks run in a temporary directory against the release build, the built plugins and `examples/`, and must exit 0 and print any expected lines that are given. The tool reports the number of no-run blocks. The exact fence syntax is decided by the ADR.
5. **Diagrams (H3).** Every Mermaid block parses with a pinned Mermaid version, as part of gate 17. The parser is a new GUI dev dependency. The npm dependency set is closed (D-59), so the dependency, its version and how gate 17 reaches the GUI toolchain are recorded in the **(M10 ADR)**, which amends D-59.

## Acceptance criteria
1. `docs/manual/architecture.md` contains the six required diagrams. It names every workspace member and every GUI package, the dependency diagram equals the workspace graph, and every key type exists (`check-docs`). `docs/architecture.md` and `docs/frontend-architecture.md` link to it, and no table is copied between the three files (review).
2. The development manual covers the topics in (b), points to `docs/build.md` for gates and to the skills and guides for extension tasks, and lists every agent and skill (`check-docs`).
3. The user manual has the pages in (c). Every CLI subcommand and long flag, every `umbra.toml` key, every top-level template, scenario and behavior key, every OpenAPI operation and every stream message type is covered (`check-docs`). Every OpenAPI tag has a task section in `server.md`.
4. Every executable command block in `README.md` and `docs/manual/` runs and passes. No-run blocks have a reason, and their count is recorded here.
5. All relative links and anchors resolve. Every Mermaid block parses in gate 17. One manual check that GitHub renders the diagrams is recorded here.
6. `check-docs` has negative tests: a missing crate, a missing flag, a missing endpoint, a wrong dependency edge, an unknown key type, a broken link or anchor, a failing command block and an invalid Mermaid block each give exit 1. Gate 17 is listed in `docs/build.md` and run by `scripts/gates.sh`.
7. `README.md` is the front page with an executed quickstart and a link to `docs/manual/README.md`. The web-app manual's screenshots come from the Playwright script against `walking_skeleton`, and a rerun of the script reproduces them (the ADR states the comparison tolerance).
8. Walk-through, recorded here: on a clean Linux checkout, someone who follows only `getting-started.md` builds Umbra, runs `walking_skeleton`, serves it and sees the entities in the web app. The Windows app section is checked on Windows if available. Otherwise it is recorded as unverified (NFR-3).
9. M10 changes no engine, server, plugin or GUI behavior and no contract. `git diff m9-done...HEAD -- crates/ api/ examples/ gui/features gui/feature-shells gui/src` is empty, apart from changes under Compatibility rule 2. The goldens are unchanged, and every gate in `docs/build.md` (including 15 and the new 17) passes.

## Ordering rules
1. M10 starts only after **M9** passes. It is the last milestone, and every mandatory milestone comes before it.
2. **GUI pages (H1).** M8 and M8b are on the GUI track and may still be open when M9 passes. M10 may then start and overlap with them. Its work items that document M8 or M8b surfaces (`web-app.md`, the GUI part of the overview) wait for `m8-done` or `m8b-done`. M10 is accepted and tagged `m10-done` only after `m9-done`, `m8-done` and `m8b-done` (the ADR 0047 pattern).
3. **M3b** is optional. If it is done before M10, the manual covers it. If it is done later, M3b updates the manuals itself, and gate 17 enforces this.
4. **Until M10, every milestone keeps updating the docs it already owns** (each milestone's docs deliverable). In particular, **M9 keeps its "docs" deliverable:** the HLA adapter guide and mapping (in `docs/net/`), the C# plugin sample guide and the benchmark results, all in the reference docs. M10 writes no reference content that a milestone owns. It writes the cross-cutting overview, the manuals and the README, and links to the references. A gap that M10 finds in a reference doc is fixed in that doc, in an M10 work item.

## Compatibility
1. No acceptance criterion of M0-M9, M3b, M5b, M8 or M8b changes. File formats, the state hash, the plugin ABI and the control API are unchanged.
2. Allowed outside docs: `xtask` (`check-docs` and its tests), `scripts/gates.sh`, the screenshot script, and the Mermaid parser dev dependency with its script under `gui/` (outside the feature, shell and app source). A defect that the documentation work finds, such as a wrong `--help` text or a behavior bug, is a separate work item that follows the normal workflow (and `update-openapi` for a contract change).

## Who
The `architect` plans M10 (skill `implement-milestone`) and writes the overview, with the `frontend-architect` for the GUI part. The owning specialists write the user pages for their parts: `rust-engineer` (engine, scenarios, CLI), `api-engineer` (server), `plugin-abi-engineer` (plugins), `network-engineer` (network), `frontend-engineer` (web and Windows app). The `build-engineer` writes `check-docs` and gate 17, the `test-engineer` writes its negative tests, and the `code-reviewer` reviews every page for accuracy against the spec and the code. No new agent.

## Numbering
Decision row D-96 is this decision. M10 reserves **D-97..D-111, ASM-1000..ASM-1099 and ADR 0090..0099** (`docs/assumptions.md`). Numbering outside the block skips it.

## Decision log row (spec section 2, applied 2026-10-23)
| ID | Decision | Why | Rejected alternatives |
|---|---|---|---|
| D-96 | **Documentation milestone M10, mandatory and last** (A-004, human decision 2026-10-23). Architecture overview with Mermaid diagrams, development manual and task-oriented user manual per part under `docs/manual/`; `README.md` becomes the front page; one source per fact (manuals link to the reference docs); plain Markdown (mdBook-ready); screenshots generated by Playwright; coverage, links, commands and Mermaid parsing checked mechanically in gate 17. Starts after M9 and may overlap M8/M8b: pages for their surfaces wait for `m8-done`/`m8b-done`, and acceptance comes only after `m8b-done`. Until then, milestones keep updating the docs they own (M9 keeps its docs deliverable) | One entry point for users and contributors; v1 surfaces are documented once when they are final; the check keeps the manuals current afterwards | A parallel docs track (writes against moving surfaces, rework); folding it into M9's "docs" deliverable (overloads hardening, delays `m9-done`) |

## What changes in the spec (applied 2026-10-23 by human decision H5, as ADR 0047 did)
- Title v0.4. Changelog entry v0.3 -> v0.4 (A-004): adds D-96 and M10. Compatibility: nothing else changes.
- Section 2: row D-96.
- Section 17: new last row `| **M10** (new) | Documentation | Architecture overview (Mermaid), development manual, user manual per part, README front page, docs check | Overview names every crate and GUI package and matches the dependency graph; every CLI subcommand and flag, umbra.toml key, endpoint and stream message covered; commands executed, links checked, Mermaid parsed in gate 17; walk-through recorded; no behavior or contract change |`.
- Section 17 ordering paragraph, new sentence: "M10 (A-004, D-96) is mandatory and the last milestone. It starts after M9 and may overlap M8 and M8b. Its work items for M8/M8b surfaces wait for `m8-done`/`m8b-done`, and M10 is accepted only after `m8b-done`."

## Human decisions 2026-10-23 (the former open questions)
| # | Question | Decision | Rejected |
|---|---|---|---|
| H1 | M10 and the GUI track | M10 starts after M9 and may overlap M8/M8b. Its M8/M8b pages wait for `m8-done`/`m8b-done`, and acceptance comes only after `m8b-done` (ordering rule 2) | Start only after M9, M8 and M8b have all passed (simpler, but M10 may wait idle) |
| H2 | Screenshots in the web-app manual | Generated by a Playwright script against `walking_skeleton`, committed under `docs/manual/images/` | No screenshots; reuse of the test baselines (ties docs to tests) |
| H3 | Mermaid check | Every block is parsed in gate 17 with a pinned Mermaid version. The new GUI dev dependency gets its ADR in M10 planning (D-59) | Manual render check only |
| H4 | Docs site generator | Plain Markdown for v1, with an mdBook-compatible layout | mdBook (or another generator) now |
| H5 | Spec text | The spec edits above are applied now, as ADR 0047 did | Apply them later in a separate change |

**Left to the M10-planning ADR (ADR 0090..0099):**
- the `check-docs` design: list sources, fence syntax for runnable and no-run blocks, and the place of gate 17 in the gate order;
- the Mermaid parser dependency and how gate 17 reaches the GUI toolchain (this amends D-59);
- the screenshot script: location, theme, viewport, and the tolerance for comparing a rerun.
