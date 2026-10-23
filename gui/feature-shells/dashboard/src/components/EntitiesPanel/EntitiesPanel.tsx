import type { EntitySource } from "@umbra/engine-stream";
import { EntityListPanel } from "@umbra/entity-inspector";
import { LogView } from "@umbra/log-view";
import { type Affiliation, Button, Panel, Tabs } from "@umbra/ui-kit";
import { type ReactElement, useId, useState } from "react";
import { t } from "../../messages.ts";
import { collapsedAtWidth } from "../../model/collapsedAtWidth.ts";
import "./EntitiesPanel.css";

type View = "entities" | "log";

const TABS = [
  { value: "entities", label: t("entities.tab.entities") },
  { value: "log", label: t("entities.tab.log") },
];

export interface EntitiesPanelProps {
  source: EntitySource;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  affiliationOf: (force: string | null | undefined) => Affiliation;
}

/**
 * The left floating panel: Tabs "Entities" and "Log", collapsible. It starts collapsed on a viewport of 1280 px or
 * less (read once at mount, ASM-973). Collapsed, the landmark and its toggle stay and the body is not rendered.
 */
export function EntitiesPanel({
  source,
  selectedId,
  onSelect,
  affiliationOf,
}: EntitiesPanelProps): ReactElement {
  const [collapsed, setCollapsed] = useState(() =>
    collapsedAtWidth(document.documentElement.clientWidth),
  );
  const [view, setView] = useState<View>("entities");
  const id = useId();
  const bodyId = `${id}-body`;
  return (
    <aside className="dashboard-left" aria-label={t("entities.title")} data-collapsed={collapsed}>
      <Panel
        title={t("entities.title")}
        width="100%"
        className="dashboard-left-panel"
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon={collapsed ? "chevron_right" : "chevron_left"}
            aria-label={t(collapsed ? "panel.expand" : "panel.collapse")}
            aria-expanded={!collapsed}
            aria-controls={bodyId}
            onClick={() => setCollapsed((c) => !c)}
          />
        }
      >
        {collapsed ? null : (
          <div className="dashboard-left-body" id={bodyId}>
            <Tabs
              id={id}
              items={TABS}
              value={view}
              label={t("entities.tabs")}
              onChange={(value) => setView(value === "log" ? "log" : "entities")}
            />
            <div
              className="dashboard-left-view"
              role="tabpanel"
              id={`${id}-panel-${view}`}
              aria-labelledby={`${id}-tab-${view}`}
            >
              {view === "log" ? (
                <LogView label={t("log.label")} compact />
              ) : (
                <EntityListPanel
                  source={source}
                  selectedId={selectedId}
                  onSelect={onSelect}
                  affiliationOf={affiliationOf}
                />
              )}
            </div>
          </div>
        )}
      </Panel>
    </aside>
  );
}
