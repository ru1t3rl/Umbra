import type { EntitySource } from "@umbra/engine-stream";
import { EntityInspector } from "@umbra/entity-inspector";
import type { Affiliation } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./InspectorPanel.css";

export interface InspectorPanelProps {
  source: EntitySource;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  onCenter: (id: number) => void;
  affiliationOf: (force: string | null | undefined) => Affiliation;
}

/** The right floating panel, mounted only while an entity is selected. The landmark carries the stable name. */
export function InspectorPanel({
  source,
  selectedId,
  onSelect,
  onCenter,
  affiliationOf,
}: InspectorPanelProps): ReactElement | null {
  if (selectedId === null) return null;
  return (
    <aside className="dashboard-right" aria-label={t("inspector.title")}>
      <EntityInspector
        source={source}
        selectedId={selectedId}
        onSelect={onSelect}
        onCenter={onCenter}
        onClose={() => onSelect(null)}
        affiliationOf={affiliationOf}
      />
    </aside>
  );
}
