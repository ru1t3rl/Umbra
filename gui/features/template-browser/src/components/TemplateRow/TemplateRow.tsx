import type { TemplateSummary } from "@umbra/data";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./TemplateRow.css";

export interface TemplateRowProps {
  template: TemplateSummary;
}

/** The content of one list row: id and description, then motion model and behavior. Presentational only. */
export function TemplateRow({ template }: TemplateRowProps): ReactElement {
  return (
    <span className="umbra-template-row">
      <span className="umbra-template-row-top">
        <span className="umbra-template-row-id type-data">{template.id}</span>
        {template.description ? (
          <span className="umbra-template-row-desc type-body">{template.description}</span>
        ) : null}
      </span>
      <span className="umbra-template-row-meta type-caption">
        {template.motion} · {template.behavior ?? t("list.noBehavior")}
      </span>
    </span>
  );
}
