import { Card } from "@umbra/ui-kit";
import type { ReactElement, ReactNode } from "react";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";

export interface ObjectFieldProps {
  node: Extract<FieldNode, { kind: "object" }>;
  idPrefix: string;
  children: ReactNode;
}

/** A nested object as a named group. */
export function ObjectField({ node, idPrefix, children }: ObjectFieldProps): ReactElement {
  const helpId = `${fieldId(idPrefix, node.path)}-help`;
  return (
    <fieldset
      className="umbra-schema-form-group"
      aria-describedby={node.description ? helpId : undefined}
    >
      <legend className="type-label">{node.label}</legend>
      <Card variant="outline">
        {node.description ? (
          <p className="umbra-schema-form-help type-caption" id={helpId}>
            {node.description}
          </p>
        ) : null}
        {children}
      </Card>
    </fieldset>
  );
}
