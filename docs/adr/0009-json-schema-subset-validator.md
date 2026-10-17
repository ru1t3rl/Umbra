# ADR 0009: JSON Schema subset with an in-house validator

- **Status**: Accepted (approved by the human project owner on 2026-10-17; M2 plan `docs/plans/M2.md`)
- **Requirement IDs**: REQ-T-2, REQ-T-3, D-14, REQ-K-4, NFR-4, REQ-P-3
- **Affected**: `cgf-templates` (`schema` module), component authors (built-in now, plugins in M4), `docs/templates.md`

## Context
Parameter schemas are JSON Schema (D-14, REQ-T-3). Validation must report **all** errors with file and
line (REQ-T-2), so every failure must map back to a TOML span; error order must be stable (REQ-K-4,
reproducible output). Our own schemas need little: object, number with bounds, required, no extra
properties, arrays of three numbers, enums, alternatives (ECEF or lat/lon). Plugin authors (M4) will
expect "JSON Schema", but plugins also must not be able to make validation non-deterministic or slow.

## Options
1. **Hand-rolled validator for a documented subset of draft 2020-12, fail-closed.**
   Keywords: `type` (string or array), `properties`, `required`, `additionalProperties` (bool or schema),
   `items` (single schema), `minItems`, `maxItems`, `enum`, `const`, `minimum`, `maximum`,
   `exclusiveMinimum`, `exclusiveMaximum`, `minLength`, `maxLength`, `oneOf`, `$defs` with local
   `$ref: "#/$defs/<name>"`. Annotations accepted and ignored: `$schema`, `$id`, `$comment`, `title`,
   `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly`. Any other keyword makes
   the schema invalid (compile error naming keyword and component).
   - Pros: no dependency; about 400 lines; deterministic by construction (schema keys walked in sorted
     order, instance errors sorted by span); errors carry the instance path so the loader attaches the
     exact TOML span; our wording. Cons: not the full standard (`pattern`, `format`, `allOf`, `anyOf`,
     `if/then/else`, `patternProperties`, remote refs are missing); we own its correctness.
2. **`jsonschema` crate (MIT).**
   - Pros: full standard, well tested. Cons: large tree (`regex`/`fancy-regex`, `referencing`, `ahash`,
     `url`/`idna`, optional HTTP resolving that must be disabled), frequent releases with MSRV moves,
     validates `serde_json::Value` only, so spans must be recovered from JSON pointers anyway; regex
     backtracking (`fancy-regex`) lets a plugin schema make loading slow.
3. **`boon` or `valico`.** Smaller or older; same span mapping work; `valico` is unmaintained.

## Decision
Option 1. KISS: the subset covers every schema the engine ships, and fail-closed compilation means a
schema we cannot evaluate is rejected loudly, never half-checked. Integers satisfy `"number"`;
`"integer"` accepts only TOML integers. `oneOf` reports, when no branch matches, the errors of the branch
with the fewest errors (ties: first branch), so messages stay specific.

## Consequences
- Easier: no new dependency or licence; plugin schemas cannot introduce regex cost; error ordering and text
  are ours and stable.
- Harder: plugin authors (M4) must stay within the subset; the SDK docs list it. Extending the subset is
  backward compatible (additive); each new keyword needs tests.
- Revisit in M4 if plugin authors need `pattern`/`format`/`anyOf` (add keywords, or reconsider option 2
  behind the same `CompiledSchema` API, which keeps callers unchanged).
