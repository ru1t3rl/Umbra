import { Badge, Card } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { useComponentsQuery } from "../../hooks/useComponentsQuery.ts";
import { t } from "../../messages.ts";
import { componentGroups } from "../../model/componentGroups.ts";
import { kindLabel } from "../../model/kindLabel.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { LoadingState } from "../LoadingState/LoadingState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import "./ComponentList.css";

/** The registered components, one Card per kind with the provenance of each (M8 WI-13). */
export function ComponentList(): ReactElement | null {
  const { idle, query } = useComponentsQuery();
  if (idle) return <IdleState />;
  const alert = query.isError ? (
    <ProblemAlert error={query.error} failed="components.failed" />
  ) : null;
  if (query.data === undefined) return alert ?? <LoadingState />;
  const groups = componentGroups(query.data);
  if (groups.length === 0)
    return (
      <>
        {alert}
        <p className="umbra-component-empty type-body">{t("components.empty")}</p>
      </>
    );
  return (
    <div className="umbra-component-list">
      {alert}
      {groups.map((group) => {
        const label = kindLabel(group.kind);
        return (
          <Card key={group.kind} title={label}>
            <ul
              className="umbra-component-entries"
              aria-label={t("components.list", { kind: label })}
            >
              {group.entries.map((entry) => (
                <li key={entry.id} className="umbra-component-entry">
                  <span className="umbra-component-id type-data">{entry.id}</span>
                  <span className="umbra-component-version type-data">{entry.version}</span>
                  <Badge tone={entry.provenance === "builtin" ? "neutral" : "info"}>
                    {entry.provenance}
                  </Badge>
                  <span className="umbra-component-description type-body">{entry.description}</span>
                </li>
              ))}
            </ul>
          </Card>
        );
      })}
    </div>
  );
}
