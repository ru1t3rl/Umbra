# ADR 0002: Grammar for namespaced string IDs

- **Status**: Accepted (approved by the human project owner on 2026-10-17)
- **Requirement IDs**: REQ-C-2, REQ-T-1, REQ-P-3, REQ-M-2, REQ-X-5
- **Affected**: `umbra-core` (ID newtypes), later `umbra-templates`, `umbra-plugin-host`, `umbra-api-types`

## Context
REQ-C-2 defines `TemplateId`, `BehaviorId`, `MotionModelId` and `PluginId` as "string newtypes with dotted
namespaces like `acme.behavior.patrol`", but it gives no grammar. These IDs are a public contract: users
write them in template files, plugins send them across the C ABI in descriptors, and the REST API and
GUI show them. Changing the grammar after M2 would break user data. Examples in the spec:
`tank.mbt_generic`, `ground_vehicle.base`, `builtin.ground.kinematic`, `builtin.fsm.patrol_and_engage`,
`builtin.visual`, `acme.behavior.smart_tank`. The spec is also inconsistent: REQ-M-2 names
`ground.kinematic`, while the section 7.1 template uses `builtin.ground.kinematic`.

## Options
1. **Strict lower-case:** `segment ("." segment)+`, where `segment = [a-z][a-z0-9_]*`. At least two
   segments. At most 128 bytes in total. Pros: unambiguous on case-insensitive filesystems (Windows), safe
   in URLs, file names and JSON keys, and every spec example matches it. Cons: no upper case or hyphens
   (`acme-tools` must be written `acme_tools`).
2. **Permissive:** `[A-Za-z0-9_-]` segments, at least one segment. Pros: friendlier to users. Cons: case
   collisions (`Tank.Base` against `tank.base`) and IDs without a namespace that collide between vendors.
3. **Strict, but one segment allowed:** the same as option 1 with `+` replaced by `*`. Pros: allows a
   short `my_tank`. Cons: drops the namespace that prevents vendor collisions.

## Decision (proposed)
Option 1. The `builtin` first segment is reserved for engine-provided capabilities. The plugin host
(M4) rejects plugins that register `builtin.*` IDs. Full built-in IDs always include the prefix, so
REQ-M-2's `ground.kinematic` means `builtin.ground.kinematic`. The grammar is implemented once in a
single private validation function in `umbra-core` that all four newtypes share (DRY).

## Consequences
- Validation errors are precise (empty input, empty segment, bad character with index, too long, too few
  segments). The template loader (REQ-T-2) reports them with file and line.
- Relaxing the grammar later is backward compatible. Tightening it is not. That is why the strict option
  is proposed now.
- Revisit if users ask for Unicode or upper-case IDs. Display names belong in a separate `name` field,
  not in IDs.
