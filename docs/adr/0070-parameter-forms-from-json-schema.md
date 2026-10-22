# ADR 0070: Parameter forms from JSON Schema: an own renderer in a foundation package

- **Status**: Proposed (frontend-architect, M8 plan `docs/plans/M8.md`, 2026-10-22)
- **Requirement IDs**: REQ-X-5 ("list templates (with param schemas)"), spec section 13 ("template browser", "editing UI ... driven by JSON Schemas"), A-001 / M8b ("property forms from catalog JSON Schemas"), A-003 (Vantage only), F-4, `gui/CLAUDE.md` rules 4 and 6
- **Affected**: new foundation package `@umbra/schema-form`, `@umbra/template-browser` (M8), `@umbra/behavior-editor` (M8b), decision row D-80

## Context
M8 acceptance: "the template browser lists templates with parameter forms from their schemas". The
contract gives every registered component a `params_schema` (`GET /api/v1/components`, `ComponentEntry`)
and every template its merged parameter values (`GET /api/v1/templates/{id}`, `ComponentRef.params`).
The built-in schemas are small and regular: `type: object`, `properties` of `number`, `string`,
`boolean`, bounds (`minimum`, `exclusiveMinimum`, `maximum`), `required`, `default`, `description`,
`examples`, `additionalProperties: false`, units by key suffix. Plugin schemas (M4+, more kinds in M7)
can be anything a plugin author writes. M8b needs the same renderer, editable and validated, inside the
behavior editor. Two domain features may not import each other, so a shared renderer must be a
foundation package. Every control must be a `@umbra/ui-kit` component (A-003).

## Options
1. **Own renderer in a new foundation package `@umbra/schema-form` over a documented subset, with a
   read-only JSON fallback for anything outside it (chosen).** A pure `model/` turns a schema plus a
   value into a field tree; components map each field kind to ui-kit (`TextField`, `Toggle`, `Select`,
   `ChipGroup`, `Card` for nested objects). M8 ships the read-only mode; M8b adds editing and
   client-side validation (library chosen then, `ajv` 2020 being the candidate).
   Pros: total Vantage compliance, small bundle, no theming layer, every construct tested, unknown
   constructs degrade visibly instead of breaking. Cons: we own the subset and must grow it when M7 or
   plugins use new constructs (a planned M7-dependent work item).
2. `@rjsf/core` (react-jsonschema-form, Apache-2.0) with a custom Vantage theme and `@rjsf/validator-ajv8`.
   Pros: most complete JSON Schema coverage, mature. Cons: a full custom theme (widgets, templates,
   field templates) to meet A-003, about 250 kB minified with ajv and lodash, its own state model that
   M8b's graph editor would have to wrap; class names and markup not ours.
3. JSON Forms (`@jsonforms/react`, MIT) with custom renderers. Pros: clean renderer registry. Cons: it
   wants a UI schema per form (none exists in the contract); custom renderers for every control anyway;
   similar size to option 2.
4. Hand-written forms per known component. Cons: breaks for every plugin and M7 schema; not "from their
   schemas".

## Decision
Option 1.
- Package `@umbra/schema-form`, tier `foundation`, depends on `@umbra/ui-kit` only, no npm runtime
  dependency in M8. Public API: `SchemaForm`, `SchemaFormProps` (`schema: unknown`, `value: unknown`,
  `label: string`, `idPrefix: string`, `readOnly: true` in M8). The model is internal.
- Supported subset in M8 (each with a test): `type` `object` (nested objects as labelled groups),
  `number`, `integer`, `string`, `boolean`, `string` `enum` (ChipGroup up to 5 values, Select above),
  arrays of primitives (read-only list), `title`, `description` (help text), `default`, `required`
  (marked), numeric bounds (shown as help), `examples` (not rendered), local `$ref` to `#/$defs/...` or
  `#/definitions/...` (non-recursive), and the unit from the key suffix (ASM-818).
- Anything else (`oneOf`, `anyOf`, `allOf`, `if`/`then`, `patternProperties`, tuples, recursive refs,
  a non-object root) renders that subtree as mono, read-only JSON with the note "Shown as JSON". Values
  present in the data but not in the schema render under "Other parameters". The component never throws
  on any input (property test over generated schemas).
- The engine stays authoritative for validity: M8 shows values that the engine already accepted.

## Consequences
- Easier: one renderer for M8 and M8b; Vantage by construction; forward compatible (unknown constructs
  are visible, not fatal).
- Harder: subset maintenance; M7's air and sea motion schemas are checked in an M7-dependent work item.
- Revisit when a real schema needs a construct outside the subset that is common enough to deserve a
  control, or when M8b's editing needs make option 2's coverage cheaper than growing our own.
