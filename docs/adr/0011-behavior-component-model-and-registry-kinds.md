# ADR 0011: Behavior component model: core traits, registry kinds, graph data model in `umbra-core`

- **Status**: Accepted (decided by the human project owner on 2026-10-17: option 1, including the one M2 unit test edit; M3 plan `docs/plans/M3.md`). Spec decision D-28.
- **Requirement IDs**: REQ-B-1, REQ-B-3, REQ-B-5, REQ-B-6, REQ-B-9, REQ-B-13, REQ-B-17, REQ-T-1, REQ-T-3, REQ-P-4, D-8, D-26, REQ-A-1..3, A-001 constraints 1-5
- **Affected**: `umbra-core` (`behavior`, `graph`, `catalog`, `registry`), `umbra-behavior` (new crate, already in the rule table), `umbra-templates`, `umbra-engine`, later `umbra-plugin-host` (M4/M7)

## Context
A behavior graph is parsed and validated by `umbra-templates` (A-001 constraint 4: same pipeline) and run
by `umbra-behavior`. Both may depend only on `umbra-core` (section 4). Graph leaves (conditions, actions) need
JSON Schemas the loader can check with file and line (REQ-T-2, REQ-T-3), so they must be visible through
the one registry (D-8). Subgraphs resolve "through the registry" whether the target is a file, built-in or
plugin graph (REQ-B-9). The catalog needs id, description, source, version and schema for each entry
(REQ-B-17). `ComponentKind` already has `Behavior` and `Sensor` (ASM-055). A-001 freezes the M2 public
API and tests; an exhaustive in-crate unit test (`umbra-core/src/registry.rs::all_lists_every_kind_once`)
fails to compile when a `ComponentKind` variant is added.

## Options
1. **New kinds `Condition` and `Action`; one product trait per kind; metadata on registration; graph
   definition types in `umbra-core`.** `Registry` gains `register_behavior/condition/action/sensor(factory,
   info)` and lookups; `ComponentInfo { description, source, version, ... }` is stored next to each
   behavior-family factory. File graphs become `Factory<dyn Behavior>` entries in a per-scenario clone of
   the registry. Pros: every lookup, message ("`x` is an action, not a condition") and catalog row is
   exact; M7's `condition`/`action` ABI kinds map 1:1. Cons: `all_lists_every_kind_once` needs two more
   match arms and `COUNT = 6` (mechanical, its stated purpose; no assertion changes meaning).
2. **Conditions and actions registered under `ComponentKind::Behavior` with a role tag.** Pros: no M2 test
   is touched. Cons: `ids(Behavior)` mixes roles, so M2's `unknown_id` notes list conditions as candidate
   template behaviors; the M2 template-ref check must special-case roles; a validated `[behavior] ref` to
   a condition would fail only at spawn (validate/run disagreement, ASM-058).
3. **A separate leaf registry inside `umbra-behavior`.** Violates D-8; `umbra-templates` cannot see it, so leaf
   parameters could not be validated with file and line.

## Decision
Option 1 (approved by the human 2026-10-17; option 2 is not used).
- `umbra-core::behavior`: `Status`, `Behavior`/`BehaviorInstance`, `Condition`, `Action`/`ActionInstance`,
  `Blackboard`, `BehaviorContext`, `EntityView`, `IntentBuffer` (signatures in the M3 plan).
- `umbra-core::graph`: the validated, format-free graph definition (`GraphDef`, `NodeDef`, `NodeKind`,
  `FsmDef`, `Interface`) that `umbra-templates` produces and `umbra-behavior` compiles. TOML never leaves
  `umbra-templates` (ADR 0008 rule kept).
- `umbra-core::catalog`: `Provenance { Builtin, File, Plugin }`, `ComponentInfo`, `CatalogEntry`, `Catalog`,
  built as a pure function of a `Registry` plus the static node-type table.
- `Registry` derives `Clone` (factories are `Arc`s). The engine clones the base registry per scenario and
  `umbra_behavior::register_graphs` adds that scenario's file graphs in dependency order.
- `umbra_engine::builtin_registry()` keeps exactly its M2 contents (an M2 test asserts it); the new
  `umbra_engine::standard_registry()` adds the M3 built-ins and is what `umbra-cli` uses.

## Consequences
- Easier: plugin behaviors (M4) and plugin conditions/actions (M7) register through the same calls with
  `Provenance::Plugin`; the catalog and editor (M5, M8b) read one source.
- Harder: `umbra-core` grows (graph and catalog data types); `umbra-behavior` tests cannot use `umbra-templates`
  (dev edges are checked, ASM-005), so they build `GraphDef`s in Rust; file-based tests live in `umbra-engine`.
- Migration (approved): the one M2 test edit is made in its own commit, reviewed as a
  diff of that test alone. Every other M2 test and fixture stays byte-identical.
- Revisit if a plugin needs one ID under two kinds (ASM-063), or if `umbra-core` must shrink.

## Amended 2026-10-17 after WI-3 review
Code-reviewer and plugin-abi-engineer approved WI-3 (b182969); refinements adopted for the core traits so
an FFI bridge (M4) can implement them: `Behavior::instantiate` and `Action::instantiate` return
`Result<Box<dyn ..Instance>, InstantiateError>` (`#[non_exhaustive]`, `thiserror`); `BehaviorInstance::output`
returns an owned `Option<BbValue>`; `BehaviorInstance::set_input(&mut self, param, Option<BbValue>) ->
Result<(), BlackboardError>` is a required method; `Contact`, `RouteView`, `BbType`, `BbValue` are
`#[non_exhaustive]` (structs with `new`). Batching, fault reporting and snapshots stay additive default
methods or fields for M4 (ASM-086). Signatures: `docs/plans/M3.md`, "`umbra-core` additions".
- Clarification (WI-5): `register_sensor(factory)` takes no `ComponentInfo`; sensors are not catalog material.
