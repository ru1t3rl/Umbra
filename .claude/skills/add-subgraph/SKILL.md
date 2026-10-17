---
name: add-subgraph
description: Create a reusable behavior subgraph (declarative file, built-in, or plugin-provided) with a declared interface, semver, validation and tests. Use when authoring or reviewing any graph meant to be referenced by other graphs.
---

# Add a subgraph

Requirements: REQ-B-6..17, D-18, amendment A-001. Owner: `rust-engineer` (declarative/built-in) or `plugin-abi-engineer` (plugin-provided).

1. **Purpose and interface first.** One responsibility ("suppress target", "move to cover"). Define `[interface]`: typed params (JSON Schema, with defaults), named outputs if any, and what Success, Failure and Running mean. Keep the interface small; it is a public contract (REQ-B-10, B-14).
2. **File**: `<namespace>.<name>.behavior.toml` with `id`, `schema_version`, `version` (semver), `[interface]`, `root`, nodes. No hidden shared state: use only params and the instance's scoped blackboard (REQ-B-11). Randomness only via `SimRng`, time only via `SimTime` (REQ-B-16).
3. **No cycles and bounded depth**: do not reference graphs that (transitively) reference this one; stay within the nesting limit (REQ-B-12).
4. **Version rules**: a breaking interface change (removed/renamed param, changed semantics) needs a major bump; additive optional params are minor.
5. **Provenance**: files are `file` source and editable; built-ins are `builtin`; plugin-provided ones are `plugin`. The last two are read-only for editors (REQ-B-13).
6. **Tests first**: validation (valid, wrong param type, missing param, bad version range, cycle), behavior (intents emitted per scenario tick), isolation (two instances do not share state), determinism (run twice, hashes equal).
7. **Docs**: a header comment describing the interface and an example call site; the catalog description field is what editors display.
8. Run `review-change` and `verify-and-build`. Confirm M2 fixtures still pass unchanged.

For plugin-provided subgraphs/leaves use the existing plugin mechanism (`add-plugin-kind` for new kinds); never add a second loader.
