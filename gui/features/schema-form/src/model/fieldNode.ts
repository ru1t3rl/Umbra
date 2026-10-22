/** Why a subtree is shown as JSON instead of a form control (ASM-898, ASM-899). */
export type JsonReason =
  | "oneOf"
  | "anyOf"
  | "allOf"
  | "if"
  | "not"
  | "patternProperties"
  | "tuple"
  | "recursive-ref"
  | "ref-chain"
  | "unresolved-ref"
  | "depth"
  | "budget"
  | "enum-too-large"
  | "type-mismatch"
  | "unsupported";

export interface FieldBase {
  /** Last element of `path`; "" for the root. */
  key: string;
  /** Property-name chain from the root. */
  path: string[];
  label: string;
  description?: string;
  required: boolean;
}

export interface ExtraEntry {
  key: string;
  json: string;
  /** A count of what was not shown, not a real key. */
  summary?: true;
}

export type FieldNode =
  | (FieldBase & { kind: "object"; children: FieldNode[]; extras: ExtraEntry[] })
  | (FieldBase & {
      kind: "number" | "integer";
      text: string;
      unit?: string;
      bounds?: string;
      defaultText?: string;
      fromDefault: boolean;
    })
  | (FieldBase & {
      kind: "string";
      text: string;
      defaultText?: string;
      fromDefault: boolean;
    })
  | (FieldBase & {
      kind: "boolean";
      checked: boolean;
      defaultText?: string;
      fromDefault: boolean;
    })
  | (FieldBase & {
      kind: "enum";
      options: string[];
      selected: string | undefined;
      control: "chips" | "select";
      defaultText?: string;
      fromDefault: boolean;
    })
  | (FieldBase & {
      kind: "array";
      items: string[];
      more: number;
      unit?: string;
      defaultText?: string;
      fromDefault: boolean;
    })
  | (FieldBase & {
      kind: "json";
      json: string;
      /** The visible note ("Shown as JSON"). */
      note: string;
      reason: JsonReason;
      defaultText?: string;
      fromDefault: boolean;
    });

/** Key of the budget node; no schema property can produce it. */
export const BUDGET_KEY = "\u0000budget";
/** Characters of JSON text one tree may hold; later JSON nodes say the text is not shown. */
export const MAX_TREE_JSON_CHARS = 200_000;
export const MAX_DEPTH = 8;
export const MAX_NODES = 1000;
export const MAX_ENUM = 200;
export const MAX_ARRAY_ITEMS = 200;
export const CHIP_LIMIT = 5;
