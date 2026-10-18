# ADR 0014: Blackboard, graph interface, input bindings and subgraph isolation

- **Status**: Accepted (decided by the human project owner on 2026-10-17, including dynamic input bindings; M3 plan `docs/plans/M3.md`). Spec decision D-31.
- **Requirement IDs**: REQ-B-5, REQ-B-10, REQ-B-11, REQ-B-12, REQ-B-14, REQ-T-3, ADR 0009, ADR 0012
- **Affected**: `umbra-core::behavior` (`Blackboard`), `umbra-core::graph` (`Interface`, `ParamTemplate`), `umbra-templates` (binding checks), `umbra-behavior`

## Context
REQ-B-10: an interface has typed parameters (JSON Schema), optional named outputs and result semantics;
call sites are validated. REQ-B-11: each subgraph instance has its own node state and scoped blackboard;
data crosses only through params (in) and outputs (out). REQ-B-12 requires rejecting invalid bindings at
load time, so keys and types must be known statically. The human asked (2026-10-17) that a call site can
bind a param to a parent blackboard key, so a parent can hand a dynamic value (e.g. a target) to a
subgraph. Entity IDs and positions are not JSON Schema types.

## Options
1. **Typed, declared blackboard; static params in; outputs out.** No way to pass a dynamic value in.
2. **Untyped blackboard, free bindings.** Bindings cannot be checked at load (REQ-B-12).
3. **Option 1 plus input bindings: a subgraph call site may bind any interface param to a parent key.**

## Decision
Option 3.
- **Blackboard:** every key is declared in `[blackboard]` with one type: `bool`, `number`, `integer`,
  `string`, `entity`, `position`. Values start unset; wrong-type writes are refused (ASM-071).
- **Interface params:** `params = { <name> = { <fragment>, default? } }`. A fragment with a JSON type
  (`boolean`, `number`, `integer`, `string`) goes into the assembled object schema (ADR 0009 subset;
  `required` = params without `default`, `additionalProperties = false`). A fragment with `type =
  "entity"` or `"position"` is a binding-only param: it has no literal form and is kept out of the JSON
  Schema (it is listed in `Interface::types` and in the catalog). Every param is also visible inside the
  child as a **read-only blackboard key of the same name and type** (it must not also be declared in the
  child's `[blackboard]`).
- **Two distinct forms in call-site `params` (ADR 0012):**
  - `{ param = "<name>" }`: a **build-time reference** to the enclosing graph's own interface param
    (ASM-076); allowed in `condition`, `action` and `subgraph` params; substituted once when built.
  - `{ key = "<parent key>" }`: a **run-time input binding** to a key of the calling graph's blackboard
    (including its read-only param keys); allowed only as a value in `subgraph` node params. Not allowed
    in template `[behavior] params` (a top-level behavior has no parent blackboard): `invalid_binding`.
  - Load-time checks: the parent key is declared and its type equals the param's type (`integer` may bind
    to a `number` param); a param that the child uses in a `{ param = .. }` reference cannot be bound,
    because its value is needed at build time (`invalid_binding`, note at the reference).
- **Input copy:** before each tick of the subgraph node, every bound param's parent value is copied into
  the child's read-only key; an unset parent key makes the child key unset. A **required** bound param
  (no `default`) that is unset makes the subgraph node return Failure without ticking the child (a Running
  child is halted). An optional bound param that is unset stays unset in the child (the default is not
  substituted for a bound param). Binding-only params with no binding are `missing_param` at the call site;
  a top-level graph therefore cannot have required binding-only params (`missing_param` at the template).
- **Result semantics:** `[interface] result` is optional free text describing what Success, Failure and
  Running mean (REQ-B-10); it is shown by the catalog. It is not a port and names nothing.
- **Outputs:** `outputs = { <port> = { type, from = "<key>" } }`; call site `outputs = { <port> = "<parent
  key>" }`; copied after each tick, including unset; one output writer per parent key (ASM-088).
- **Isolation:** each graph instance (each entity, each `subgraph` node in an instance) owns its node state
  and blackboard. A child never holds a reference to the parent's blackboard; inputs are copies into
  read-only keys, so child writes reach the parent only through bound outputs.
- **Lifetime:** a blackboard lives as long as its instance; halting does not clear it.
- **Versioning:** removing or renaming a param or port, or changing a type or meaning, is a major bump
  (REQ-B-14); an optional param or a port is minor. Documented, not machine-checked.

## Consequences
- Easier: targets and other dynamic values flow parent -> child explicitly; every binding is a located
  load-time diagnostic; isolation is structural and testable.
- Harder: two reserved one-key table forms (`param`, `key`) in params; params are visible both as build
  values and as blackboard keys, so the "bindable iff not used as `{ param }`" rule must be taught
  (`docs/behavior.md`).
- Revisit if plugins (M4) need structured (list or JSON) blackboard values.

## Amended 2026-10-17 after the WI-4 review
- "Listed in `Interface::types` and in the catalog" is implemented by a typed param map
  `param_types: BTreeMap<String, BbType>` on `ComponentInfo` and `CatalogEntry` (builders
  `with_param_type`/`with_param_types`), filled by `register_graphs` from `Interface::types`. Binding
  checks (`invalid_binding`, `missing_param`) and the Integer-to-Number input conversion read it for
  registered behaviors; plugin behaviors (M4) declare it.
- Both reserved forms are recognised by one public classifier, `umbra_core::reserved_form`
  (`ReservedForm::{Param, Key}`), applied only to values inside `params`, never to the `params` table
  itself; `ParamTemplate::substitute` reports a `Key` form as `SubstituteError::Binding`.
- `result` is free-text result semantics, not a port name (wording made explicit above).

## Amended 2026-10-17 after step 4 (acceptance tests)
Binding-only params (`entity`, `position`) have no literal form and therefore cannot have a `default`
(a `default` on one is `invalid_value`); they are always required. "An optional bound param that is unset
stays unset" applies only to literal-capable types (`bool`, `number`, `integer`, `string`) with a default.
