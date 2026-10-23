# Umbra: project instructions for all agents

Source of truth: `docs/SPEC.md` (export of the "Umbra: Software Specification"). Requirement IDs (REQ-*, NFR-*, D-*, A-*) are referenced in code comments, commits and PRs.

## Ground rules
1. Build strictly in milestone order (spec section 17). Never start Mn+1 before Mn's acceptance criteria pass. The mandatory engine milestones are M0-M7 and M9; the optional M3b and the GUI-track M5b follow the A-002 ordering rules (see Amendments). After them comes the documentation milestone M10, which is also mandatory and is the last milestone; it starts after M9 (A-004, see Amendments).
2. MUST/SHOULD/MAY in the spec carry their RFC meaning. Do not silently reverse a decision (D-*). If you think one is wrong, stop and write an ADR proposal in `docs/adr/` and ask the human.
3. Ambiguity: pick the recommended, best and most future-proof option consistent with the spec, accepted ADRs and SOLID/DRY (human decision 2026-10-21: not merely the simplest), record it with the alternatives considered in the current milestone's file under `docs/assumptions/` (index and numbering rule: `docs/assumptions.md`), continue.
4. Dependencies point inward (REQ-A-1..4). `umbra-core` depends on no workspace crate. `umbra-sdk` depends only on `umbra-plugin-api`.
5. `unsafe` only in `umbra-plugin-api`, `umbra-plugin-host`, `umbra-sdk`; every block has `// SAFETY:` and a test. Everywhere else `#![forbid(unsafe_code)]`.
6. No `unwrap`/`expect` on external input. `thiserror` in libraries, `anyhow` only in binaries.
7. Determinism (REQ-K-4): no wall clock, no `HashMap` iteration order, no ad-hoc randomness inside simulation logic. Use `Clock` and `SimRng`.
8. Definition of done for any change: every gate in the ordered local gate list in `docs/build.md` passes (it is canonical; there is no CI, see ADR 0004 / D-19); docs and tests updated; commit references requirement IDs.

## Team (subagents in `.claude/agents/`)
| Agent | Use for |
|---|---|
| `architect` | Design, ADRs, crate boundaries, trait design, spec questions |
| `rust-engineer` | Implementing kernel, templates, behavior, motion, engine facade |
| `plugin-abi-engineer` | `umbra-plugin-api`, host, SDK, FFI safety |
| `network-engineer` | `umbra-net`, DIS/HLA adapters |
| `api-engineer` | `umbra-server`, `umbra-api-types`, OpenAPI, stream schema |
| `frontend-architect` | GUI package plan: features, shells, tiers, public APIs, ADRs |
| `frontend-engineer` | GUI implementation under `gui/` with Bun (features, shells, app, Tauri) |
| `frontend-test-engineer` | GUI tests: bun test, Playwright, stream and architecture tests |
| `test-engineer` | Test strategy, determinism/property/plugin/network tests, benchmarks |
| `code-reviewer` | Read-only review against spec and principles |
| `build-engineer` | Workspace, CI, cross-platform builds, releases |

## Skills (in `.claude/skills/`)
Backend: `implement-milestone`, `add-crate`, `add-plugin-kind`, `add-network-adapter`, `write-tests`, `update-openapi`, `record-decision`, `verify-and-build`, `review-change`.
Frontend (`gui/`, Bun): `add-feature`, `add-feature-shell`, `generate-api-client`, `write-frontend-tests`, `verify-frontend`, `review-frontend-change`, `apply-design-system` (A-003).
Frontend rules live in `gui/CLAUDE.md` and `docs/frontend-architecture.md` (they supersede spec section 13).

## Standard workflow
1. `architect` plans the milestone slice (skill `implement-milestone`).
2. Specialist engineer implements one requirement ID at a time, test first.
3. `test-engineer` reviews coverage and adds determinism/property/failure tests.
4. `code-reviewer` reviews (skill `review-change`).
5. `build-engineer` runs full verification (skill `verify-and-build`).
6. Only then mark acceptance criteria met in `docs/milestones.md`.

