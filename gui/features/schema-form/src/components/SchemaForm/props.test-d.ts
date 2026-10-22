import type { SchemaFormProps } from "../../index.ts";

// Assumed: SchemaFormProps = { schema: unknown; value: unknown; label: string; idPrefix: string; readOnly: true }
export const ok: SchemaFormProps = {
  schema: {},
  value: undefined,
  label: "l",
  idPrefix: "p",
  readOnly: true,
};
export const editable: SchemaFormProps = {
  schema: {},
  value: 1,
  label: "l",
  idPrefix: "p",
  // @ts-expect-error M8 is read-only: `readOnly: false` is rejected until M8b adds editing
  readOnly: false,
};
// @ts-expect-error label and idPrefix are required (accessible name, unique ids)
export const missing: SchemaFormProps = { schema: {}, value: 1, readOnly: true };
