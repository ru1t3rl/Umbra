# ADR 0012: Behavior graph file format v1 (`*.behavior.toml`)

- **Status**: Accepted (decided by the human project owner on 2026-10-17; M3 plan `docs/plans/M3.md`). Spec decision D-29.
- **Requirement IDs**: REQ-B-6, REQ-B-7, REQ-B-9, REQ-B-10, REQ-B-12, REQ-B-14, REQ-B-16, REQ-T-2, REQ-T-5, D-14, D-26, ADR 0010
- **Affected**: `cgf-templates` (graph reader, discovery), `cgf-core::graph`, user files, `docs/behavior.md`, `crates/cgf-templates/schemas/behavior.v1.schema.json`

## Context
REQ-B-6 fixes the top-level keys (`id`, `schema_version`, `version`, optional `[interface]`, `root`,
`[nodes.*]`) and A-001 gives an example; node settings, FSM syntax, blackboard declaration, how interface
parameters reach nodes, and how graph files are found are not specified. The format becomes a contract on
the first release and must reuse the M2 pipeline (A-001 constraint 4) without changing M2 formats.

## Options
1. **Strict, hand-read format like ADR 0010, one graph per file, discovered by the `.behavior.toml`
   suffix in the existing `template_paths`.** No scenario key is added.
2. **Same format, plus a new `[scenario] behavior_paths` key.** Pros: explicit. Cons: a scenario format
   change for no new capability; two search lists to keep in sync.
3. **Inline graphs inside template files.** Cons: no reuse across templates, conflicts with ADR 0010.

## Decision
Option 1.
- **Top level:** `schema_version` (shared `SCHEMA_VERSION = 1`, ASM-057 policy), `id` (ADR 0002 component
  ID, globally unique with registered components, ASM-063), `version` (semver, required, REQ-B-14),
  optional `description`, `root` (node name), `[interface]`, `[blackboard]`, `[nodes.<name>]`, `[editor]`.
- **Nodes** are tables keyed by name (`[a-z][a-z0-9_]*`) with a `type`. Settings are top-level node keys:
  `sequence`/`selector` (`children`, optional `memory = false`), `parallel` (`children`, `policy =
  "all"|"any"`, default `all`), `inverter`/`succeed_always` (`child`), `repeat` (`child`, optional `count`
  >= 1, absent = forever), `cooldown`/`timeout` (`child`, `duration_s` > 0), `condition`/`action` (`ref`,
  optional `params`), `subgraph` (`ref`, optional `version` semver range, `params`, `outputs`), `fsm`
  (below). The graph is a tree: every node except `root` has exactly one parent; unknown, orphaned, reused
  or cyclic node references are errors (ASM-075).
- **FSM node:** `initial` (state name, required), `states = { <state> = "<node>" }`, optional
  `exits = { <exit> = "success"|"failure" }`, and `[[nodes.<fsm>.transitions]]` with `from`, `to` (state or
  exit) and exactly one of `when = "<guard node>"` or `on = "success"|"failure"`. A guard node's subtree
  may contain only `condition`, `inverter`, `sequence`, `selector`. Hierarchy: a state's node may itself
  be an `fsm`. Unreachable states and a missing or unknown `initial` are errors (REQ-B-12).
- **Parameter references and input bindings** (two distinct reserved forms, whole values only, no
  expressions, REQ-B-15): in `condition`, `action` and `subgraph` `params`, a value written exactly as
  `{ param = "<interface param>" }` is replaced by that interface parameter's value at build time (type
  compatibility checked at load; bounds at each call site after substitution, ASM-076). In `subgraph`
  `params` only, a value written exactly as `{ key = "<blackboard key>" }` binds the child's param to a
  key of the calling graph's blackboard, copied before each tick (ADR 0014). Leaf parameters that name
  blackboard keys are plain strings (`target_key = "target"`, `store_as = "target"`), never these forms.
- **`[interface]`** and **`[blackboard]`**: ADR 0014.
- **`[editor]`**: any table; the engine drops it after parsing (M2 parse-stage value rules still apply);
  it never reaches `GraphDef`, the state hash or replay (REQ-B-16).
- **Discovery:** under `template_paths` and in `validate` directories, files whose name ends in
  `.behavior.toml` (case-sensitive) are graphs; all other `.toml` files keep the ADR 0010 rules.
- **Diagnostics:** the M2 format and sorting, with new stable codes (list in the M3 plan).

## Consequences
- Easier: one discovery walk and one report for templates, scenarios and graphs; editors get
  `behavior.v1.schema.json` and the catalog; a graph ID is a valid template `[behavior] ref` (REQ-B-6).
- Harder: strict tree rules reject editor drafts with loose nodes (revisit in M8b); reading by hand is
  more code than serde (same trade-off as ADR 0010).
- Additive evolution as ADR 0010: new optional keys or node types do not bump `schema_version`; a new node
  type needs an ADR (`add-behavior-node` skill step 1).
- Revisit if users need graph libraries outside scenario directories (M5 content dirs, REQ-X-12).
