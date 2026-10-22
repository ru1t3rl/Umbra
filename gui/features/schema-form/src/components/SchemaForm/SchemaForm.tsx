import { type ReactElement, useMemo } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { fieldId } from "../../model/fieldId.ts";
import { helpText } from "../../model/helpText.ts";
import { toFieldTree } from "../../model/toFieldTree.ts";
import { FieldView } from "../FieldView/FieldView.tsx";
import "./SchemaForm.css";

export interface SchemaFormProps {
  /** A JSON Schema (the supported subset); anything else renders as JSON. */
  schema: unknown;
  /** The parameter values to show; `undefined` and `null` mean none are set. */
  value: unknown;
  /** Accessible name of the whole form. */
  label: string;
  /** Prefix of every generated id; must be unique among the forms on one page. */
  idPrefix: string;
  /** Read-only in M8; editing arrives in M8b. */
  readOnly: true;
}

/** Read-only parameter form rendered from a JSON Schema (ADR 0070). Never throws on any schema or value. */
export function SchemaForm({ schema, value, label, idPrefix }: SchemaFormProps): ReactElement {
  const { root } = useMemo(() => toFieldTree(schema, value), [schema, value]);
  // A root that fell back to JSON shows its help in the form group itself, so the group points at it.
  const rootHelp = root.kind === "json" && helpText(root, helpMessages);
  return (
    <fieldset
      className="umbra-schema-form"
      aria-label={label}
      aria-describedby={rootHelp ? `${fieldId(idPrefix, root.path)}-help` : undefined}
    >
      <FieldView node={root} idPrefix={idPrefix} bare />
    </fieldset>
  );
}