## Frontend workflow (milestones M5b, M8 and M8b)
1. `frontend-architect` plans packages (feature vs shell, tier, public API).
2. `frontend-test-engineer` writes failing tests (`write-frontend-tests`).
3. `frontend-engineer` implements via `add-feature` / `add-feature-shell`, using `generate-api-client` after contract changes.
4. `code-reviewer` applies `review-frontend-change`.
5. `build-engineer` / anyone runs `verify-frontend` before done.
GUI work may begin after M5 (the API exists); the first GUI slice is M5b (live map), which M8 extends. GUI work must never block or change engine milestones.

## Model policy
Each agent pins its model in frontmatter (full IDs, so behavior does not shift when aliases move). Bump IDs deliberately when upgrading.

| Model | Agents | Why |
|---|---|---|
| `claude-opus-5-5` | `architect`, `frontend-architect`, `code-reviewer`, `plugin-abi-engineer` | Low-volume, high-stakes work: design, review, FFI/`unsafe`/ABI |
| `claude-sonnet-5-5` | `rust-engineer`, `network-engineer`, `api-engineer`, `frontend-engineer`, `test-engineer`, `frontend-test-engineer`, `build-engineer` | Well-specified, high-volume implementation guarded by spec, plans and tests |

Rules:
- Never set `CLAUDE_CODE_SUBAGENT_MODEL`; it overrides every agent's `model:` and defeats this policy.
- Escalation: if a Sonnet agent fails twice on the same task (hard determinism bug, tricky protocol issue), re-run that task with a per-invocation model override to Opus. Do not permanently upgrade the agent.
- Skills run in the main session's model. Use Opus (`/model`) when planning or orchestrating a milestone, Sonnet for routine execution.
- Verify after changing any frontmatter: ask Claude "list the project subagents and their models" or open the files in `.claude/agents/`. (The `/agents` wizard was removed in Claude Code v2.1.198.) Restart Claude Code if a new or edited agent is not picked up.

## Amendments
`docs/amendments/` holds accepted spec amendments; they override `docs/SPEC.md` text where they differ until SPEC.md is updated.
- A-001 (behavior graphs, subgraphs, editor; decisions D-26, D-27): see docs/amendments/A-001-behavior-graphs.md. M0-M2 are frozen: additive changes only; M2 fixtures must keep passing; a breaking change requires an ADR with a migration plan.
  Skills: `add-subgraph`, `add-behavior-node`.
- A-002 (Umbra rename; visual timeline; decisions D-35, D-36): see docs/amendments/A-002-umbra-and-visual-timeline.md and ADR 0018. M3b (trace and GeoJSON/KML export) is optional and non-blocking, picked up only when the human asks. M5b (live map) is the first GUI slice, after M5; M8 extends it. Mandatory engine milestones keep strict order.
  Ordering: M3b may be picked up any time after M3 passes, in its own work items; no milestone depends on it and M4 does not wait for it. M5b starts after M5 passes and may run alongside M6 and M7; no engine milestone waits for it. M8 starts after M5b and may run alongside M7 (ADR 0047, D-63): its M7-dependent work items wait for the M7 change they consume, and M8 is accepted and tagged only after `m7-done`; M8b follows M8.
- A-003 (Vantage design system, GUI only, no engine impact): see docs/amendments/A-003-design-system.md and docs/design-system/. Every GUI surface uses @umbra/ui-kit tokens and components; use the skill `apply-design-system` for any UI work. Human decision 2026-10-19: all of A-003 (tokens, ThemeProvider, checks, all 14 reference components, showcase in Ops and Day) is implemented in M5b as its first work items, before `map-view` and the dashboard shell (see docs/milestones.md, M5b).
- A-004 (documentation milestone M10; decision D-96; human decision 2026-10-23): see docs/amendments/A-004-documentation-milestone.md. M10 delivers three things under docs/manual/: the architecture overview (Mermaid diagrams), the development manual, and the user manual for each part. It also makes README.md the front page, uses plain Markdown in an mdBook-ready layout, adds screenshots generated by Playwright, and adds gate 17 (`cargo xtask check-docs`, including a Mermaid parse). M10 is mandatory and last.
  Ordering: M10 starts after M9 passes. Its work items for M8/M8b surfaces wait for those milestones, and M10 is accepted only after `m8b-done`. Until M10, every milestone keeps updating the docs it owns, and M9 keeps its docs deliverable. M10 changes no engine, server, plugin or GUI behavior and no contract.
