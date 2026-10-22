import { Card } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { helpMessages } from "../../helpMessages.ts";
import { t } from "../../messages.ts";
import { fieldId } from "../../model/fieldId.ts";
import type { FieldNode } from "../../model/fieldNode.ts";
import { helpText } from "../../model/helpText.ts";

export interface JsonFieldProps {
  node: Extract<FieldNode, { kind: "json" }>;
  idPrefix: string;
  /** The form root: the form element is already the named group. */
  bare?: boolean | undefined;
}

/** The fallback for what the subset cannot show: a visible note and the value as pretty JSON. */
export function JsonField({ node, idPrefix, bare }: JsonFieldProps): ReactElement {
  const help = helpText(node, helpMessages);
  const helpId = `${fieldId(idPrefix, node.path)}-help`;
  const body = (
    <>
      <p className="umbra-schema-form-note type-caption">{t("shownAsJson")}</p>
      {help ? (
        <p className="umbra-schema-form-help type-caption" id={helpId}>
          {help}
        </p>
      ) : null}
      {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be keyboard focusable */}
      <pre className="umbra-schema-form-json type-data" tabIndex={0}>
        {node.json}
      </pre>
    </>
  );
  if (bare) return body;
  return (
    <fieldset className="umbra-schema-form-group" aria-describedby={help ? helpId : undefined}>
      <legend className="type-label">{node.label}</legend>
      <Card variant="outline">{body}</Card>
    </fieldset>
  );
}
