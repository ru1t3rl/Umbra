# Amendment A-001: Behavior graphs, subgraphs and the behavior editor

Status: Accepted. Spec version: v0.1 -> v0.2. Decisions: D-26, D-27 (published as D-18/D-19; renumbered 2026-10-17 because the v0.1-based patch dropped the project's existing D-18..D-25, which were restored).

## Decision
- The primary declarative behavior format is a **behavior tree** with two extra node types: **`fsm`** (embedded finite state machine, optionally hierarchical) and **`subgraph`** (reference to another graph by ID).
- Subgraphs are reusable pre-built pieces authored as data files, built-in, plugin-provided or third-party. They are referenced by ID (and semver range) through the existing registry.
- A **visual behavior editor** lives inside the GUI (not a separate app) and is driven by an engine **catalog** API and a **validation** API.

## Why
- In a tree a subtree is simply a node with a uniform result (Success/Failure/Running), so reuse needs no exit-point design. In a pure FSM, nested machines need explicit exit semantics and transitions tend to tangle across levels.
- FSM nodes keep mode-based logic (patrol / approach / engage) natural and easy to draw.
- All graphs emit the same intents (REQ-B-1), so the kernel, motion and network layers do not change.
- One validation pipeline and one registry (D-8, REQ-T-2) keeps rules in one place.

## Compatibility (hard constraints for implementers)
1. **M0-M2 are frozen.** Do not change template or scenario file formats, the registry's public API, or the validation error format. Additive extension only.
2. Existing templates referencing behaviors through `[behavior] ref` keep working; a behavior graph's `id` is a valid `ref`.
3. The M2 fixtures and tests MUST continue to pass unchanged. If a change would break one, stop and write an ADR with a migration plan.
4. The loader/validator for graphs reuses the M2 pipeline (error collection, file+line reporting, `schema_version` handling).
5. Anything in the registry that M2 shaped as "behavior by ID" is extended, not replaced: add graph, subgraph and catalog entries as new registry entry kinds or metadata.

## What changed in the spec
See `docs/amendments/spec-v0.1-to-v0.2.patch` (apply with `patch -p1 < docs/amendments/spec-v0.1-to-v0.2.patch` from the repo root) or replace `docs/SPEC.md` with the v0.2 file if you have not edited it. Summary: D-18/D-19; section 8.1 REQ-B-6..17; REQ-P-4 adds `condition`/`action` kinds (M7); REQ-X-10..12; M3, M5, M7 amended; new M8b; test strategy bullet; open question 3 resolved.

## Example graph
```toml
id = "acme.behavior.patrol_and_engage"
schema_version = 1
version = "1.0.0"
root = "main"

[interface]
params = { aggression = { type = "number", default = 0.5 } }

[nodes.main]
type = "selector"
children = ["engage", "patrol"]

[nodes.engage]
type = "sequence"
children = ["enemy_near", "suppress"]

[nodes.enemy_near]
type = "condition"
ref = "builtin.cond.enemy_in_range"
params = { range_m = 2500.0 }

[nodes.suppress]
type = "subgraph"
ref = "acme.subgraph.suppress_target"
version = "^1.0"
params = { duration_s = 20.0 }

[nodes.patrol]
type = "action"
ref = "builtin.action.follow_route"

[editor]   # ignored by the engine, excluded from state hashes
positions = { main = [0, 0], engage = [-160, 120], patrol = [160, 120] }
```

## Work to apply this amendment (give to the architect agent)
1. Apply the spec patch (or replace SPEC.md), then add the **Amendments** block below to the root `CLAUDE.md` and the **Frontend inventory additions** to `docs/frontend-architecture.md`.
2. Plan M3 (or re-plan if M3 already started) against REQ-B-6..17 with `implement-milestone`; list which existing M2 registry/loader pieces are reused and prove nothing in M2 changes (compatibility checklist above).
3. Add skills `add-subgraph` and `add-behavior-node` (included in this update) to `.claude/skills/`.
4. Record assumptions made (node evaluation order, blackboard typing, FSM exit naming) in `docs/assumptions.md`.

### Amendments block (append to root CLAUDE.md)
```
## Amendments
`docs/amendments/` holds accepted spec amendments; they override `docs/SPEC.md` text where they differ until SPEC.md is updated.
- A-001 (behavior graphs, subgraphs, editor): see docs/amendments/A-001-behavior-graphs.md. M0-M2 are frozen: additive changes only; M2 fixtures must keep passing; a breaking change requires an ADR with a migration plan.
Skills: `add-subgraph`, `add-behavior-node`.
```

### Frontend inventory additions (append to docs/frontend-architecture.md table)
| Package | Type | Tier | Purpose |
|---|---|---|---|
| `@umbra/behavior-editor` | feature | domain | Node-and-arrow graph canvas (xyflow/React Flow behind an interface), property forms from catalog JSON Schemas, validation display, subgraph library panel, convert-selection-to-subgraph, `[editor]` layout persistence |
| `@umbra/shell-behaviors` | shell | | Behavior list, editor, validation and (later) live preview page |

The editor reads the catalog through `engine-client` hooks and never hard-codes node, condition or action types. It must not import other domain features; the shell wires it to other features. Delivered in milestone M8b.
