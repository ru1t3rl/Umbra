import type { ReactElement } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { t } from "../../messages.ts";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";
import { helpText } from "../../model/helpText.ts";

export interface ArrayFieldProps {
  node: Extract<FieldNode, { kind: "array" }>;
  idPrefix: string;
}

/** An array of primitives as a named list; items beyond the shown limit are counted. */
export function ArrayField({ node, idPrefix }: ArrayFieldProps): ReactElement {
  const id = fieldId(idPrefix, node.path);
  const help = helpText(node, helpMessages);
  return (
    <div className="umbra-schema-form-field">
      <span className="umbra-schema-form-label type-label" id={`${id}-label`}>
        {node.label}
      </span>
      <ul
        className="umbra-schema-form-items type-data"
        aria-labelledby={`${id}-label`}
        aria-describedby={help ? `${id}-help` : undefined}
      >
        {node.items.map((item, index) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a static, never reordered list; items may repeat
          <li key={`${index}:${item}`}>{item}</li>
        ))}
      </ul>
      {node.more > 0 ? (
        <p className="umbra-schema-form-note type-caption">
          {t("moreItems", { count: node.more })}
        </p>
      ) : null}
      {help ? (
        <p className="umbra-schema-form-help type-caption" id={`${id}-help`}>
          {help}
        </p>
      ) : null}
    </div>
  );
}
