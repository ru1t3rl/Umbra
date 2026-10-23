import { Button, TextField } from "@umbra/ui-kit";
import { type ReactElement, useRef } from "react";
import { useSpawnForm } from "../../hooks/useSpawnForm.ts";
import { type MessageKey, t } from "../../messages.ts";
import {
  FIELDS,
  type FieldName,
  type FieldProblem,
  NUMERIC_FIELDS,
} from "../../model/spawnForm.ts";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./SpawnForm.css";

export interface SpawnFormProps {
  templateId: string;
}

const LABEL = {
  lat: "spawn.lat",
  lon: "spawn.lon",
  alt: "spawn.alt",
  heading: "spawn.heading",
  speed: "spawn.speed",
  name: "spawn.name",
  force: "spawn.force",
  group: "spawn.group",
} as const satisfies Record<FieldName, MessageKey>;

function problemText(field: FieldName, problem: FieldProblem): string {
  const params = { field: t(LABEL[field]) };
  switch (problem.kind) {
    case "required":
      return t("spawn.required", params);
    case "number":
      return t("spawn.number", params);
    case "range":
      return t("spawn.range", { ...params, min: problem.min, max: problem.max });
  }
}

/** Spawns an entity from the template (private to the package, rendered by TemplateDetail, ASM-949). */
export function SpawnForm({ templateId }: SpawnFormProps): ReactElement {
  const formRef = useRef<HTMLFormElement>(null);
  const form = useSpawnForm(templateId, formRef);
  const { result } = form;
  return (
    <form
      ref={formRef}
      aria-label={t("spawn.label")}
      noValidate
      className="umbra-spawn-form"
      onSubmit={form.submit}
    >
      <TextField label={t("spawn.template")} value={templateId} readOnly mono />
      <div className="umbra-spawn-form-grid">
        {FIELDS.map((field) => {
          const problem = form.problems[field];
          const duplicate = field === "name" && form.duplicateName;
          const invalid = problem !== undefined || duplicate;
          const help = problem
            ? problemText(field, problem)
            : duplicate
              ? t("spawn.duplicate")
              : undefined;
          return (
            <TextField
              key={field}
              name={field}
              label={t(LABEL[field])}
              value={form.values[field]}
              onChange={(e) => form.setValue(field, e.target.value)}
              invalid={invalid}
              mono={NUMERIC_FIELDS.includes(field)}
              autoComplete="off"
              {...(NUMERIC_FIELDS.includes(field) ? { inputMode: "decimal" as const } : {})}
              {...(help ? { help } : {})}
            />
          );
        })}
      </div>
      <div className="umbra-spawn-form-actions">
        <Button type="submit" variant="filled" disabled={!form.enabled || form.pending}>
          {form.pending ? t("spawn.submitting") : t("spawn.submit")}
        </Button>
      </div>
      {result?.kind === "failed" ? (
        <ProblemAlert error={result.error} fallbackTitle={t("spawn.failed")} />
      ) : null}
      <p className="umbra-spawn-form-done type-body" role="status" aria-live="polite">
        {result?.kind === "spawned" ? t("spawn.done", { id: result.id }) : ""}
      </p>
    </form>
  );
}
