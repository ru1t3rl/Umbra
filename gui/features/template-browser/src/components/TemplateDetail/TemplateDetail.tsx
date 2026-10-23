import { useGetTemplate } from "@umbra/data";
import { SchemaForm } from "@umbra/schema-form";
import { Readout } from "@umbra/ui-kit";
import { type ReactElement, useId } from "react";
import { useComponentIndex } from "../../hooks/useComponentIndex.ts";
import { useScenarioIdle } from "../../hooks/useScenarioIdle.ts";
import { t } from "../../messages.ts";
import { paramsSections } from "../../model/paramsSections.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import { SpawnForm } from "../SpawnForm/SpawnForm.tsx";
import "./TemplateDetail.css";

export interface TemplateDetailProps {
  /** The template to show; null renders nothing. */
  templateId: string | null;
}

const SECTION_TITLE = {
  motion: "detail.motion",
  sensor: "detail.sensor",
  behavior: "detail.behavior",
} as const;

/**
 * One template: its inheritance chain, source file, a read-only parameter form per component (the schemas are
 * joined from `GET /components`, ASM-816) and the spawn form. The header and the spawn form belong to the
 * `templateId` prop, not to the loaded data, so switching to an uncached template keeps what the user typed.
 */
export function TemplateDetail({ templateId }: TemplateDetailProps): ReactElement | null {
  const idle = useScenarioIdle();
  const enabled = templateId !== null && !idle;
  const query = useGetTemplate(templateId ?? "", { query: { enabled } });
  const components = useComponentIndex(enabled);
  const prefix = useId();
  if (templateId === null) return null;
  if (idle) return <IdleState />;
  if (query.isError) return <ProblemAlert error={query.error} fallbackTitle={t("detail.failed")} />;
  const template = query.data;
  const sections =
    template && !components.pending ? paramsSections(template, components.index) : [];
  return (
    <div className="umbra-template-detail">
      <h2 className="type-title">{templateId}</h2>
      {template?.description ? <p className="type-body">{template.description}</p> : null}
      {template ? (
        <>
          <p className="type-label" id={`${prefix}-chain`}>
            {t("detail.chain")}
          </p>
          <ol className="umbra-template-detail-chain type-data" aria-labelledby={`${prefix}-chain`}>
            {template.chain.map((id, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: an ordered chain; the position is part of the identity
              <li key={`${i}-${id}`}>{id}</li>
            ))}
          </ol>
          <Readout label={t("detail.source")} value={template.source} />
        </>
      ) : (
        <p className="type-caption">{t("detail.loading")}</p>
      )}
      {sections.map((s) => (
        <section
          key={s.key}
          aria-labelledby={`${prefix}-${s.key}`}
          className="umbra-template-detail-section"
        >
          <h3 className="type-title-sm" id={`${prefix}-${s.key}`}>
            {t(SECTION_TITLE[s.kind], { id: s.id })}
          </h3>
          <SchemaForm
            schema={s.schema}
            value={s.params}
            label={t("detail.params", { id: s.id })}
            idPrefix={`${prefix}-${s.key}`}
            readOnly
          />
        </section>
      ))}
      {template && components.pending ? (
        <p className="type-caption">{t("detail.loadingParams")}</p>
      ) : null}
      <SpawnForm templateId={templateId} />
    </div>
  );
}
