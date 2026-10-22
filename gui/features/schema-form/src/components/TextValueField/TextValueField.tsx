import { TextField } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";
import { helpText } from "../../model/helpText.ts";

export interface TextValueFieldProps {
  node: Extract<FieldNode, { kind: "number" | "integer" | "string" }>;
  idPrefix: string;
}

/** A number, integer or string as a read-only, disabled text field. */
export function TextValueField({ node, idPrefix }: TextValueFieldProps): ReactElement {
  const help = helpText(node, helpMessages);
  return (
    <TextField
      id={fieldId(idPrefix, node.path)}
      label={node.label}
      {...(help ? { help } : {})}
      value={node.text}
      mono={node.kind !== "string"}
      readOnly
      disabled
    />
  );
}
