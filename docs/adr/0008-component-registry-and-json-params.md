# ADR 0008: Component registry in `umbra-core`, JSON-valued parameters, schemas provided by components

- **Status**: Accepted (approved by the human project owner on 2026-10-17; M2 plan `docs/plans/M2.md`)
- **Requirement IDs**: REQ-T-1, REQ-T-3, REQ-M-4, REQ-P-3, REQ-P-6, REQ-C-6, D-8, D-14, REQ-A-1..3, ASM-018
- **Affected**: `umbra-core` (new `params`, `registry` modules, new external dependency `serde_json`), `umbra-motion` (built-in factory), `umbra-templates` (lookups), `umbra-engine` (composition), later `umbra-plugin-host` (M4 registers plugin capabilities here)

## Context
Templates name motion models, behaviors, sensors and components by ID only and must not know whether an
ID is built-in or from a plugin (REQ-T-1). The component behind each ID supplies its parameter schema as
JSON Schema (REQ-T-3, REQ-P-3), and plugin parameters cross the C ABI as UTF-8 JSON (REQ-P-6). D-8 asks
for one registry for every kind. The rule table constrains where it can live: `umbra-templates` may depend
only on `umbra-core`, `umbra-motion` only on `umbra-core`, `umbra-plugin-host` only on `umbra-core` and
`umbra-plugin-api`. The registry type and the factory trait must therefore be visible from `umbra-core`.
Parameters need one in-memory representation shared by templates (producer), built-ins and the plugin
host (consumers). ASM-018 deferred serde to M2.

## Options
1. **Registry and factory trait in `umbra-core`; parameters are `serde_json::Value`; schemas are
   hand-written JSON documents shipped by each component.** `umbra-core` gains `serde_json` (MIT OR
   Apache-2.0; brings `serde`, `itoa`, `ryu`, `memchr`).
   - Pros: every crate that needs it can see it within the rule table; the JSON value type matches the
     ABI form (REQ-P-6) so M4 needs no second representation; `serde_json::Map` is a `BTreeMap` by default
     (deterministic order); built-ins provide schemas exactly as plugins will (dogfooding, REQ-SDK-3
     spirit); no derive macros on domain types.
   - Cons: `umbra-core`'s public API exposes a `serde_json` type (a 1.x crate, stable for years); `core` is no
     longer dependency-free beyond `thiserror`.
2. **Same placement, but an own `ParamValue` enum in `umbra-core` (no dependency).**
   - Pros: `umbra-core` stays tiny. Cons: about 150 lines of value type, conversions and a JSON writer that
     the plugin host would need in M4 anyway; duplicates a well-tested crate (DRY).
3. **Registry in `umbra-engine`, with a schema-lookup trait defined in `umbra-templates`.**
   - Pros: `umbra-core` unchanged. Cons: `umbra-plugin-host` cannot reach the registry (it may not depend on
     `umbra-engine`), so M4 needs a second registration path or an adapter layer; two traits for one concept.
4. **Schemas generated with `schemars` from serde-derived parameter structs.**
   - Pros: no hand-written schema. Cons: serde and schemars derives on component types, unit-suffixed
     field names (`max_speed_mps`) diverge from typed fields (`max_speed: MetersPerSecond`), output dialect
     and `$defs` layout change between schemars versions, and plugins cannot use it (they ship JSON anyway).

## Decision
Option 1.
- `umbra_core::params`: `pub use serde_json::Value as ParamValue;` and `ParamSchema` (a parsed JSON
  document, object at the root; `from_json_str`, `as_json`). Non-finite numbers cannot exist in a
  `ParamValue` (serde_json rejects them), which the loader turns into a located error.
- `umbra_core::registry`: `ComponentKind` (`MotionModel`, `Terrain`, `Behavior`, `Sensor`;
  `#[non_exhaustive]`), the generic `Factory<T: ?Sized>` trait (`id`, `param_schema`, `build(&ParamValue)
  -> Result<Arc<T>, BuildError>`), and one `Registry` holding a `BTreeMap<ComponentId, Arc<dyn Factory<..>>>`
  per kind. Registration rejects duplicate IDs. M2 registers only `builtin.ground.kinematic` (from
  `umbra-motion`) and `builtin.terrain.flat` (from `umbra-engine`); the behavior and sensor maps stay empty
  until M3 adds their product traits, so such IDs are unknown in M2.
- No serde derives on any `umbra-core` type. `umbra-templates` reads TOML by hand into its own located tree
  (ADR 0010) and converts validated values to `ParamValue`; TOML never leaves `umbra-templates`.
- Built-in schemas are hand-written JSON files next to the component (`include_str!`), each with an
  `examples` array that a contract test validates and builds.

## Consequences
- Easier: M4 plugin capabilities register into the same `Registry` (D-8) with their JSON schema strings
  unchanged; the GUI (M8) and `umbra schema` serve the same documents; one parameter type end to end.
- Harder: `umbra-core` depends on `serde_json`; a major bump of it would be a breaking change of our API
  (acceptable: none planned). `TerrainProvider` becomes a registry kind that REQ-P-4 does not list; the ABI
  kind list stays as specified until a terrain plugin is designed.
- `Factory::build` is called once per placement at load time, never per tick: no determinism or
  performance impact on the tick loop (REQ-K-4).
- Revisit if `umbra-core` must become `no_std` or dependency-free, or when M4 shows the ABI needs a
  different parameter encoding (byte buffers, REQ-P-6).
