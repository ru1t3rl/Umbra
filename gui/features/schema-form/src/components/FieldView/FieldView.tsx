import type { ReactElement } from "react";
import type { FieldNode } from "../../model/fieldNode.ts";
import { ArrayField } from "../ArrayField/ArrayField.tsx";
import { BooleanField } from "../BooleanField/BooleanField.tsx";
import { EnumField } from "../EnumField/EnumField.tsx";
import { ExtrasGroup } from "../ExtrasGroup/ExtrasGroup.tsx";
import { JsonField } from "../JsonField/JsonField.tsx";
import { ObjectField } from "../ObjectField/ObjectField.tsx";
import { TextValueField } from "../TextValueField/TextValueField.tsx";

export interface FieldViewProps {
  node: FieldNode;
  idPrefix: string;
  /** The form root: no group of its own, the form element is the group. */
  bare?: boolean | undefined;
}

/** Picks the read-only view for one field node; objects recurse. */
export function FieldView({ node, idPrefix, bare }: FieldViewProps): ReactElement {
  switch (node.kind) {
    case "object": {
      const body = (
        <>
          {node.children.map((child) => (
            <FieldView key={child.key} node={child} idPrefix={idPrefix} />
          ))}
          <ExtrasGroup extras={node.extras} />
        </>
      );
      return bare ? (
        body
      ) : (
        <ObjectField node={node} idPrefix={idPrefix}>
          {body}
        </ObjectField>
      );
    }
    case "number":
    case "integer":
    case "string":
      return <TextValueField node={node} idPrefix={idPrefix} />;
    case "boolean":
      return <BooleanField node={node} idPrefix={idPrefix} />;
    case "enum":
      return <EnumField node={node} idPrefix={idPrefix} />;
    case "array":
      return <ArrayField node={node} idPrefix={idPrefix} />;
    case "json":
      return <JsonField node={node} idPrefix={idPrefix} bare={bare} />;
  }
}
