import { Toggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";
import { helpText } from "../../model/helpText.ts";

export interface BooleanFieldProps {
  node: Extract<FieldNode, { kind: "boolean" }>;
  idPrefix: string;
}

/** A boolean as a disabled switch. */
export function BooleanField({ node, idPrefix }: BooleanFieldProps): ReactElement {
  const help = helpText(node, helpMessages);
  const helpId = `${fieldId(idPrefix, node.path)}-help`;
  return (
    <div className="umbra-schema-form-field">
      <Toggle
        label={node.label}
        checked={node.checked}
        disabled
        {...(help ? { "aria-describedby": helpId } : {})}
      />
      {help ? (
        <p className="umbra-schema-form-help type-caption" id={helpId}>
          {help}
        </p>
      ) : null}
    </div>
  );
}
