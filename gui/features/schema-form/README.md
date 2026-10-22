# @umbra/schema-form

Foundation feature (tier `foundation`): a read-only parameter form rendered from a JSON Schema (ADR 0070, own renderer, no form library). Depends on `@umbra/ui-kit` only. M8b adds editing behind the same component.

## Public API
Only `SchemaForm` and `SchemaFormProps` (the model stays internal):

```tsx
<SchemaForm schema={component.params_schema} value={template.params} label="Motion parameters" idPrefix="tpl-motion" readOnly />
```

`SchemaFormProps = { schema: unknown; value: unknown; label: string; idPrefix: string; readOnly: true }`. `label` names the whole form (a `fieldset`); `idPrefix` must be unique per form on a page (every generated id is `${idPrefix}-` plus the escaped property path); `readOnly: false` is a type error in M8. The component imports its own CSS (ADR 0043 amendment 2026-10-22, human decision). `undefined` and `null` values mean "no parameters set". The component never throws, whatever the schema or value.

## Supported subset
| Schema | Rendered as |
|---|---|
| `type: object` + `properties` | named group (the root is the form itself), fields in schema order, `required` marked |
| `number`, `integer`, `string` | ui-kit `TextField`, readonly and disabled (mono for numbers) |
| `boolean` | disabled `Toggle` |
| `string` + `enum` (up to 5 values) | `ChipGroup` of disabled `Chip`s, the value pressed |
| `string` + `enum` (6 to 200) | disabled `Select` |
| `array` of `number`/`integer`/`string`/`boolean` | named list, at most 200 items, then "...and N more" |
| `$ref` to `#/$defs/<name>` or `#/definitions/<name>` | resolved; the property `title` wins over the target `title` |
| `title`, `description`, `default`, `required`, `minimum`/`maximum`/`exclusiveMinimum`/`exclusiveMaximum` | label, help, "Required", bounds ("min 0, max 1", "greater than 0"), "Default: d" |
| key suffix `_m`, `_s`, `_mps`, `_mps2`, `_deg`, `_dps`, `_hz` | "Unit: m/s" etc. in the help (ASM-818) |
| value keys the schema does not list | group "Other parameters", one group per key with its JSON |

Help text order: description, Required, Unit, bounds, then "Default: d" for a given value or "Using default: d" when the value is absent (the default is shown in the field). Annotations that do not change the shape (`examples`, `$schema`, `$id`, `additionalProperties`, `pattern`, `format`, `minLength`, `maxLength`, `minItems`, `maxItems`) are ignored.

## Fallback: nothing is silently dropped
Anything else is shown as a group with the note "Shown as JSON" and a `<pre>` of the value (or "(not set)"): `oneOf`, `anyOf`, `allOf`, `if`, `not`, `patternProperties`, `const`, numeric enums, `type` lists, a missing or unknown `type`, tuples, arrays of objects or arrays, a non-object root, a property schema that is not an object, an unresolved or recursive `$ref`, and a value whose type does not match its field (never coerced).

## Limits (hostile input, ASM-898)
- Object nesting deeper than 8 levels below the root becomes a JSON node.
- At most 1,000 field nodes. The rest becomes one JSON node (reason `budget`) listing the first 100 skipped properties and the count of the others; extras end with "N more keys not shown".
- Enums above 200 values are JSON; arrays show 200 items.
- A JSON view is cut at 20,000 characters ("… (truncated)"); deeper than 32 levels prints "[Too deep]", a cycle "[Circular]".
- A getter or Proxy that throws turns the affected subtree into JSON.
- A `$ref` already on its own chain is a recursive ref; the same ref used twice is fine. Only own properties are read (`__proto__` is never resolved).

Fields follow `Object.keys` order of `properties`, so integer-like keys ("1", "2") come first in numeric order, as in JavaScript.

## Layout
`src/model/` is pure (user-visible strings it embeds, such as bounds words and "(not set)", come from `src/messages.ts`) (`toFieldTree`, `unitFromKey`, `describeBounds`, `safeStringify`, `helpText`, `fieldId`); `src/components/<Name>/` has one component each (ADR 0046). Tests use the M6 built-in schemas in `test/fixtures/components.m6.json` (ASM-900).
