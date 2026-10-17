---
name: add-behavior-node
description: Add a new node type, condition or action to the behavior graph runtime (built-in), including catalog entry, parameter schema, validation and editor exposure. Use when extending what behavior graphs can express.
---

# Add a behavior node type, condition or action

Requirements: REQ-B-1, B-6..17, REQ-T-3, D-18, amendment A-001. Owner: `rust-engineer`.

1. **Decide the kind**: new node *type* (changes the format; needs an ADR and a `schema_version` consideration) vs new *condition/action* (a catalog entry; no format change). Prefer a condition/action.
2. **Contract**: pure read of state and perception; writes intents only (REQ-B-1). Define the result (Success/Failure/Running) for every input, including missing perception data.
3. **Catalog entry** (REQ-B-17): `id` (`builtin.cond.<name>` / `builtin.action.<name>`), description, param JSON Schema with defaults and units in names (`range_m`), source `builtin`. The editor is built from this, so descriptions and schemas are user-facing documentation.
4. **Determinism**: only `SimTime`, perception and `SimRng`; no wall clock, no unordered iteration (REQ-K-4).
5. **Validation**: the M2 pipeline must report bad params for this node with file and line. Add negative tests.
6. **Tests first**: unit tests for each result path, a scenario test via a graph using it, determinism check. Keep M2 fixtures unchanged and passing.
7. **Editor/API**: nothing to hand-code in the GUI; confirm the new entry appears in the catalog output and the generated schema (`update-openapi` if the catalog DTOs changed).
8. Run `review-change` and `verify-and-build`.

Custom conditions/actions from plugins arrive via the `condition` and `action` plugin kinds (M7) using `add-plugin-kind`.
